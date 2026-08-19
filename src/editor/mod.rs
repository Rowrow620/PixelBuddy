pub mod clipboard;
pub mod history;
pub mod selection;

use crate::document::{AnimationFrame, AnimationManager, Document};
pub use clipboard::ClipboardBuffer;
use history::{Command, DocumentSnapshotCommand, History};
pub use selection::Selection;

#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum ToolType {
    Hand,
    Zoom,
    Marquee,
    Move,
    Pencil,
    Eraser,
    Line,
    Rectangle,
    Ellipse,
    Fill,
    Eyedropper,
}

pub struct EditorState {
    pub animation: AnimationManager,
    pub history: History,
    pub primary_color: [u8; 4],
    pub secondary_color: [u8; 4],
    pub active_tool: ToolType,
    pub selection: Selection,
    pub clipboard: Option<ClipboardBuffer>,
    /// Runtime-only clipboard for complete animation frames.
    ///
    /// This intentionally stays separate from [`Self::clipboard`], which is
    /// the pixel-selection clipboard used by the canvas. Copying a frame must
    /// never make a pending pixel paste disappear, or vice versa.
    frame_clipboard: Option<Box<AnimationFrame>>,
    /// Display name of the opened `.pbud` project, if it has one.
    ///
    /// This is intentionally runtime-only: the name comes from the file
    /// chosen by the user rather than from the project contents.
    pub project_name: Option<String>,
    /// Monotonically advancing runtime revision used to associate an
    /// asynchronous save result with the exact project contents it encoded.
    revision: u64,
    /// The runtime revision known to have reached persistent storage.
    saved_revision: u64,
}

impl EditorState {
    pub fn new(width: u32, height: u32) -> Self {
        let initial_doc = Document::new(width, height);
        Self {
            animation: AnimationManager::new(initial_doc),
            history: History::new(100),
            primary_color: [0, 0, 0, 255],
            secondary_color: [255, 255, 255, 255],
            active_tool: ToolType::Pencil,
            selection: Selection::new(),
            clipboard: None,
            frame_clipboard: None,
            project_name: None,
            revision: 0,
            saved_revision: 0,
        }
    }

    /// The document belonging to the selected animation frame.
    ///
    /// `AnimationManager` is the sole owner of frame documents. Keeping this
    /// accessor instead of a second `Document` field prevents UI edits from
    /// being silently overwritten when the frame changes.
    pub fn document(&self) -> &Document {
        self.animation.current_doc()
    }

    /// Mutably accesses the selected frame's document and marks the project
    /// as having unsaved changes.
    pub fn document_mut(&mut self) -> &mut Document {
        self.mark_dirty();
        self.animation.current_doc_mut()
    }

    pub fn select_frame(&mut self, index: usize) {
        if index >= self.animation.frames.len() || index == self.animation.current_frame_index {
            return;
        }

        self.animation.current_frame_index = index;
        self.history.clear();
        self.selection.deselect();
        self.mark_dirty();
    }

    /// Adds a blank frame after the selected frame and makes it active.
    pub fn add_frame(&mut self) {
        self.animation.add_frame();
        self.history.clear();
        self.selection.deselect();
        self.mark_dirty();
    }

    /// Duplicates the selected frame, selects the duplicate, and invalidates
    /// the index-based history until it has stable object identifiers.
    pub fn duplicate_frame(&mut self) {
        self.animation.duplicate_frame();
        self.history.clear();
        self.selection.deselect();
        self.mark_dirty();
    }

    /// Copies the selected animation frame into the runtime-only frame
    /// clipboard without affecting the pixel-selection clipboard or project
    /// dirty state.
    pub fn copy_current_frame(&mut self) {
        self.frame_clipboard = Some(Box::new(self.animation.current_frame().clone()));
    }

    /// Returns whether a complete animation frame is available to paste.
    pub fn has_copied_frame(&self) -> bool {
        self.frame_clipboard.is_some()
    }

    /// Pastes the copied animation frame immediately after the selected frame.
    ///
    /// Frame documents are not safely addressable by the active document-only
    /// history yet, so a successful structural animation change clears that
    /// history instead of leaving commands that could target the wrong frame.
    /// Returns `false` without changing state when no frame has been copied.
    pub fn paste_frame_after_current(&mut self) -> bool {
        let Some(frame) = self.frame_clipboard.as_deref().cloned() else {
            return false;
        };

        // Changing the frame order while playback is running would otherwise
        // leave a partially elapsed duration associated with a different
        // sequence. A later Play action starts with a fresh clock.
        self.animation.stop();
        self.animation.insert_frame_after_current(frame);
        self.history.clear();
        self.selection.deselect();
        self.mark_dirty();
        true
    }

    /// Moves an animation frame to its final zero-based position.
    ///
    /// The selected logical frame remains selected even when the move crosses
    /// it. Like paste, this is structural and therefore invalidates the
    /// document-only history until frame commands have stable identifiers.
    /// Returns `false` for invalid positions or a no-op move.
    pub fn move_frame(&mut self, from: usize, to: usize) -> bool {
        if !self.animation.move_frame(from, to) {
            return false;
        }

        self.animation.stop();
        self.history.clear();
        self.selection.deselect();
        self.mark_dirty();
        true
    }

    /// Removes the selected frame when another frame remains.
    pub fn remove_frame(&mut self) {
        let previous_count = self.animation.frames.len();
        self.animation.remove_frame();
        if self.animation.frames.len() != previous_count {
            self.history.clear();
            self.selection.deselect();
            self.mark_dirty();
        }
    }

    /// Advances playback and clears index-based history after a frame switch.
    pub fn update_animation_playback(&mut self, current_time: f64) -> bool {
        let advanced = self.animation.update_playback(current_time);
        if advanced {
            self.history.clear();
            self.selection.deselect();
        }
        advanced
    }

    /// Changes the shared animation rate and records it as a project edit.
    pub fn set_animation_fps(&mut self, fps: u32) {
        self.animation.set_fps(fps);
        self.mark_dirty();
    }

    /// Changes onion-skin visibility and records it as a project edit.
    pub fn set_onion_skin_enabled(&mut self, enabled: bool) {
        self.animation.onion_skin_enabled = enabled;
        self.mark_dirty();
    }

    /// Changes onion-skin opacity and records it as a project edit.
    pub fn set_onion_skin_opacity(&mut self, opacity: f32) {
        self.animation.onion_skin_opacity = opacity.clamp(0.0, 1.0);
        self.mark_dirty();
    }

    /// Replaces the current project content after importing a flattened image.
    ///
    /// Importing must replace the animation store too. Keeping previous frames
    /// would let a later GIF or sprite-sheet export use stale artwork.
    pub fn replace_document(&mut self, document: Document) {
        self.animation = AnimationManager::new(document);
        self.history.clear();
        self.selection.deselect();
        self.clipboard = None;
        self.frame_clipboard = None;
        self.project_name = None;
        self.mark_dirty();
    }

    /// Replaces the complete editable project after a validated `.pbud`
    /// file has been decoded. Runtime-only editor state is reset deliberately.
    pub fn replace_project(
        &mut self,
        mut animation: AnimationManager,
        project_name: Option<String>,
    ) {
        animation.stop();
        self.animation = animation;
        self.history.clear();
        self.selection.deselect();
        self.clipboard = None;
        self.frame_clipboard = None;
        self.project_name = project_name;
        self.reset_saved_revision();
    }

    /// Stops playback and loads frame zero without changing frame zero's data.
    pub fn stop_animation(&mut self) {
        self.animation.stop();
        self.select_frame(0);
    }

    /// Returns whether the project has changes that have not been saved to a
    /// `.pbud` file.
    pub fn is_dirty(&self) -> bool {
        self.revision != self.saved_revision
    }

    /// Returns the runtime revision for the current editable contents.
    ///
    /// It is deliberately not serialized: it only exists to prevent a delayed
    /// Save As completion from claiming a newer edit was persisted.
    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// Marks the project as modified. Callers that alter public animation
    /// fields directly should use this immediately after the mutation.
    pub fn mark_dirty(&mut self) {
        self.revision = self.revision.wrapping_add(1);
        // In the practically unreachable event that the counter wraps to the
        // saved revision, advance it once more so a real edit never appears
        // clean merely because of integer overflow.
        if self.revision == self.saved_revision {
            self.revision = self.revision.wrapping_add(1);
        }
    }

    /// Marks the current project contents as persisted successfully.
    pub fn mark_saved(&mut self) {
        self.saved_revision = self.revision;
    }

    fn reset_saved_revision(&mut self) {
        self.revision = 0;
        self.saved_revision = 0;
    }

    /// Marks the project saved only when an asynchronous write completed for
    /// the revision that is still currently displayed.
    ///
    /// Returns `false` if the user edited the project after that save request
    /// was created, leaving the newer changes dirty and eligible for recovery.
    pub fn mark_saved_if_current(&mut self, saved_revision: u64) -> bool {
        if self.revision != saved_revision {
            return false;
        }

        self.saved_revision = saved_revision;
        true
    }

    /// Changes the display name associated with the current project.
    pub fn set_project_name(&mut self, project_name: Option<String>) {
        self.project_name = project_name;
    }

    pub fn swap_colors(&mut self) {
        if self.primary_color != self.secondary_color {
            std::mem::swap(&mut self.primary_color, &mut self.secondary_color);
            self.mark_dirty();
        }
    }

    pub fn set_primary_color(&mut self, color: [u8; 4]) {
        if self.primary_color != color {
            self.primary_color = color;
            self.mark_dirty();
        }
    }

    pub fn set_secondary_color(&mut self, color: [u8; 4]) {
        if self.secondary_color != color {
            self.secondary_color = color;
            self.mark_dirty();
        }
    }

    pub fn set_active_tool(&mut self, tool: ToolType) {
        if self.active_tool != tool {
            self.active_tool = tool;
            self.mark_dirty();
        }
    }

    /// Applies one structural edit to the selected frame and records a
    /// document snapshot only when the closure reports that it changed data.
    ///
    /// Use this for layer and palette operations until history commands have
    /// stable object identifiers. The closure must return `true` exactly when
    /// it mutates the document; returning `false` intentionally avoids both
    /// an undo entry and a dirty-state change.
    pub fn mutate_document(
        &mut self,
        description: impl Into<String>,
        mutation: impl FnOnce(&mut Document) -> bool,
    ) -> bool {
        let before = self.document().clone();
        let changed = mutation(self.animation.current_doc_mut());
        if !changed {
            return false;
        }

        let after = self.document().clone();
        self.history
            .push_applied(Box::new(DocumentSnapshotCommand::new(
                description,
                before,
                after,
            )));
        self.mark_dirty();
        true
    }

    /// Push a command to history, executing it on the document.
    /// This method exists to avoid borrow-checker issues when calling
    /// history.push(&mut document) since both are fields of EditorState.
    pub fn push_command(&mut self, command: Box<dyn Command>) {
        let (history, animation) = (&mut self.history, &mut self.animation);
        history.push(command, animation.current_doc_mut());
        self.mark_dirty();
    }

    /// Undoes the latest active-frame command and reports whether a command
    /// was applied.
    pub fn undo(&mut self) -> bool {
        if !self.history.can_undo() {
            return false;
        }

        let (history, animation) = (&mut self.history, &mut self.animation);
        history.undo(animation.current_doc_mut());
        self.mark_dirty();
        true
    }

    /// Redoes the latest active-frame command and reports whether a command
    /// was applied.
    pub fn redo(&mut self) -> bool {
        if !self.history.can_redo() {
            return false;
        }

        let (history, animation) = (&mut self.history, &mut self.animation);
        history.redo(animation.current_doc_mut());
        self.mark_dirty();
        true
    }

    /// Jumps backward through the active frame's undo history.
    pub fn jump_to_undo_index(&mut self, target_idx: usize) -> bool {
        if target_idx >= self.history.undo_descriptions().len() {
            return false;
        }

        let (history, animation) = (&mut self.history, &mut self.animation);
        history.jump_to_undo_index(target_idx, animation.current_doc_mut());
        self.mark_dirty();
        true
    }
}

#[cfg(test)]
mod tests {
    use super::EditorState;
    use crate::document::Document;

    #[test]
    fn replace_document_resets_animation_to_imported_artwork() {
        let mut editor = EditorState::new(2, 2);
        let imported = Document::new(5, 3);

        editor.replace_document(imported);

        assert_eq!(editor.document().width, 5);
        assert_eq!(editor.document().height, 3);
        assert_eq!(editor.animation.frames.len(), 1);
        assert_eq!(editor.animation.current_doc().width, 5);
        assert_eq!(editor.animation.current_doc().height, 3);
        assert!(editor.is_dirty());
    }

    #[test]
    fn active_document_is_stored_only_in_the_selected_animation_frame() {
        let mut editor = EditorState::new(2, 2);
        editor
            .document_mut()
            .active_layer_mut()
            .canvas
            .set_pixel(1, 1, [1, 2, 3, 255]);

        assert_eq!(
            editor
                .animation
                .current_doc()
                .active_layer()
                .canvas
                .get_pixel(1, 1),
            [1, 2, 3, 255]
        );

        editor.duplicate_frame();
        editor
            .document_mut()
            .active_layer_mut()
            .canvas
            .set_pixel(0, 0, [9, 8, 7, 255]);
        editor.select_frame(0);

        assert_eq!(
            editor.document().active_layer().canvas.get_pixel(0, 0),
            [0, 0, 0, 0]
        );
        assert_eq!(
            editor.document().active_layer().canvas.get_pixel(1, 1),
            [1, 2, 3, 255]
        );
    }

    #[test]
    fn project_load_resets_runtime_state_and_marks_contents_clean() {
        let mut editor = EditorState::new(2, 2);
        editor
            .document_mut()
            .active_layer_mut()
            .canvas
            .set_pixel(0, 0, [1, 2, 3, 255]);
        let animation = editor.animation;

        let mut loaded = EditorState::new(1, 1);
        loaded.selection.set_rect(0, 0, 0, 0);
        loaded.clipboard = Some(crate::editor::ClipboardBuffer {
            pixels: vec![[0, 0, 0, 0]],
            width: 1,
            height: 1,
        });
        loaded.copy_current_frame();
        loaded.mark_dirty();
        loaded.replace_project(animation, Some("sprite.pbud".to_owned()));

        assert!(!loaded.is_dirty());
        assert_eq!(loaded.project_name.as_deref(), Some("sprite.pbud"));
        assert!(!loaded.selection.active);
        assert!(loaded.clipboard.is_none());
        assert!(!loaded.has_copied_frame());
        assert_eq!(
            loaded.document().active_layer().canvas.get_pixel(0, 0),
            [1, 2, 3, 255]
        );
    }

    #[test]
    fn mutate_document_records_only_reported_structural_changes() {
        let mut editor = EditorState::new(2, 2);

        assert!(!editor.mutate_document("No-op", |_| false));
        assert!(!editor.is_dirty());
        assert!(!editor.history.can_undo());

        assert!(editor.mutate_document("Add ink layer", |document| {
            document.add_layer();
            document.active_layer_mut().name = "Ink".to_owned();
            document.palette.add_color([12, 34, 56, 255]);
            true
        }));
        assert!(editor.is_dirty());
        assert_eq!(editor.document().layers.len(), 2);
        assert_eq!(editor.document().active_layer().name, "Ink");
        assert_eq!(
            editor.document().palette.selected_color(),
            [12, 34, 56, 255]
        );

        assert!(editor.undo());
        assert_eq!(editor.document().layers.len(), 1);
        assert_eq!(editor.document().palette.colors.len(), 24);

        assert!(editor.redo());
        assert_eq!(editor.document().layers.len(), 2);
        assert_eq!(editor.document().active_layer().name, "Ink");
        assert_eq!(
            editor.document().palette.selected_color(),
            [12, 34, 56, 255]
        );
    }

    #[test]
    fn renaming_a_layer_is_undoable() {
        let mut editor = EditorState::new(2, 2);

        assert!(editor.mutate_document("Rename layer", |document| {
            document.layers[0].name = "Ink".to_owned();
            true
        }));
        assert_eq!(editor.document().layers[0].name, "Ink");

        assert!(editor.undo());
        assert_eq!(editor.document().layers[0].name, "Layer 1");

        assert!(editor.redo());
        assert_eq!(editor.document().layers[0].name, "Ink");
    }

    #[test]
    fn copied_frames_preserve_a_snapshot_without_replacing_the_pixel_clipboard() {
        let mut editor = EditorState::new(2, 2);
        editor
            .document_mut()
            .active_layer_mut()
            .canvas
            .set_pixel(0, 0, [1, 2, 3, 255]);
        editor.animation.current_frame_mut().duration_ms = 175;
        editor.clipboard = Some(crate::editor::ClipboardBuffer {
            pixels: vec![[9, 8, 7, 255]],
            width: 1,
            height: 1,
        });
        editor.mark_saved();

        editor.copy_current_frame();
        assert!(editor.has_copied_frame());
        assert!(!editor.is_dirty());

        // Change the source after copying and leave an active document command
        // behind. Pasting must use the copied snapshot and invalidate that
        // frame-indexed command rather than applying it to the pasted frame.
        assert!(editor.mutate_document("Rename source frame", |document| {
            document.layers[0].name = "Changed source".to_owned();
            true
        }));
        editor.selection.set_rect(0, 0, 1, 1);
        assert!(editor.history.can_undo());

        assert!(editor.paste_frame_after_current());

        assert_eq!(editor.animation.frames.len(), 2);
        assert_eq!(editor.animation.current_frame_index, 1);
        assert_eq!(editor.animation.current_frame().duration_ms, 175);
        assert_eq!(editor.document().layers[0].name, "Layer 1");
        assert_eq!(
            editor.document().active_layer().canvas.get_pixel(0, 0),
            [1, 2, 3, 255]
        );
        assert!(!editor.selection.active);
        assert!(!editor.history.can_undo());
        assert!(editor.is_dirty());

        let pixel_clipboard = editor.clipboard.as_ref().expect("pixel clipboard");
        assert_eq!(pixel_clipboard.width, 1);
        assert_eq!(pixel_clipboard.height, 1);
        assert_eq!(pixel_clipboard.pixels, vec![[9, 8, 7, 255]]);
    }

    #[test]
    fn pasting_without_a_copied_frame_is_a_clean_no_op() {
        let mut editor = EditorState::new(2, 2);
        editor.selection.set_rect(0, 0, 1, 1);
        editor.mark_saved();

        assert!(!editor.has_copied_frame());
        assert!(!editor.paste_frame_after_current());

        assert_eq!(editor.animation.frames.len(), 1);
        assert!(editor.selection.active);
        assert!(!editor.is_dirty());
    }

    #[test]
    fn moving_frames_keeps_the_selected_frame_and_invalidates_document_history() {
        let mut editor = EditorState::new(1, 1);
        editor
            .document_mut()
            .active_layer_mut()
            .canvas
            .set_pixel(0, 0, [1, 0, 0, 255]);
        editor.animation.current_frame_mut().duration_ms = 100;

        editor.duplicate_frame();
        editor
            .document_mut()
            .active_layer_mut()
            .canvas
            .set_pixel(0, 0, [2, 0, 0, 255]);
        editor.animation.current_frame_mut().duration_ms = 200;

        editor.duplicate_frame();
        editor
            .document_mut()
            .active_layer_mut()
            .canvas
            .set_pixel(0, 0, [3, 0, 0, 255]);
        editor.animation.current_frame_mut().duration_ms = 300;

        editor.select_frame(1);
        editor.mark_saved();
        assert!(editor.mutate_document("Edit selected frame", |document| {
            document.layers[0].name = "Selected frame".to_owned();
            true
        }));
        editor.selection.set_rect(0, 0, 0, 0);
        editor.animation.toggle_play(1.0);
        assert!(editor.animation.is_playing);

        assert!(editor.move_frame(0, 2));

        let markers: Vec<_> = editor
            .animation
            .frames
            .iter()
            .map(|frame| frame.document.active_layer().canvas.get_pixel(0, 0)[0])
            .collect();
        let durations: Vec<_> = editor
            .animation
            .frames
            .iter()
            .map(|frame| frame.duration_ms)
            .collect();

        assert_eq!(markers, vec![2, 3, 1]);
        assert_eq!(durations, vec![200, 300, 100]);
        assert_eq!(editor.animation.current_frame_index, 0);
        assert_eq!(editor.document().layers[0].name, "Selected frame");
        assert!(!editor.animation.is_playing);
        assert!(!editor.selection.active);
        assert!(!editor.history.can_undo());
        assert!(editor.is_dirty());
    }

    #[test]
    fn invalid_frame_moves_do_not_change_editor_state() {
        let mut editor = EditorState::new(2, 2);
        editor.selection.set_rect(0, 0, 1, 1);
        editor.mark_saved();

        assert!(!editor.move_frame(0, 0));
        assert!(!editor.move_frame(1, 0));

        assert_eq!(editor.animation.frames.len(), 1);
        assert!(editor.selection.active);
        assert!(!editor.is_dirty());
    }

    #[test]
    fn persisted_preferences_and_frame_selection_mark_the_project_dirty() {
        let mut editor = EditorState::new(2, 2);

        editor.set_primary_color([0, 0, 0, 255]);
        assert!(!editor.is_dirty());
        editor.set_primary_color([1, 2, 3, 255]);
        assert!(editor.is_dirty());

        editor.mark_saved();
        editor.set_active_tool(super::ToolType::Line);
        assert!(editor.is_dirty());

        editor.duplicate_frame();
        editor.mark_saved();
        editor.select_frame(0);
        assert!(editor.is_dirty());
    }

    #[test]
    fn delayed_save_cannot_clear_a_newer_revision() {
        let mut editor = EditorState::new(2, 2);
        let initial_revision = editor.revision();

        editor.set_primary_color([1, 2, 3, 255]);
        let saved_request_revision = editor.revision();
        assert_ne!(saved_request_revision, initial_revision);
        editor.set_secondary_color([4, 5, 6, 255]);

        assert!(!editor.mark_saved_if_current(saved_request_revision));
        assert!(editor.is_dirty());
        let current_revision = editor.revision();
        assert!(editor.mark_saved_if_current(current_revision));
        assert!(!editor.is_dirty());
    }

    #[test]
    fn loading_a_project_resets_its_runtime_revision_to_a_clean_state() {
        let mut source = EditorState::new(2, 2);
        source.set_primary_color([1, 2, 3, 255]);
        let animation = source.animation;

        let mut loaded = EditorState::new(1, 1);
        loaded.set_primary_color([9, 9, 9, 255]);
        loaded.replace_project(animation, None);

        assert_eq!(loaded.revision(), 0);
        assert!(!loaded.is_dirty());
    }
}

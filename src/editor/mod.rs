pub mod clipboard;
pub mod history;
pub mod selection;

use crate::document::{AnimationFrame, AnimationManager, Document, Layer};
pub use clipboard::ClipboardBuffer;
use history::{Command, DocumentSnapshotCommand, History};
pub use selection::Selection;

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
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
    pub brush_size: u8,
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
        Self::with_animation(AnimationManager::new(Document::new(width, height)))
    }

    fn with_animation(animation: AnimationManager) -> Self {
        Self {
            animation,
            history: History::new(100),
            primary_color: [0, 0, 0, 255],
            secondary_color: [255, 255, 255, 255],
            active_tool: ToolType::Pencil,
            brush_size: 1,
            selection: Selection::new(),
            clipboard: None,
            frame_clipboard: None,
            project_name: None,
            revision: 0,
            saved_revision: 0,
        }
    }

    /// Starts an unnamed project from flattened raster artwork without
    /// inheriting serialized editor preferences from the discarded project.
    pub(crate) fn from_imported_document(document: Document) -> Self {
        let mut editor = Self::with_animation(AnimationManager::new(document));
        editor.mark_dirty();
        editor
    }

    /// Starts an unnamed project from imported raster animation frames without
    /// inheriting project preferences or runtime state from the old editor.
    pub(crate) fn from_imported_animation(mut animation: AnimationManager) -> Self {
        animation.stop();
        let mut editor = Self::with_animation(animation);
        editor.mark_dirty();
        editor
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
        self.pause_animation_for_editing();
        self.mark_dirty();
        self.animation.current_doc_mut()
    }

    /// Selects a frame for editing and applies every model-owned transition
    /// invariant. App/UI callers must additionally synchronize canvas
    /// transients and GPU caches through `PixelBuddyApp::select_frame`.
    ///
    /// A manual selection stops playback. Same-frame and invalid requests are
    /// complete no-ops and return `false`.
    pub fn select_frame(&mut self, index: usize) -> bool {
        if index >= self.animation.frames.len() {
            return false;
        }

        let displayed_before = self.animation.current_frame_index;
        let selected_before = self.animation.selected_frame_index();
        if index == displayed_before && index == selected_before {
            return false;
        }

        self.animation.stop();
        if index != selected_before {
            let changed = self.animation.select_frame(index);
            debug_assert!(changed, "the editing selection differs from the target");
            self.mark_dirty();
        }
        self.clear_active_frame_runtime();
        true
    }

    /// Adds a blank frame after the selected frame and makes it active.
    pub fn add_frame(&mut self) -> bool {
        if self.animation.frames.len() >= crate::document::animation::MAX_ANIMATION_FRAMES {
            return false;
        }
        self.animation.pause_at_current_frame();
        self.animation.add_frame();
        self.clear_active_frame_runtime();
        self.mark_dirty();
        true
    }

    /// Duplicates the selected frame, selects the duplicate, and invalidates
    /// the index-based history until it has stable object identifiers.
    pub fn duplicate_frame(&mut self) -> bool {
        if self.animation.frames.len() >= crate::document::animation::MAX_ANIMATION_FRAMES {
            return false;
        }
        self.animation.pause_at_current_frame();
        self.animation.duplicate_frame();
        self.clear_active_frame_runtime();
        self.mark_dirty();
        true
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
        if self.animation.frames.len() >= crate::document::animation::MAX_ANIMATION_FRAMES {
            return false;
        }
        let Some(frame) = self.frame_clipboard.as_deref().cloned() else {
            return false;
        };

        // Changing the frame order while playback is running would otherwise
        // leave a partially elapsed duration associated with a different
        // sequence. A later Play action starts with a fresh clock.
        self.animation.pause_at_current_frame();
        self.animation.insert_frame_after_current(frame);
        self.clear_active_frame_runtime();
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
        if from >= self.animation.frames.len() || to >= self.animation.frames.len() || from == to {
            return false;
        }

        self.animation.pause_at_current_frame();
        let changed = self.animation.move_frame(from, to);
        debug_assert!(changed, "the frame move was validated above");
        self.clear_active_frame_runtime();
        self.mark_dirty();
        true
    }

    /// Removes the selected frame when another frame remains.
    pub fn remove_frame(&mut self) {
        let previous_count = self.animation.frames.len();
        if previous_count <= 1 {
            return;
        }

        self.animation.pause_at_current_frame();
        self.animation.remove_frame();
        if self.animation.frames.len() != previous_count {
            self.clear_active_frame_runtime();
            self.mark_dirty();
        }
    }

    /// Advances playback and clears index-based history after a frame switch.
    pub fn update_animation_playback(&mut self, current_time: f64) -> bool {
        let advanced = self.animation.update_playback(current_time);
        if advanced {
            self.clear_active_frame_runtime();
        }
        advanced
    }

    /// Starts preview playback or pauses on the currently previewed frame.
    /// Pausing adopts that frame as the persisted editing selection and marks
    /// dirty once when it differs from the selection playback started from.
    pub fn toggle_animation_playback(&mut self, current_time: f64) -> bool {
        let selection_changed = self.animation.toggle_play(current_time);
        if selection_changed {
            self.mark_dirty();
        }
        selection_changed
    }

    /// Stops preview playback while retaining the visible frame for an edit.
    pub fn pause_animation_for_editing(&mut self) -> bool {
        if !self.animation.is_playing {
            return false;
        }

        let selection_changed = self.animation.pause_at_current_frame();
        if selection_changed {
            self.mark_dirty();
        }
        selection_changed
    }

    fn clear_active_frame_runtime(&mut self) {
        self.history.clear();
        self.selection.deselect();
    }

    /// Selects a layer in only the current frame. Layer selection is persisted
    /// per frame but does not alter artwork or frame-local history.
    pub fn select_layer_current_frame(&mut self, index: usize) -> bool {
        if index >= self.document().layers.len() || index == self.document().active_layer_index {
            return false;
        }
        self.pause_animation_for_editing();
        self.animation.current_doc_mut().active_layer_index = index;
        self.mark_dirty();
        true
    }

    pub fn select_palette_color_current_frame(&mut self, index: usize) -> bool {
        if index >= self.document().palette.colors.len()
            || index == self.document().palette.selected_index
        {
            return false;
        }
        self.pause_animation_for_editing();
        self.animation.current_doc_mut().palette.set_selected(index);
        self.mark_dirty();
        true
    }

    pub fn create_animation_tag(&mut self, tag: crate::document::animation::FrameTag) -> bool {
        if self.animation.tags.len() >= crate::document::animation::MAX_ANIMATION_TAGS
            || tag.validate(self.animation.frames.len()).is_err()
        {
            return false;
        }
        self.animation.tags.push(tag);
        self.mark_dirty();
        true
    }

    pub fn update_animation_tag(
        &mut self,
        index: usize,
        tag: crate::document::animation::FrameTag,
    ) -> bool {
        if tag.validate(self.animation.frames.len()).is_err()
            || self.animation.tags.get(index) == Some(&tag)
        {
            return false;
        }
        let Some(existing) = self.animation.tags.get_mut(index) else {
            return false;
        };
        *existing = tag;
        self.mark_dirty();
        true
    }

    pub fn remove_animation_tag(&mut self, index: usize) -> bool {
        if index >= self.animation.tags.len() {
            return false;
        }
        self.animation.tags.remove(index);
        self.mark_dirty();
        true
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

    /// Clears state that is meaningful only for the project instance being
    /// replaced. App-level replacement calls this for every source so even an
    /// `EditorState` supplied by a future loader cannot carry runtime history,
    /// selections, clipboards, or playback into the active project.
    pub(crate) fn reset_runtime_state_for_replacement(&mut self) {
        self.animation.stop();
        self.history.clear();
        self.selection = Selection::new();
        self.clipboard = None;
        self.frame_clipboard = None;
    }

    /// Stops playback and loads frame zero without changing frame zero's data.
    pub fn stop_animation(&mut self) -> bool {
        let displayed_before = self.animation.current_frame_index;
        let selected_before = self.animation.selected_frame_index();
        self.animation.stop();
        let selection_changed = selected_before != 0;
        if selection_changed {
            let changed = self.animation.select_frame(0);
            debug_assert!(
                changed,
                "a nonzero editing selection must move to frame zero"
            );
            self.mark_dirty();
        }

        let frame_changed = displayed_before != 0 || selection_changed;
        if frame_changed {
            self.clear_active_frame_runtime();
        }
        frame_changed
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

    /// Resizes every animation frame as one persisted project mutation.
    pub fn resize_animation(
        &mut self,
        width: u32,
        height: u32,
    ) -> Result<bool, crate::document::LayerError> {
        if self
            .animation
            .frames
            .iter()
            .all(|frame| frame.document.width == width && frame.document.height == height)
        {
            return Ok(false);
        }

        self.animation.try_resize(width, height)?;
        self.pause_animation_for_editing();
        self.history.clear();
        self.selection.deselect();
        self.mark_dirty();
        Ok(true)
    }

    /// Adds the same empty layer to every frame so animation layer topology
    /// remains aligned. The operation intentionally clears frame-local history.
    pub fn add_layer_all_frames(&mut self) -> bool {
        if self
            .animation
            .frames
            .iter()
            .any(|frame| frame.document.layers.len() >= crate::document::MAX_LAYERS_PER_FRAME)
        {
            return false;
        }
        let layer_count = self.document().layers.len();
        let name = format!("Layer {}", layer_count + 1);
        let width = self.document().width;
        let height = self.document().height;

        self.pause_animation_for_editing();
        for frame in &mut self.animation.frames {
            frame
                .document
                .layers
                .push(Layer::new(name.clone(), width, height));
        }
        self.animation.current_doc_mut().active_layer_index = layer_count;
        self.history.clear();
        self.mark_dirty();
        true
    }

    /// Duplicates the selected layer in every frame where it exists.
    pub fn duplicate_active_layer_all_frames(&mut self) -> bool {
        if self
            .animation
            .frames
            .iter()
            .any(|frame| frame.document.layers.len() >= crate::document::MAX_LAYERS_PER_FRAME)
        {
            return false;
        }
        let active = self.document().active_layer_index;
        self.pause_animation_for_editing();
        let mut changed = false;
        for frame in &mut self.animation.frames {
            if let Some(layer) = frame.document.layers.get(active).cloned() {
                let mut copy = layer;
                copy.name = format!("{} copy", copy.name);
                frame.document.layers.insert(active + 1, copy);
                changed = true;
            }
        }
        if !changed {
            return false;
        }

        self.animation.current_doc_mut().active_layer_index = active + 1;
        self.history.clear();
        self.mark_dirty();
        true
    }

    /// Removes the selected layer from every frame where it exists.
    pub fn remove_active_layer_all_frames(&mut self) -> bool {
        let active = self.document().active_layer_index;
        if self.document().layers.len() <= 1 {
            return false;
        }

        self.pause_animation_for_editing();
        let mut changed = false;
        for frame in &mut self.animation.frames {
            if frame.document.layers.len() > 1 && active < frame.document.layers.len() {
                frame.document.layers.remove(active);
                if frame.document.active_layer_index >= frame.document.layers.len() {
                    frame.document.active_layer_index = frame.document.layers.len() - 1;
                }
                changed = true;
            }
        }
        if !changed {
            return false;
        }

        self.history.clear();
        self.mark_dirty();
        true
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
        self.pause_animation_for_editing();
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
        self.pause_animation_for_editing();
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

        self.pause_animation_for_editing();
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

        self.pause_animation_for_editing();
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

        self.pause_animation_for_editing();
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
    fn switching_frames_discards_cross_frame_history_without_touching_pixels() {
        let mut editor = EditorState::new(1, 1);
        editor.add_frame();
        assert!(editor.select_frame(0));

        assert!(editor.mutate_document("Paint red", |document| {
            document
                .active_layer_mut()
                .canvas
                .set_pixel(0, 0, [255, 0, 0, 255]);
            true
        }));
        assert!(editor.mutate_document("Paint blue", |document| {
            document
                .active_layer_mut()
                .canvas
                .set_pixel(0, 0, [0, 0, 255, 255]);
            true
        }));
        assert!(editor.undo());
        assert!(editor.history.can_undo());
        assert!(editor.history.can_redo());
        editor.selection.set_rect(0, 0, 0, 0);

        assert!(editor.select_frame(1));

        assert!(!editor.history.can_undo());
        assert!(!editor.history.can_redo());
        assert!(!editor.selection.active);
        assert!(!editor.undo());
        assert!(!editor.redo());
        assert_eq!(
            editor.document().active_layer().canvas.get_pixel(0, 0),
            [0, 0, 0, 0]
        );

        assert!(editor.select_frame(0));
        assert_eq!(
            editor.document().active_layer().canvas.get_pixel(0, 0),
            [255, 0, 0, 255]
        );
    }

    #[test]
    fn same_or_invalid_frame_selection_is_a_complete_no_op() {
        let mut editor = EditorState::new(2, 2);
        assert!(editor.mutate_document("Rename once", |document| {
            document.active_layer_mut().name = "Ink".to_owned();
            true
        }));
        assert!(editor.mutate_document("Rename twice", |document| {
            document.active_layer_mut().name = "Linework".to_owned();
            true
        }));
        assert!(editor.undo());
        editor.selection.set_rect(0, 0, 1, 1);
        editor.mark_saved();

        let before_revision = editor.revision();
        let before_undo = editor.history.undo_descriptions();
        let before_redo = editor.history.redo_descriptions();
        let before = crate::io::project::encode_editor_bytes(&editor)
            .expect("the editor should encode before a no-op selection");

        assert!(!editor.select_frame(0));
        assert!(!editor.select_frame(1));

        assert_eq!(editor.animation.current_frame_index, 0);
        assert_eq!(editor.revision(), before_revision);
        assert!(!editor.is_dirty());
        assert!(editor.selection.active);
        assert_eq!(editor.history.undo_descriptions(), before_undo);
        assert_eq!(editor.history.redo_descriptions(), before_redo);
        assert_eq!(
            crate::io::project::encode_editor_bytes(&editor)
                .expect("the editor should encode after a no-op selection"),
            before
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
        assert_eq!(editor.document().palette.colors.len(), 16);

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

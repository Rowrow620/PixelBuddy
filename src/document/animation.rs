use crate::document::{Canvas, Document, Layer};

/// Lowest and highest playback rates exposed by the timeline.
pub const MIN_FPS: u32 = 1;
pub const MAX_FPS: u32 = 30;
/// Maximum frames retained by one editable animation/project.
pub const MAX_ANIMATION_FRAMES: usize = 4_096;
/// Maximum tags retained by one editable animation/project.
pub const MAX_ANIMATION_TAGS: usize = 1_024;
/// Tag names are bounded in both Unicode scalar count and encoded UTF-8 bytes.
pub const MAX_TAG_NAME_CHARS: usize = 64;
pub const MAX_TAG_NAME_BYTES: usize = 128;
const MAX_PLAYBACK_CATCH_UP_STEPS: usize = 120;

#[derive(Clone)]
pub struct AnimationFrame {
    pub document: Document,
    pub duration_ms: u32,
}

impl AnimationFrame {
    pub fn new(doc: Document) -> Self {
        Self::with_duration(doc, AnimationManager::frame_duration_ms_for_fps(8))
    }

    pub fn with_duration(doc: Document, duration_ms: u32) -> Self {
        Self {
            document: doc,
            // A zero-duration frame would make playback's catch-up loop never
            // advance time. Keep imported or hand-built frames safe too.
            duration_ms: duration_ms.max(1),
        }
    }
}

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct FrameTag {
    pub name: String,
    pub color: [f32; 3],
    pub from_frame: usize,
    pub to_frame: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FrameTagValidationError {
    EmptyName,
    NameTooLong,
    ControlCharacter,
    InvalidColor,
    InvalidRange,
}

impl FrameTag {
    pub fn validate(&self, frame_count: usize) -> Result<(), FrameTagValidationError> {
        let trimmed = self.name.trim();
        if trimmed.is_empty() {
            return Err(FrameTagValidationError::EmptyName);
        }
        if trimmed.len() > MAX_TAG_NAME_BYTES || trimmed.chars().count() > MAX_TAG_NAME_CHARS {
            return Err(FrameTagValidationError::NameTooLong);
        }
        if trimmed.chars().any(char::is_control) {
            return Err(FrameTagValidationError::ControlCharacter);
        }
        if self
            .color
            .iter()
            .any(|component| !component.is_finite() || !(0.0..=1.0).contains(component))
        {
            return Err(FrameTagValidationError::InvalidColor);
        }
        if self.from_frame > self.to_frame || self.to_frame >= frame_count {
            return Err(FrameTagValidationError::InvalidRange);
        }
        Ok(())
    }
}

/// Ordered animation frames and their runtime playback cursor.
///
/// Frame indices are positional. Async UI work must pair an index with the
/// app-level document session and frame generation before applying results.
pub struct AnimationManager {
    pub frames: Vec<AnimationFrame>,
    pub tags: Vec<FrameTag>,
    pub current_frame_index: usize,
    pub fps: u32,
    pub is_playing: bool,
    pub last_frame_time: f64,
    /// The editing selection that playback temporarily previews from.
    ///
    /// While present, `current_frame_index` is a runtime preview cursor and
    /// this value is the stable selection written to project files. Pausing
    /// adopts the preview cursor; stopping restores this origin.
    pub(crate) playback_origin_frame_index: Option<usize>,
    pub onion_skin_enabled: bool,
    pub onion_skin_opacity: f32,
}

impl AnimationManager {
    pub fn new(initial_doc: Document) -> Self {
        Self {
            frames: vec![AnimationFrame::new(initial_doc)],
            tags: Vec::new(),
            current_frame_index: 0,
            fps: 8,
            is_playing: false,
            last_frame_time: 0.0,
            playback_origin_frame_index: None,
            onion_skin_enabled: false,
            onion_skin_opacity: 0.35,
        }
    }

    pub fn try_resize(
        &mut self,
        new_width: u32,
        new_height: u32,
    ) -> Result<(), crate::document::LayerError> {
        let resized = self
            .frames
            .iter()
            .map(|frame| frame.document.try_resized(new_width, new_height))
            .collect::<Result<Vec<_>, _>>()?;
        for (frame, document) in self.frames.iter_mut().zip(resized) {
            frame.document = document;
        }
        Ok(())
    }

    /// Converts the global FPS control into the duration stored on GIF frames.
    ///
    /// GIF export and preview playback both consume `duration_ms`. Keeping the
    /// conversion here makes the timeline's FPS control apply to both rather
    /// than leaving preview and export at different speeds.
    pub fn frame_duration_ms_for_fps(fps: u32) -> u32 {
        let fps = fps.clamp(MIN_FPS, MAX_FPS);
        (1_000.0 / fps as f64).round().max(1.0) as u32
    }

    /// Sets a uniform timing for the animation.
    ///
    /// The public FPS control intentionally applies to every frame. Per-frame
    /// timing remains stored on frames so imported/project animations can use
    /// it later, and playback/export will still honor those values.
    pub fn set_fps(&mut self, fps: u32) {
        self.fps = fps.clamp(MIN_FPS, MAX_FPS);
        let duration_ms = Self::frame_duration_ms_for_fps(self.fps);
        for frame in &mut self.frames {
            frame.duration_ms = duration_ms;
        }
    }

    /// Restarts the elapsed-time measurement after a timing setting changes.
    pub fn reset_playback_clock(&mut self, current_time: f64) {
        self.last_frame_time = current_time;
    }

    pub fn current_frame(&self) -> &AnimationFrame {
        &self.frames[self.current_frame_index]
    }

    pub fn current_frame_mut(&mut self) -> &mut AnimationFrame {
        &mut self.frames[self.current_frame_index]
    }

    pub fn current_doc(&self) -> &Document {
        &self.frames[self.current_frame_index].document
    }

    pub fn current_doc_mut(&mut self) -> &mut Document {
        &mut self.frames[self.current_frame_index].document
    }

    /// Returns the frame selection that belongs to editable project state.
    /// During preview playback the displayed cursor can differ temporarily.
    pub(crate) fn selected_frame_index(&self) -> usize {
        self.playback_origin_frame_index
            .unwrap_or(self.current_frame_index)
    }

    pub fn add_frame(&mut self) {
        // A new frame belongs to the same project, so retain its layer stack,
        // palette, and active-layer choice while starting every layer empty.
        // Build fresh canvases instead of cloning and clearing pixel buffers,
        // which avoids briefly duplicating potentially large artwork.
        let source = self.current_doc();
        let new_doc = Document {
            layers: source
                .layers
                .iter()
                .map(|layer| Layer {
                    name: layer.name.clone(),
                    canvas: Canvas::new(source.width, source.height),
                    opacity: layer.opacity,
                    blend_mode: layer.blend_mode,
                    visible: layer.visible,
                    locked: layer.locked,
                })
                .collect(),
            active_layer_index: source.active_layer_index,
            palette: source.palette.clone(),
            width: source.width,
            height: source.height,
        };
        let idx = self.current_frame_index + 1;
        let duration_ms = Self::frame_duration_ms_for_fps(self.fps);
        self.frames
            .insert(idx, AnimationFrame::with_duration(new_doc, duration_ms));
        self.adjust_tags_for_insertion(idx);
        self.current_frame_index = idx;
    }

    fn adjust_tags_for_insertion(&mut self, insert_index: usize) {
        for tag in &mut self.tags {
            if tag.from_frame >= insert_index {
                // Inserting before a tag shifts the whole range.
                tag.from_frame += 1;
                tag.to_frame += 1;
            } else if insert_index <= tag.to_frame {
                // Only insertion strictly inside a range expands it. A frame
                // inserted immediately after the range is not a tag member.
                tag.to_frame += 1;
            }
        }
    }

    pub fn duplicate_frame(&mut self) {
        let cloned_doc = self.current_doc().clone();
        let idx = self.current_frame_index + 1;
        let duration_ms = self.current_frame().duration_ms.max(1);
        self.frames
            .insert(idx, AnimationFrame::with_duration(cloned_doc, duration_ms));
        self.adjust_tags_for_insertion(idx);
        self.current_frame_index = idx;
    }

    /// Inserts a complete frame immediately after the selected frame and
    /// selects the inserted copy.
    ///
    /// Unlike [`Self::add_frame`], this preserves the copied frame's pixels,
    /// layer structure, palette, and per-frame timing. The caller owns the
    /// clipboard policy; this method only performs the ordered frame-store
    /// mutation.
    pub fn insert_frame_after_current(&mut self, frame: AnimationFrame) {
        // Keep the manager resilient if a malformed runtime value reaches it:
        // inserting at the end is still valid and makes the new frame the
        // selected one instead of indexing past the frame vector.
        let insert_index = self
            .current_frame_index
            .saturating_add(1)
            .min(self.frames.len());
        self.frames.insert(
            insert_index,
            AnimationFrame::with_duration(frame.document, frame.duration_ms),
        );
        self.adjust_tags_for_insertion(insert_index);
        self.current_frame_index = insert_index;
    }

    /// Moves a frame to its final zero-based position.
    ///
    /// `to` names the index in the resulting frame order, rather than an
    /// insertion slot. For example, moving frame `0` to `2` changes
    /// `[A, B, C]` into `[B, C, A]`. The selected *frame* remains selected,
    /// even when another frame moves across it.
    ///
    /// Returns `false` without changing state for invalid positions or a
    /// no-op move.
    pub fn move_frame(&mut self, from: usize, to: usize) -> bool {
        if from >= self.frames.len() || to >= self.frames.len() || from == to {
            return false;
        }

        let selected_index = self.current_frame_index;
        let frame = self.frames.remove(from);
        self.frames.insert(to, frame);

        self.adjust_tags_for_move(from, to);

        self.current_frame_index = if selected_index == from {
            to
        } else if from < to && (from < selected_index && selected_index <= to) {
            selected_index - 1
        } else if to < from && (to <= selected_index && selected_index < from) {
            selected_index + 1
        } else {
            selected_index
        };

        true
    }

    fn adjust_tags_for_move(&mut self, from: usize, to: usize) {
        // Simple approach: we simulate the move by removing and inserting,
        // but semantically it's better to just shift tags.
        // Actually, if a frame moves within a tag, the tag bounds might not change,
        // or they might shift.
        // If it's too complex, we can use a simpler approach or just let bounds shift.
        // Let's do the exact math:
        // When frame at `from` is removed, things after `from` shift left.
        // Then it's inserted at `to`, things after `to` shift right.
        for tag in &mut self.tags {
            // Remove phase
            let mut tag_from = tag.from_frame;
            let mut tag_to = tag.to_frame;
            let mut frame_was_in_tag = false;

            if from >= tag_from && from <= tag_to {
                frame_was_in_tag = true;
                tag_to = tag_to.saturating_sub(1);
            } else if from < tag_from {
                tag_from = tag_from.saturating_sub(1);
                tag_to = tag_to.saturating_sub(1);
            }

            // Insert phase
            if to <= tag_from {
                tag_from += 1;
                tag_to += 1;
            } else if to <= tag_to + 1 && frame_was_in_tag {
                // If it was in the tag and inserted adjacent or inside, we expand the tag
                tag_to += 1;
            } else if to <= tag_to {
                tag_to += 1;
            }

            // If it was in the tag, and was moved outside of the tag...
            // the tag shrinks, which we handled in remove phase.

            tag.from_frame = tag_from;
            tag.to_frame = tag_to;
        }
    }

    pub fn remove_frame(&mut self) {
        if self.frames.len() > 1 {
            self.frames.remove(self.current_frame_index);
            self.adjust_tags_for_removal(self.current_frame_index);
            if self.current_frame_index >= self.frames.len() {
                self.current_frame_index = self.frames.len() - 1;
            }
        }
    }

    fn adjust_tags_for_removal(&mut self, remove_index: usize) {
        let mut i = 0;
        while i < self.tags.len() {
            let mut remove_tag = false;
            let tag = &mut self.tags[i];
            if remove_index < tag.from_frame {
                tag.from_frame = tag.from_frame.saturating_sub(1);
                tag.to_frame = tag.to_frame.saturating_sub(1);
            } else if remove_index >= tag.from_frame && remove_index <= tag.to_frame {
                if tag.from_frame == tag.to_frame {
                    remove_tag = true;
                } else {
                    tag.to_frame = tag.to_frame.saturating_sub(1);
                }
            }
            if remove_tag {
                self.tags.remove(i);
            } else {
                i += 1;
            }
        }
    }

    /// Changes the selected frame without applying editor-level transition
    /// policy. UI code must use `EditorState`/`PixelBuddyApp` instead so
    /// frame-local history and canvas transients cannot cross this boundary.
    pub(crate) fn select_frame(&mut self, index: usize) -> bool {
        if index >= self.frames.len() || index == self.current_frame_index {
            return false;
        }

        self.current_frame_index = index;
        true
    }

    /// Toggles preview playback.
    ///
    /// Returns `true` when pausing adopts a preview frame different from the
    /// prior editing selection. The editor uses that signal to mark the
    /// persisted selection dirty exactly once instead of on every preview tick.
    pub fn toggle_play(&mut self, current_time: f64) -> bool {
        // A one-frame animation cannot advance, so don't leave the app in a
        // repaint loop that looks like playback but never changes the canvas.
        if self.frames.len() <= 1 {
            self.stop();
            return false;
        }

        if self.is_playing {
            return self.pause_at_current_frame();
        }

        self.playback_origin_frame_index = Some(self.current_frame_index);
        self.is_playing = true;
        self.reset_playback_clock(current_time);
        false
    }

    /// Pauses while retaining the frame currently visible in the preview.
    pub(crate) fn pause_at_current_frame(&mut self) -> bool {
        let origin = self
            .playback_origin_frame_index
            .take()
            .unwrap_or(self.current_frame_index);
        self.is_playing = false;
        self.last_frame_time = 0.0;
        self.current_frame_index != origin
    }

    pub fn stop(&mut self) {
        if let Some(origin) = self.playback_origin_frame_index.take() {
            self.current_frame_index = origin.min(self.frames.len().saturating_sub(1));
        }
        self.is_playing = false;
        self.last_frame_time = 0.0;
    }

    pub fn update_playback(&mut self, current_time: f64) -> bool {
        if !self.is_playing {
            return false;
        }

        if self.frames.len() <= 1 {
            self.stop();
            return false;
        }

        // A clock reset (for example after the window regains focus) should
        // never fast-forward through every frame.
        if current_time < self.last_frame_time {
            self.reset_playback_clock(current_time);
            return false;
        }

        let mut advanced = false;
        let mut steps = 0;
        while steps < MAX_PLAYBACK_CATCH_UP_STEPS
            && current_time - self.last_frame_time
                >= self.current_frame().duration_ms.max(1) as f64 / 1_000.0
        {
            let duration_seconds = self.current_frame().duration_ms.max(1) as f64 / 1_000.0;
            self.last_frame_time += duration_seconds;
            self.current_frame_index = (self.current_frame_index + 1) % self.frames.len();
            advanced = true;
            steps += 1;
        }

        // Do not try to simulate hours of frames after a suspended window
        // resumes. The next tick starts fresh while retaining the frame we
        // reached during bounded catch-up.
        if steps == MAX_PLAYBACK_CATCH_UP_STEPS {
            self.reset_playback_clock(current_time);
        }

        advanced
    }
}

#[cfg(test)]
mod tests {
    use super::{AnimationFrame, AnimationManager, FrameTag};
    use crate::document::{BlendMode, Document};

    fn frame_with_marker(marker: u8, duration_ms: u32) -> AnimationFrame {
        let mut document = Document::new(1, 1);
        document
            .active_layer_mut()
            .canvas
            .set_pixel(0, 0, [marker, 0, 0, 255]);
        AnimationFrame::with_duration(document, duration_ms)
    }

    fn animation_with_nine_frames() -> AnimationManager {
        let mut animation = AnimationManager::new(Document::new(2, 2));
        for _ in 1..9 {
            animation.duplicate_frame();
        }
        animation
    }

    #[test]
    fn frame_insertion_preserves_exact_tag_membership() {
        let tag = FrameTag {
            name: "Run".to_owned(),
            color: [0.8, 0.2, 0.2],
            from_frame: 0,
            to_frame: 6,
        };

        let mut after = animation_with_nine_frames();
        after.tags.push(tag.clone());
        after.select_frame(6);
        after.add_frame();
        assert_eq!((after.tags[0].from_frame, after.tags[0].to_frame), (0, 6));

        let mut inside = animation_with_nine_frames();
        inside.tags.push(FrameTag {
            from_frame: 2,
            to_frame: 5,
            ..tag.clone()
        });
        inside.select_frame(3);
        inside.add_frame();
        assert_eq!((inside.tags[0].from_frame, inside.tags[0].to_frame), (2, 6));

        let mut before = animation_with_nine_frames();
        before.tags.push(FrameTag {
            from_frame: 2,
            to_frame: 5,
            ..tag
        });
        before.select_frame(0);
        before.add_frame();
        assert_eq!((before.tags[0].from_frame, before.tags[0].to_frame), (3, 6));
    }

    #[test]
    fn stop_does_not_change_the_selected_frame() {
        let mut animation = AnimationManager::new(Document::new(2, 2));
        animation.duplicate_frame();
        assert_eq!(animation.current_frame_index, 1);

        animation.stop();

        assert_eq!(animation.current_frame_index, 1);
        assert!(!animation.is_playing);
    }

    #[test]
    fn stop_restores_the_editing_selection_after_preview_advances() {
        let mut animation = AnimationManager::new(Document::new(2, 2));
        animation.duplicate_frame();
        animation.select_frame(0);
        animation.toggle_play(0.0);
        assert!(animation.update_playback(0.2));
        assert_eq!(animation.current_frame_index, 1);
        assert_eq!(animation.selected_frame_index(), 0);

        animation.stop();

        assert_eq!(animation.current_frame_index, 0);
        assert_eq!(animation.selected_frame_index(), 0);
        assert!(!animation.is_playing);
    }

    #[test]
    fn pause_adopts_the_preview_frame_as_the_editing_selection() {
        let mut animation = AnimationManager::new(Document::new(2, 2));
        animation.duplicate_frame();
        animation.select_frame(0);
        animation.toggle_play(0.0);
        assert!(animation.update_playback(0.2));

        assert!(animation.toggle_play(0.2));

        assert_eq!(animation.current_frame_index, 1);
        assert_eq!(animation.selected_frame_index(), 1);
        assert!(!animation.is_playing);
    }

    #[test]
    fn playback_clock_starts_at_toggle_time() {
        let mut animation = AnimationManager::new(Document::new(2, 2));
        animation.duplicate_frame();
        animation.toggle_play(10.0);

        assert!(!animation.update_playback(10.05));
        assert!(animation.update_playback(10.13));
    }

    #[test]
    fn fps_control_synchronizes_all_frame_durations() {
        let mut animation = AnimationManager::new(Document::new(2, 2));
        animation.duplicate_frame();
        animation.set_fps(20);

        assert_eq!(animation.fps, 20);
        assert!(animation.frames.iter().all(|frame| frame.duration_ms == 50));
    }

    #[test]
    fn playback_uses_frame_duration_not_a_separate_fps_interval() {
        let mut animation = AnimationManager::new(Document::new(2, 2));
        animation.duplicate_frame();
        animation.frames[0].duration_ms = 200;
        animation.frames[1].duration_ms = 100;
        animation.select_frame(0);
        animation.toggle_play(1.0);

        assert!(!animation.update_playback(1.19));
        assert!(animation.update_playback(1.21));
        assert_eq!(animation.current_frame_index, 1);
        assert!(!animation.update_playback(1.29));
        assert!(animation.update_playback(1.31));
        assert_eq!(animation.current_frame_index, 0);
    }

    #[test]
    fn one_frame_animation_does_not_start_playback() {
        let mut animation = AnimationManager::new(Document::new(2, 2));

        animation.toggle_play(10.0);

        assert!(!animation.is_playing);
    }

    #[test]
    fn new_frame_keeps_document_structure_but_clears_layer_pixels() {
        let mut document = Document::new(2, 1);
        document.active_layer_mut().name = "Background".to_owned();
        document.active_layer_mut().opacity = 0.4;
        document.active_layer_mut().blend_mode = BlendMode::Screen;
        document.active_layer_mut().visible = false;
        document.active_layer_mut().locked = true;
        document
            .active_layer_mut()
            .canvas
            .set_pixel(0, 0, [1, 2, 3, 255]);
        document.palette.add_color([4, 5, 6, 255]);
        document.add_layer();
        document.active_layer_mut().name = "Details".to_owned();
        document
            .active_layer_mut()
            .canvas
            .set_pixel(1, 0, [7, 8, 9, 255]);

        let mut animation = AnimationManager::new(document);
        animation.add_frame();
        let new_frame = animation.current_doc();

        assert_eq!(new_frame.active_layer_index, 1);
        assert_eq!(new_frame.palette.colors.last(), Some(&[4, 5, 6, 255]));
        assert_eq!(new_frame.layers[0].name, "Background");
        assert_eq!(new_frame.layers[0].opacity, 0.4);
        assert_eq!(new_frame.layers[0].blend_mode, BlendMode::Screen);
        assert!(!new_frame.layers[0].visible);
        assert!(new_frame.layers[0].locked);
        assert!(new_frame
            .layers
            .iter()
            .all(|layer| { layer.canvas.pixels().iter().all(|&channel| channel == 0) }));
    }

    #[test]
    fn inserting_a_copied_frame_preserves_its_content_and_timing() {
        let mut animation = AnimationManager::new(Document::new(1, 1));
        animation.frames[0] = frame_with_marker(1, 100);

        animation.insert_frame_after_current(frame_with_marker(9, 275));

        assert_eq!(animation.current_frame_index, 1);
        assert_eq!(animation.frames.len(), 2);
        assert_eq!(animation.current_frame().duration_ms, 275);
        assert_eq!(
            animation
                .current_doc()
                .active_layer()
                .canvas
                .get_pixel(0, 0),
            [9, 0, 0, 255]
        );
    }

    #[test]
    fn moving_a_frame_preserves_timing_and_the_selected_frame_identity() {
        let mut animation = AnimationManager::new(frame_with_marker(1, 100).document);
        animation.frames[0].duration_ms = 100;
        animation.frames.push(frame_with_marker(2, 200));
        animation.frames.push(frame_with_marker(3, 300));
        animation.select_frame(1);

        assert!(animation.move_frame(0, 2));

        let markers: Vec<_> = animation
            .frames
            .iter()
            .map(|frame| frame.document.active_layer().canvas.get_pixel(0, 0)[0])
            .collect();
        let durations: Vec<_> = animation
            .frames
            .iter()
            .map(|frame| frame.duration_ms)
            .collect();

        assert_eq!(markers, vec![2, 3, 1]);
        assert_eq!(durations, vec![200, 300, 100]);
        assert_eq!(animation.current_frame_index, 0);
        assert_eq!(
            animation
                .current_doc()
                .active_layer()
                .canvas
                .get_pixel(0, 0),
            [2, 0, 0, 255]
        );
    }

    #[test]
    fn moving_an_invalid_or_unchanged_frame_is_a_no_op() {
        let mut animation = AnimationManager::new(Document::new(1, 1));
        let original_index = animation.current_frame_index;

        assert!(!animation.move_frame(0, 0));
        assert!(!animation.move_frame(1, 0));

        assert_eq!(animation.frames.len(), 1);
        assert_eq!(animation.current_frame_index, original_index);
    }
}

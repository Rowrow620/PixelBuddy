use crate::document::{Canvas, Document, Layer};

/// Lowest and highest playback rates exposed by the timeline.
pub const MIN_FPS: u32 = 1;
pub const MAX_FPS: u32 = 30;
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

pub struct AnimationManager {
    pub frames: Vec<AnimationFrame>,
    pub current_frame_index: usize,
    pub fps: u32,
    pub is_playing: bool,
    pub last_frame_time: f64,
    pub onion_skin_enabled: bool,
    pub onion_skin_opacity: f32,
}

impl AnimationManager {
    pub fn new(initial_doc: Document) -> Self {
        Self {
            frames: vec![AnimationFrame::new(initial_doc)],
            current_frame_index: 0,
            fps: 8,
            is_playing: false,
            last_frame_time: 0.0,
            onion_skin_enabled: false,
            onion_skin_opacity: 0.35,
        }
    }

    pub fn resize(&mut self, new_width: u32, new_height: u32) {
        for frame in &mut self.frames {
            frame.document.resize(new_width, new_height);
        }
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
        self.current_frame_index = idx;
    }

    pub fn duplicate_frame(&mut self) {
        let cloned_doc = self.current_doc().clone();
        let idx = self.current_frame_index + 1;
        let duration_ms = self.current_frame().duration_ms.max(1);
        self.frames
            .insert(idx, AnimationFrame::with_duration(cloned_doc, duration_ms));
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

    pub fn remove_frame(&mut self) {
        if self.frames.len() > 1 {
            self.frames.remove(self.current_frame_index);
            if self.current_frame_index >= self.frames.len() {
                self.current_frame_index = self.frames.len() - 1;
            }
        }
    }

    pub fn select_frame(&mut self, index: usize) {
        if index < self.frames.len() {
            self.current_frame_index = index;
        }
    }

    pub fn toggle_play(&mut self, current_time: f64) {
        // A one-frame animation cannot advance, so don't leave the app in a
        // repaint loop that looks like playback but never changes the canvas.
        if self.frames.len() <= 1 {
            self.stop();
            return;
        }
        self.is_playing = !self.is_playing;
        self.reset_playback_clock(current_time);
    }

    pub fn stop(&mut self) {
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
    use super::{AnimationFrame, AnimationManager};
    use crate::document::{BlendMode, Document};

    fn frame_with_marker(marker: u8, duration_ms: u32) -> AnimationFrame {
        let mut document = Document::new(1, 1);
        document
            .active_layer_mut()
            .canvas
            .set_pixel(0, 0, [marker, 0, 0, 255]);
        AnimationFrame::with_duration(document, duration_ms)
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

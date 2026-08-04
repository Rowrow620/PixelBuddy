use crate::document::Document;

#[derive(Clone)]
pub struct AnimationFrame {
    pub document: Document,
    pub duration_ms: u32,
}

impl AnimationFrame {
    pub fn new(doc: Document) -> Self {
        Self {
            document: doc,
            duration_ms: 100,
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
        let (w, h) = (self.current_doc().width, self.current_doc().height);
        let new_doc = Document::new(w, h);
        let idx = self.current_frame_index + 1;
        self.frames.insert(idx, AnimationFrame::new(new_doc));
        self.current_frame_index = idx;
    }

    pub fn duplicate_frame(&mut self) {
        let cloned_doc = self.current_doc().clone();
        let idx = self.current_frame_index + 1;
        self.frames.insert(idx, AnimationFrame::new(cloned_doc));
        self.current_frame_index = idx;
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

    pub fn toggle_play(&mut self) {
        self.is_playing = !self.is_playing;
    }

    pub fn stop(&mut self) {
        self.is_playing = false;
        self.current_frame_index = 0;
    }

    pub fn update_playback(&mut self, current_time: f64) -> bool {
        if !self.is_playing || self.frames.len() <= 1 {
            return false;
        }

        let interval = 1.0 / (self.fps as f64);
        if current_time - self.last_frame_time >= interval {
            self.current_frame_index = (self.current_frame_index + 1) % self.frames.len();
            self.last_frame_time = current_time;
            return true;
        }
        false
    }
}

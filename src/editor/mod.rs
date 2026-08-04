pub mod history;
pub mod selection;
pub mod clipboard;

use crate::document::{Document, AnimationManager};
use history::{History, Command};
pub use selection::Selection;
pub use clipboard::ClipboardBuffer;

#[derive(Clone, Copy, Debug, PartialEq)]
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
    pub document: Document,
    pub animation: AnimationManager,
    pub history: History,
    pub primary_color: [u8; 4],
    pub secondary_color: [u8; 4],
    pub active_tool: ToolType,
    pub selection: Selection,
    pub clipboard: Option<ClipboardBuffer>,
}

impl EditorState {
    pub fn new(width: u32, height: u32) -> Self {
        let initial_doc = Document::new(width, height);
        Self {
            document: initial_doc.clone(),
            animation: AnimationManager::new(initial_doc),
            history: History::new(100),
            primary_color: [0, 0, 0, 255],
            secondary_color: [255, 255, 255, 255],
            active_tool: ToolType::Pencil,
            selection: Selection::new(),
            clipboard: None,
        }
    }

    pub fn save_current_frame(&mut self) {
        let idx = self.animation.current_frame_index;
        if idx < self.animation.frames.len() {
            self.animation.frames[idx].document = self.document.clone();
        }
    }

    pub fn select_frame(&mut self, index: usize) {
        if index < self.animation.frames.len() {
            self.save_current_frame();
            self.animation.current_frame_index = index;
            self.document = self.animation.frames[index].document.clone();
        }
    }

    pub fn swap_colors(&mut self) {
        std::mem::swap(&mut self.primary_color, &mut self.secondary_color);
    }
    
    pub fn set_primary_color(&mut self, color: [u8; 4]) {
        self.primary_color = color;
    }
    
    pub fn set_secondary_color(&mut self, color: [u8; 4]) {
        self.secondary_color = color;
    }
    
    pub fn set_active_tool(&mut self, tool: ToolType) {
        self.active_tool = tool;
    }

    /// Push a command to history, executing it on the document.
    /// This method exists to avoid borrow-checker issues when calling
    /// history.push(&mut document) since both are fields of EditorState.
    pub fn push_command(&mut self, command: Box<dyn Command>) {
        self.history.push(command, &mut self.document);
    }
}

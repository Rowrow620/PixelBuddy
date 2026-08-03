pub mod history;

use crate::document::Document;
use history::{History, Command};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ToolType {
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
    pub history: History,
    pub primary_color: [u8; 4],
    pub secondary_color: [u8; 4],
    pub active_tool: ToolType,
}

impl EditorState {
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            document: Document::new(width, height),
            history: History::new(100),
            primary_color: [0, 0, 0, 255],
            secondary_color: [255, 255, 255, 255],
            active_tool: ToolType::Pencil,
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

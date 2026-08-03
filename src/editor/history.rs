use crate::document::Document;

pub trait Command {
    fn execute(&mut self, doc: &mut Document);
    fn undo(&mut self, doc: &mut Document);
    fn description(&self) -> &str;
}

pub struct History {
    undo_stack: Vec<Box<dyn Command>>,
    redo_stack: Vec<Box<dyn Command>>,
    max_size: usize,
}

impl History {
    pub fn new(max_size: usize) -> Self {
        Self {
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            max_size,
        }
    }

    pub fn push(&mut self, mut command: Box<dyn Command>, doc: &mut Document) {
        command.execute(doc);
        self.undo_stack.push(command);
        self.redo_stack.clear();
        
        if self.undo_stack.len() > self.max_size {
            self.undo_stack.remove(0);
        }
    }

    pub fn undo(&mut self, doc: &mut Document) {
        if let Some(mut command) = self.undo_stack.pop() {
            command.undo(doc);
            self.redo_stack.push(command);
        }
    }

    pub fn redo(&mut self, doc: &mut Document) {
        if let Some(mut command) = self.redo_stack.pop() {
            command.execute(doc);
            self.undo_stack.push(command);
        }
    }

    pub fn can_undo(&self) -> bool {
        !self.undo_stack.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo_stack.is_empty()
    }
}

pub struct DrawCommand {
    layer_index: usize,
    /// (x, y, old_color, new_color)
    changes: Vec<(u32, u32, [u8; 4], [u8; 4])>,
}

impl DrawCommand {
    pub fn new(layer_index: usize, changes: Vec<(u32, u32, [u8; 4], [u8; 4])>) -> Self {
        Self { layer_index, changes }
    }
}

impl Command for DrawCommand {
    fn execute(&mut self, doc: &mut Document) {
        if self.layer_index < doc.layers.len() {
            let canvas = &mut doc.layers[self.layer_index].canvas;
            for &(x, y, _, new_color) in &self.changes {
                canvas.set_pixel(x, y, new_color);
            }
        }
    }

    fn undo(&mut self, doc: &mut Document) {
        if self.layer_index < doc.layers.len() {
            let canvas = &mut doc.layers[self.layer_index].canvas;
            for &(x, y, old_color, _) in self.changes.iter().rev() {
                canvas.set_pixel(x, y, old_color);
            }
        }
    }

    fn description(&self) -> &str {
        "Draw pixels"
    }
}

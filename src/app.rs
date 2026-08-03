use crate::editor::{EditorState, ToolType};
use crate::editor::history::DrawCommand;
use crate::tools;
use crate::io::{IoHandler, FileAction};
use egui::{ColorImage, TextureHandle, TextureOptions, TextureFilter};

pub struct PixelBuddyApp {
    pub editor: EditorState,
    pub zoom: f32,
    pub pan_offset: egui::Vec2,
    pub show_grid: bool,
    pub is_drawing: bool,
    pub stroke_points: Vec<(u32, u32)>,
    pub shape_start: Option<(i32, i32)>,
    pub preview_changes: Vec<tools::PixelChange>,
    pub canvas_texture: Option<TextureHandle>,
    pub texture_dirty: bool,
    pub show_new_dialog: bool,
    pub new_width: String,
    pub new_height: String,
    pub fill_tolerance: u8,
    pub fill_contiguous: bool,
    pub shape_filled: bool,
    pub io_handler: IoHandler,
}

impl PixelBuddyApp {
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            editor: EditorState::new(width, height),
            zoom: 8.0,
            pan_offset: egui::Vec2::ZERO,
            show_grid: true,
            is_drawing: false,
            stroke_points: Vec::new(),
            shape_start: None,
            preview_changes: Vec::new(),
            canvas_texture: None,
            texture_dirty: true,
            show_new_dialog: false,
            new_width: "64".to_string(),
            new_height: "64".to_string(),
            fill_tolerance: 0,
            fill_contiguous: true,
            shape_filled: false,
            io_handler: IoHandler::new(),
        }
    }

    pub fn update_texture(&mut self, ctx: &egui::Context) {
        if self.texture_dirty || self.canvas_texture.is_none() {
            let canvas = self.editor.document.composite_preview();
            let size = [canvas.width() as usize, canvas.height() as usize];
            let image = ColorImage::from_rgba_unmultiplied(size, canvas.pixels());
            self.canvas_texture = Some(ctx.load_texture(
                "canvas",
                image,
                TextureOptions {
                    magnification: TextureFilter::Nearest,
                    minification: TextureFilter::Nearest,
                    ..Default::default()
                }
            ));
            self.texture_dirty = false;
        }
    }

    /// Apply a set of pixel changes to the active layer, recording undo history.
    pub fn apply_tool_changes(&mut self, changes: Vec<tools::PixelChange>) {
        if changes.is_empty() { return; }
        let active_layer_index = self.editor.document.active_layer_index;
        let mut history_changes = Vec::new();
        {
            let layer = &self.editor.document.layers[active_layer_index];
            for &(x, y, new_color) in &changes {
                if layer.canvas.in_bounds(x as i32, y as i32) {
                    let old_color = layer.canvas.get_pixel(x, y);
                    if old_color != new_color {
                        history_changes.push((x, y, old_color, new_color));
                    }
                }
            }
        }
        if !history_changes.is_empty() {
            let cmd = Box::new(DrawCommand::new(active_layer_index, history_changes));
            // Use the EditorState helper to avoid split-borrow issues
            self.editor.push_command(cmd);
            self.texture_dirty = true;
        }
    }
}

impl eframe::App for PixelBuddyApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Handle I/O events
        while let Ok(action) = self.io_handler.receiver.try_recv() {
            match action {
                FileAction::OpenedImage(data) => {
                    if let Some(doc) = crate::io::png::import_png_to_document(&data) {
                        self.editor.document = doc;
                        self.editor.history = crate::editor::history::History::new(100);
                        self.pan_offset = egui::Vec2::ZERO;
                        self.zoom = 8.0;
                        self.texture_dirty = true;
                    }
                }
                FileAction::Exported => {
                    // Could add a toast notification here in the future
                }
            }
        }

        // Handle shortcuts
        if ctx.input(|i| i.modifiers.ctrl && !i.modifiers.shift && i.key_pressed(egui::Key::Z)) {
            self.editor.history.undo(&mut self.editor.document);
            self.texture_dirty = true;
        }
        if ctx.input(|i| i.modifiers.ctrl && (i.key_pressed(egui::Key::Y) || (i.modifiers.shift && i.key_pressed(egui::Key::Z)))) {
            self.editor.history.redo(&mut self.editor.document);
            self.texture_dirty = true;
        }
        if ctx.input(|i| i.key_pressed(egui::Key::X)) {
            self.editor.swap_colors();
        }
        
        let tools = [
            (egui::Key::B, ToolType::Pencil),
            (egui::Key::E, ToolType::Eraser),
            (egui::Key::L, ToolType::Line),
            (egui::Key::R, ToolType::Rectangle),
            (egui::Key::O, ToolType::Ellipse),
            (egui::Key::G, ToolType::Fill),
            (egui::Key::I, ToolType::Eyedropper),
        ];
        for (key, tool) in tools {
            if ctx.input(|i| i.key_pressed(key)) {
                self.editor.set_active_tool(tool);
            }
        }

        crate::ui::menu_bar::show(ctx, self);
        crate::ui::toolbar::show(ctx, self);
        crate::ui::layers_panel::show(ctx, self);
        crate::ui::palette_panel::show(ctx, self);
        crate::ui::canvas_view::show(ctx, self);

        if self.show_new_dialog {
            let mut open = true;
            egui::Window::new("New Document").open(&mut open).show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Width:");
                    ui.text_edit_singleline(&mut self.new_width);
                });
                ui.horizontal(|ui| {
                    ui.label("Height:");
                    ui.text_edit_singleline(&mut self.new_height);
                });
                ui.horizontal(|ui| {
                    if ui.button("Create").clicked() {
                        if let (Ok(w), Ok(h)) = (self.new_width.parse::<u32>(), self.new_height.parse::<u32>()) {
                            self.editor = EditorState::new(w, h);
                            self.texture_dirty = true;
                            self.show_new_dialog = false;
                        }
                    }
                    if ui.button("Cancel").clicked() {
                        self.show_new_dialog = false;
                    }
                });
            });
            if !open {
                self.show_new_dialog = false;
            }
        }

        self.update_texture(ctx);
    }
}

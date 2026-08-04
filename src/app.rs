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
    pub auto_fit_requested: bool,
    pub show_timeline: bool,
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
            auto_fit_requested: true,
            show_timeline: false,
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
        // Ensure texture is updated before rendering panels
        self.update_texture(ctx);
        if self.texture_dirty {
            ctx.request_repaint();
        }

        // Handle I/O events
        while let Ok(action) = self.io_handler.receiver.try_recv() {
            match action {
                FileAction::OpenedImage(data) => {
                    if let Some(doc) = crate::io::png::import_png_to_document(&data) {
                        self.editor.document = doc;
                        self.editor.history = crate::editor::history::History::new(100);
                        self.pan_offset = egui::Vec2::ZERO;
                        self.auto_fit_requested = true;
                        self.texture_dirty = true;
                    }
                }
                FileAction::Exported => {
                    // Could add a toast notification here in the future
                }
            }
        }

        // Handle animation playback stepping
        let current_time = ctx.input(|i| i.time);
        if self.editor.animation.update_playback(current_time) {
            self.editor.document = self.editor.animation.current_doc().clone();
            self.texture_dirty = true;
        }
        if self.editor.animation.is_playing {
            ctx.request_repaint();
        }

        // Handle shortcuts
        if ctx.input(|i| !i.modifiers.ctrl && i.key_pressed(egui::Key::Space)) {
            self.editor.save_current_frame();
            self.editor.animation.toggle_play();
        }
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
        if ctx.input(|i| i.modifiers.ctrl && i.key_pressed(egui::Key::D)) {
            self.editor.selection.deselect();
        }
        if ctx.input(|i| i.modifiers.ctrl && i.key_pressed(egui::Key::C)) {
            self.editor.clipboard = crate::editor::clipboard::ClipboardBuffer::copy(&self.editor.document, &self.editor.selection);
        }
        if ctx.input(|i| i.modifiers.ctrl && i.key_pressed(egui::Key::V)) {
            if let Some(buf) = &self.editor.clipboard.clone() {
                let mut changes = Vec::new();
                for y in 0..buf.height {
                    for x in 0..buf.width {
                        let idx = (y * buf.width + x) as usize;
                        let color = buf.pixels[idx];
                        if color[3] > 0 {
                            changes.push((x, y, color));
                        }
                    }
                }
                self.apply_tool_changes(changes);
            }
        }
        
        let tools = [
            (egui::Key::H, ToolType::Hand),
            (egui::Key::M, ToolType::Marquee),
            (egui::Key::V, ToolType::Move),
            (egui::Key::B, ToolType::Pencil),
            (egui::Key::E, ToolType::Eraser),
            (egui::Key::L, ToolType::Line),
            (egui::Key::R, ToolType::Rectangle),
            (egui::Key::O, ToolType::Ellipse),
            (egui::Key::G, ToolType::Fill),
            (egui::Key::I, ToolType::Eyedropper),
        ];
        for (key, tool) in tools {
            if ctx.input(|i| !i.modifiers.ctrl && i.key_pressed(key)) {
                self.editor.set_active_tool(tool);
            }
        }
        if ctx.input(|i| !i.modifiers.ctrl && !i.modifiers.shift && i.key_pressed(egui::Key::Z)) {
            self.editor.set_active_tool(ToolType::Zoom);
        }

        crate::ui::menu_bar::show(ctx, self);
        crate::ui::toolbar::show(ctx, self);
        crate::ui::layers_panel::show(ctx, self);
        if self.show_timeline {
            crate::ui::timeline_panel::show(ctx, self);
        }
        crate::ui::canvas_view::show(ctx, self);

        if self.show_new_dialog {
            let mut open = true;
            egui::Window::new("New Document").open(&mut open).resizable(false).show(ctx, |ui| {
                ui.label(egui::RichText::new("Presets").strong());
                ui.horizontal(|ui| {
                    for (label, w, h) in [("16×16", "16", "16"), ("32×32", "32", "32"), ("64×64", "64", "64"), ("128×128", "128", "128")] {
                        if ui.button(label).clicked() {
                            self.new_width = w.to_string();
                            self.new_height = h.to_string();
                        }
                    }
                });
                ui.separator();
                ui.horizontal(|ui| {
                    ui.label("Width:");
                    ui.text_edit_singleline(&mut self.new_width);
                });
                ui.horizontal(|ui| {
                    ui.label("Height:");
                    ui.text_edit_singleline(&mut self.new_height);
                });
                ui.separator();
                ui.horizontal(|ui| {
                    if ui.button("Create").clicked() {
                        if let (Ok(w), Ok(h)) = (self.new_width.parse::<u32>(), self.new_height.parse::<u32>()) {
                            self.editor = EditorState::new(w, h);
                            self.pan_offset = egui::Vec2::ZERO;
                            self.auto_fit_requested = true;
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

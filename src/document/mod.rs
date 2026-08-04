pub mod canvas;
pub mod layer;
pub mod palette;
pub mod animation;

pub use canvas::Canvas;
pub use layer::{BlendMode, Layer};
pub use palette::Palette;
pub use animation::{AnimationManager, AnimationFrame};

#[derive(Clone, Debug)]
pub struct Document {
    pub layers: Vec<Layer>,
    pub active_layer_index: usize,
    pub palette: Palette,
    pub width: u32,
    pub height: u32,
}

impl Document {
    pub fn new(width: u32, height: u32) -> Self {
        let layers = vec![Layer::new("Layer 1", width, height)];
        Self {
            layers,
            active_layer_index: 0,
            palette: Palette::default(),
            width,
            height,
        }
    }

    pub fn active_layer(&self) -> &Layer {
        &self.layers[self.active_layer_index]
    }

    pub fn active_layer_mut(&mut self) -> &mut Layer {
        &mut self.layers[self.active_layer_index]
    }

    pub fn add_layer(&mut self) {
        let name = format!("Layer {}", self.layers.len() + 1);
        self.layers.push(Layer::new(name, self.width, self.height));
        self.active_layer_index = self.layers.len() - 1;
    }

    pub fn remove_layer(&mut self, index: usize) {
        if self.layers.len() > 1 && index < self.layers.len() {
            self.layers.remove(index);
            if self.active_layer_index >= self.layers.len() {
                self.active_layer_index = self.layers.len() - 1;
            }
        }
    }

    pub fn duplicate_layer(&mut self, index: usize) {
        if index < self.layers.len() {
            let mut new_layer = self.layers[index].clone();
            new_layer.name = format!("{} copy", new_layer.name);
            self.layers.insert(index + 1, new_layer);
            self.active_layer_index = index + 1;
        }
    }

    pub fn move_layer(&mut self, from: usize, to: usize) {
        if from < self.layers.len() && to < self.layers.len() {
            let layer = self.layers.remove(from);
            self.layers.insert(to, layer);
            if self.active_layer_index == from {
                self.active_layer_index = to;
            } else if self.active_layer_index > from && self.active_layer_index <= to {
                self.active_layer_index -= 1;
            } else if self.active_layer_index < from && self.active_layer_index >= to {
                self.active_layer_index += 1;
            }
        }
    }

    pub fn flatten(&self) -> Canvas {
        self.composite_preview()
    }

    pub fn composite_preview(&self) -> Canvas {
        let mut final_canvas = Canvas::new(self.width, self.height);
        
        for layer in &self.layers {
            if !layer.visible {
                continue;
            }
            
            for y in 0..self.height {
                for x in 0..self.width {
                    let base_color = final_canvas.get_pixel(x, y);
                    let top_color = layer.canvas.get_pixel(x, y);
                    
                    if top_color[3] > 0 {
                        let blended = Layer::blend_mode_apply(base_color, top_color, layer.blend_mode, layer.opacity);
                        final_canvas.set_pixel(x, y, blended);
                    }
                }
            }
        }
        
        final_canvas
    }
}

use crate::app::PixelBuddyApp;
use crate::document::layer::BlendMode;
use egui::{
    ecolor::{Hsva, HsvaGamma},
    style::NumericColorSpace,
    Color32, Mesh, Pos2, Rect, Rgba, Stroke,
};

const PALETTE_SWATCH_SIZE: f32 = 22.0;
// The stock 275px picker dominates the sidebar. 100px keeps the full
// saturation/value field usable for pixel art while leaving the palette and
// undo history readable behind its temporary popup.
const COMPACT_COLOR_PICKER_SLIDER_WIDTH: f32 = 100.0;
const COMPACT_COLOR_PICKER_MARKER_RADIUS: f32 = 5.0;
const COLOR_PICKER_GRADIENT_STEPS: u32 = 6 * 6;

/// Retains the hue while the selected color is gray, matching the behavior of
/// egui's built-in picker without persisting UI state into a PixelBuddy file.
#[derive(Clone, Copy)]
struct CompactPickerColorState {
    color: Color32,
    hsvag: HsvaGamma,
}

fn compact_picker_color_state_id() -> egui::Id {
    egui::Id::new("pixelbuddy.compact_primary_color_picker_state")
}

fn compact_contrast_color(color: Color32) -> Color32 {
    if Rgba::from(color).intensity() < 0.5 {
        Color32::WHITE
    } else {
        Color32::BLACK
    }
}

/// Returns a deliberately small, fixed-size marker for the compact picker.
///
/// The stock picker uses one twelfth of the saturation/value square's width,
/// which remains visually large at this control's size and can overlap the hue
/// slider below it. The cap also keeps the marker valid in a constrained UI.
fn compact_picker_marker_radius(color_square: Rect) -> f32 {
    COMPACT_COLOR_PICKER_MARKER_RADIUS
        .min(color_square.width() / 2.0)
        .min(color_square.height() / 2.0)
}

fn compact_input_type_button_ui(ui: &mut egui::Ui) {
    let mut input_type = ui.ctx().style().visuals.numeric_color_space;
    if input_type.toggle_button_ui(ui).changed() {
        ui.ctx().all_styles_mut(|style| {
            style.visuals.numeric_color_space = input_type;
        });
    }
}

fn compact_gamma_color_inputs(ui: &mut egui::Ui, hsvag: &mut HsvaGamma) -> bool {
    let mut srgba = Hsva::from(*hsvag).to_srgba_unmultiplied();
    let mut edited = false;

    ui.horizontal(|ui| {
        compact_input_type_button_ui(ui);

        if ui
            .button("📋")
            .on_hover_text("Click to copy color values")
            .clicked()
        {
            let [r, g, b, _] = srgba;
            ui.ctx().copy_text(format!("{r}, {g}, {b}"));
        }

        edited |= ui
            .add(egui::DragValue::new(&mut srgba[0]).speed(0.5).prefix("R "))
            .changed();
        edited |= ui
            .add(egui::DragValue::new(&mut srgba[1]).speed(0.5).prefix("G "))
            .changed();
        edited |= ui
            .add(egui::DragValue::new(&mut srgba[2]).speed(0.5).prefix("B "))
            .changed();
    });

    if edited {
        *hsvag = HsvaGamma::from(Hsva::from_srgba_unmultiplied(srgba));
    }

    edited
}

fn compact_linear_color_inputs(ui: &mut egui::Ui, hsvag: &mut HsvaGamma) -> bool {
    let mut rgba = Hsva::from(*hsvag).to_rgba_unmultiplied();
    let mut edited = false;

    ui.horizontal(|ui| {
        compact_input_type_button_ui(ui);

        if ui
            .button("📋")
            .on_hover_text("Click to copy color values")
            .clicked()
        {
            let [r, g, b, _] = rgba;
            ui.ctx().copy_text(format!("{r:.03}, {g:.03}, {b:.03}"));
        }

        edited |= ui
            .add(
                egui::DragValue::new(&mut rgba[0])
                    .speed(0.003)
                    .range(0.0..=1.0)
                    .prefix("R ")
                    .custom_formatter(|number, _| format!("{number:.03}")),
            )
            .changed();
        edited |= ui
            .add(
                egui::DragValue::new(&mut rgba[1])
                    .speed(0.003)
                    .range(0.0..=1.0)
                    .prefix("G ")
                    .custom_formatter(|number, _| format!("{number:.03}")),
            )
            .changed();
        edited |= ui
            .add(
                egui::DragValue::new(&mut rgba[2])
                    .speed(0.003)
                    .range(0.0..=1.0)
                    .prefix("B ")
                    .custom_formatter(|number, _| format!("{number:.03}")),
            )
            .changed();
    });

    if edited {
        *hsvag = HsvaGamma::from(Hsva::from_rgba_unmultiplied(rgba[0], rgba[1], rgba[2], 1.0));
    }

    edited
}

fn compact_color_inputs(ui: &mut egui::Ui, hsvag: &mut HsvaGamma) -> bool {
    match ui.style().visuals.numeric_color_space {
        NumericColorSpace::GammaByte => compact_gamma_color_inputs(ui, hsvag),
        NumericColorSpace::Linear => compact_linear_color_inputs(ui, hsvag),
    }
}

/// The compact saturation/value field. This mirrors egui's private picker
/// implementation, but fixes the selection marker size and clips it to this
/// field so it cannot paint over the hue slider below.
fn compact_saturation_value_picker(
    ui: &mut egui::Ui,
    saturation: &mut f32,
    value: &mut f32,
    color_at: impl Fn(f32, f32) -> Color32,
) -> egui::Response {
    let desired_size = egui::Vec2::splat(ui.spacing().slider_width);
    let (rect, response) = ui.allocate_at_least(desired_size, egui::Sense::click_and_drag());

    if let Some(pointer) = response.interact_pointer_pos() {
        *saturation = egui::remap_clamp(pointer.x, rect.left()..=rect.right(), 0.0..=1.0);
        *value = egui::remap_clamp(pointer.y, rect.bottom()..=rect.top(), 0.0..=1.0);
    }

    if ui.is_rect_visible(rect) {
        let visuals = ui.style().interact(&response);
        let mut mesh = Mesh::default();

        for x_index in 0..=COLOR_PICKER_GRADIENT_STEPS {
            for y_index in 0..=COLOR_PICKER_GRADIENT_STEPS {
                let saturation_t = x_index as f32 / COLOR_PICKER_GRADIENT_STEPS as f32;
                let value_t = y_index as f32 / COLOR_PICKER_GRADIENT_STEPS as f32;
                let color = color_at(saturation_t, value_t);
                let x = egui::lerp(rect.left()..=rect.right(), saturation_t);
                let y = egui::lerp(rect.bottom()..=rect.top(), value_t);
                mesh.colored_vertex(egui::pos2(x, y), color);

                if x_index < COLOR_PICKER_GRADIENT_STEPS && y_index < COLOR_PICKER_GRADIENT_STEPS {
                    let x_offset = 1;
                    let y_offset = COLOR_PICKER_GRADIENT_STEPS + 1;
                    let top_left = y_index * y_offset + x_index;
                    mesh.add_triangle(top_left, top_left + x_offset, top_left + y_offset);
                    mesh.add_triangle(
                        top_left + x_offset,
                        top_left + y_offset,
                        top_left + y_offset + x_offset,
                    );
                }
            }
        }

        ui.painter().add(egui::Shape::mesh(mesh));
        ui.painter()
            .rect_stroke(rect, 0.0, visuals.bg_stroke, egui::StrokeKind::Inside);

        let marker_center = egui::pos2(
            egui::lerp(rect.left()..=rect.right(), *saturation),
            egui::lerp(rect.bottom()..=rect.top(), *value),
        );
        let picked_color = color_at(*saturation, *value);
        let marker_painter = ui.painter().with_clip_rect(rect);
        let marker_radius = compact_picker_marker_radius(rect);
        marker_painter.circle_filled(marker_center, marker_radius, picked_color);
        marker_painter.circle_stroke(
            marker_center,
            marker_radius,
            Stroke::new(
                visuals.fg_stroke.width,
                compact_contrast_color(picked_color),
            ),
        );
    }

    response
}

fn compact_hue_picker(
    ui: &mut egui::Ui,
    hue: &mut f32,
    color_at: impl Fn(f32) -> Color32,
) -> egui::Response {
    let desired_size = egui::vec2(ui.spacing().slider_width, ui.spacing().interact_size.y);
    let (rect, response) = ui.allocate_at_least(desired_size, egui::Sense::click_and_drag());

    if let Some(pointer) = response.interact_pointer_pos() {
        *hue = egui::remap_clamp(pointer.x, rect.left()..=rect.right(), 0.0..=1.0);
    }

    if ui.is_rect_visible(rect) {
        let visuals = ui.style().interact(&response);
        let mut mesh = Mesh::default();

        for index in 0..=COLOR_PICKER_GRADIENT_STEPS {
            let t = index as f32 / COLOR_PICKER_GRADIENT_STEPS as f32;
            let color = color_at(t);
            let x = egui::lerp(rect.left()..=rect.right(), t);
            mesh.colored_vertex(egui::pos2(x, rect.top()), color);
            mesh.colored_vertex(egui::pos2(x, rect.bottom()), color);
            if index < COLOR_PICKER_GRADIENT_STEPS {
                mesh.add_triangle(2 * index, 2 * index + 1, 2 * index + 2);
                mesh.add_triangle(2 * index + 1, 2 * index + 2, 2 * index + 3);
            }
        }

        ui.painter().add(egui::Shape::mesh(mesh));
        ui.painter()
            .rect_stroke(rect, 0.0, visuals.bg_stroke, egui::StrokeKind::Inside);

        let x = egui::lerp(rect.left()..=rect.right(), *hue);
        let marker_half_width = rect.height() / 4.0;
        let picked_color = color_at(*hue);
        ui.painter().add(egui::Shape::convex_polygon(
            vec![
                egui::pos2(x, rect.center().y),
                egui::pos2(x + marker_half_width, rect.bottom()),
                egui::pos2(x - marker_half_width, rect.bottom()),
            ],
            picked_color,
            Stroke::new(
                visuals.fg_stroke.width,
                compact_contrast_color(picked_color),
            ),
        ));
    }

    response
}

fn compact_color_picker_color32(ui: &mut egui::Ui, color: &mut Color32) -> bool {
    let original_color = *color;
    let state_id = compact_picker_color_state_id();
    let mut hsvag = ui
        .data(|data| data.get_temp::<CompactPickerColorState>(state_id))
        .filter(|state| state.color == original_color)
        .map(|state| state.hsvag)
        .unwrap_or_else(|| HsvaGamma::from(original_color));

    compact_color_inputs(ui, &mut hsvag);

    let selected_color_size = egui::vec2(ui.spacing().slider_width, ui.spacing().interact_size.y);
    egui::color_picker::show_color(ui, Color32::from(hsvag), selected_color_size)
        .on_hover_text("Selected color");

    hsvag.a = 1.0;
    let opaque = HsvaGamma { a: 1.0, ..hsvag };
    let HsvaGamma {
        h,
        s: saturation,
        v: value,
        a: _,
    } = &mut hsvag;

    compact_saturation_value_picker(ui, saturation, value, |saturation, value| {
        HsvaGamma {
            s: saturation,
            v: value,
            ..opaque
        }
        .into()
    })
    .on_hover_text("Saturation and value");

    compact_hue_picker(ui, h, |hue| {
        HsvaGamma {
            h: hue,
            s: 1.0,
            v: 1.0,
            a: 1.0,
        }
        .into()
    })
    .on_hover_text("Hue");

    let updated_color = Color32::from(hsvag);
    ui.data_mut(|data| {
        data.insert_temp(
            state_id,
            CompactPickerColorState {
                color: updated_color,
                hsvag,
            },
        );
    });

    if updated_color != original_color {
        *color = updated_color;
        true
    } else {
        false
    }
}

/// Renders the primary-color swatch and its compact picker popup.
///
/// `egui`'s stock color-edit button uses a 275px color field, which is too
/// large for PixelBuddy's 200px sidebar. This mirrors its popup behavior
/// (including Escape and click-outside dismissal) while using a smaller field.
fn compact_primary_color_picker(ui: &mut egui::Ui, color: &mut Color32) -> egui::Response {
    let popup_id = ui.make_persistent_id("pixelbuddy.primary_color_picker");
    let is_open = ui.memory(|memory| memory.is_popup_open(popup_id));

    let (rect, mut response) =
        ui.allocate_exact_size(ui.spacing().interact_size, egui::Sense::click());
    let visuals = if is_open {
        &ui.visuals().widgets.open
    } else {
        ui.style().interact(&response)
    };
    let swatch_rect = rect.expand(visuals.expansion);
    let stroke_width = 1.0;

    egui::color_picker::show_color_at(ui.painter(), *color, swatch_rect.shrink(stroke_width));
    ui.painter().rect_stroke(
        swatch_rect,
        visuals.corner_radius.at_most(2),
        (stroke_width, visuals.bg_fill),
        egui::StrokeKind::Inside,
    );

    response = response.on_hover_text("Edit primary color");
    if response.clicked() {
        ui.memory_mut(|memory| memory.toggle_popup(popup_id));
    }

    if ui.memory(|memory| memory.is_popup_open(popup_id)) {
        let popup_response = egui::Area::new(popup_id)
            .kind(egui::UiKind::Picker)
            .order(egui::Order::Foreground)
            .fixed_pos(response.rect.max)
            .show(ui.ctx(), |popup_ui| {
                popup_ui.spacing_mut().slider_width = COMPACT_COLOR_PICKER_SLIDER_WIDTH;
                egui::Frame::popup(ui.style()).show(popup_ui, |popup_ui| {
                    if compact_color_picker_color32(popup_ui, color) {
                        response.mark_changed();
                    }
                });
            })
            .response;

        if !response.clicked()
            && (ui.input(|input| input.key_pressed(egui::Key::Escape))
                || popup_response.clicked_elsewhere())
        {
            ui.memory_mut(|memory| memory.close_popup());
        }
    }

    response
}

#[derive(Clone, Copy)]
enum PaletteAction {
    SetSecondaryColor([u8; 4]),
    Move { from: usize, to: usize },
    Remove { index: usize },
}

#[derive(Clone, Copy)]
enum PaletteVerticalDirection {
    Up,
    Down,
}

/// Returns the number of fixed-size swatches that fit on a palette row.
///
/// Keeping the wrapping calculation explicit lets the context menu move a
/// color by the same visual column that the palette grid displays.
fn palette_grid_column_count(available_width: f32, item_spacing: f32) -> usize {
    ((available_width + item_spacing) / (PALETTE_SWATCH_SIZE + item_spacing))
        .floor()
        .max(1.0) as usize
}

fn palette_vertical_move_target(
    index: usize,
    palette_len: usize,
    columns: usize,
    direction: PaletteVerticalDirection,
) -> Option<usize> {
    if index >= palette_len || columns == 0 {
        return None;
    }

    let target = match direction {
        PaletteVerticalDirection::Up => index.checked_sub(columns),
        PaletteVerticalDirection::Down => index.checked_add(columns),
    }?;

    (target < palette_len).then_some(target)
}

/// Transient inline-rename state. It intentionally lives in egui's temporary
/// UI memory rather than the project, so an unfinished edit is never saved.
#[derive(Clone)]
struct LayerRenameDraft {
    frame_index: usize,
    layer_index: usize,
    original_name: String,
    name: String,
}

fn layer_rename_draft_id() -> egui::Id {
    egui::Id::new("pixelbuddy.layer_rename_draft")
}

fn layer_rename_text_id(frame_index: usize, layer_index: usize) -> egui::Id {
    egui::Id::new(("pixelbuddy.layer_rename_text", frame_index, layer_index))
}

fn clear_layer_rename_draft(ctx: &egui::Context) {
    ctx.data_mut(|data| data.remove::<LayerRenameDraft>(layer_rename_draft_id()));
}

fn visibility_button(ui: &mut egui::Ui, visible: bool) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(24.0, 22.0), egui::Sense::click());
    let visuals = ui.style().interact(&response);

    ui.painter()
        .rect_filled(rect, visuals.corner_radius, visuals.bg_fill);
    ui.painter().rect_stroke(
        rect,
        visuals.corner_radius,
        visuals.bg_stroke,
        egui::StrokeKind::Inside,
    );
    draw_visibility_icon(ui.painter(), rect, visible, visuals.fg_stroke.color);

    response.on_hover_text(if visible { "Hide layer" } else { "Show layer" })
}

fn draw_visibility_icon(painter: &egui::Painter, rect: Rect, visible: bool, color: Color32) {
    let center = rect.center();
    let half_width = 7.0_f32;
    let half_height = 4.5_f32;
    let stroke = Stroke::new(1.5_f32, color);

    // A small almond-shaped eye stays crisp across platforms, unlike an
    // emoji glyph that can vary with the system font.
    painter.add(egui::Shape::line(
        vec![
            Pos2::new(center.x - half_width, center.y),
            Pos2::new(center.x - 2.5_f32, center.y - half_height),
            Pos2::new(center.x + 2.5_f32, center.y - half_height),
            Pos2::new(center.x + half_width, center.y),
            Pos2::new(center.x + 2.5_f32, center.y + half_height),
            Pos2::new(center.x - 2.5_f32, center.y + half_height),
            Pos2::new(center.x - half_width, center.y),
        ],
        stroke,
    ));
    painter.circle_filled(center, 2.0_f32, color);

    if !visible {
        painter.line_segment(
            [
                Pos2::new(center.x - half_width, center.y + half_height),
                Pos2::new(center.x + half_width, center.y - half_height),
            ],
            Stroke::new(1.8_f32, color),
        );
    }
}

fn section_header(ui: &mut egui::Ui, title: &str) -> egui::Rect {
    let (rect, _response) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 24.0), egui::Sense::hover());

    // Left accent bar
    ui.painter().rect_filled(
        Rect::from_min_size(rect.min, egui::vec2(3.0, rect.height())),
        0.0,
        ui.visuals().selection.bg_fill,
    );

    // Text
    ui.painter().text(
        Pos2::new(rect.min.x + 8.0, rect.center().y),
        egui::Align2::LEFT_CENTER,
        title.to_uppercase(),
        egui::FontId::proportional(14.0),
        Color32::WHITE,
    );

    // Bottom subtle line
    ui.painter().hline(
        rect.min.x..=rect.max.x,
        rect.max.y,
        Stroke::new(1.0_f32, crate::ui::theme::SEPARATOR_COLOR),
    );
    ui.add_space(4.0);

    rect
}

pub fn show(ctx: &egui::Context, app: &mut PixelBuddyApp) {
    egui::SidePanel::right("layers_panel")
        .exact_width(200.0)
        .show_separator_line(false)
        .show(ctx, |ui| {
            section_header(ui, "Layers");

            let layers_count = app.editor.document().layers.len();
            let active_idx = app.editor.document().active_layer_index;
            let frame_index = app.editor.animation.current_frame_index;
            let mut new_active = active_idx;
            let mut visibility_changes: Vec<(usize, bool)> = Vec::new();
            let mut rename_change: Option<(usize, String)> = None;

            // A frame/layer switch could invalidate an inline editor. Cancel
            // its UI-only draft instead of applying it to a different layer.
            let stale_rename_draft = ctx.data(|data| {
                data.get_temp::<LayerRenameDraft>(layer_rename_draft_id())
                    .is_some_and(|draft| {
                        draft.frame_index != frame_index
                            || app
                                .editor
                                .document()
                                .layers
                                .get(draft.layer_index)
                                .map(|layer| layer.name.as_str())
                                != Some(draft.original_name.as_str())
                    })
            });
            if stale_rename_draft {
                clear_layer_rename_draft(ctx);
            }

            egui::ScrollArea::vertical()
                .id_salt("layers_scroll")
                .max_height(250.0)
                .show(ui, |ui| {
                    // Iterate in reverse for Photoshop-like display (top layer listed first)
                    for i in (0..layers_count).rev() {
                        let is_active = i == active_idx;
                        let layer_name = app.editor.document().layers[i].name.clone();
                        let layer_visible = app.editor.document().layers[i].visible;
                        let rename_draft = ctx.data(|data| {
                            data.get_temp::<LayerRenameDraft>(layer_rename_draft_id())
                        });
                        let is_renaming = rename_draft.as_ref().is_some_and(|draft| {
                            draft.frame_index == frame_index
                                && draft.layer_index == i
                                && draft.original_name == layer_name
                        });

                        let mut frame = egui::Frame::NONE
                            .inner_margin(egui::Margin::same(4))
                            .corner_radius(4);

                        if is_active {
                            frame = frame.fill(ui.visuals().selection.bg_fill);
                        }

                        frame.show(ui, |ui| {
                            ui.horizontal(|ui| {
                                // Visibility toggle
                                if visibility_button(ui, layer_visible).clicked() {
                                    visibility_changes.push((i, !layer_visible));
                                }

                                if is_renaming {
                                    let mut draft = rename_draft
                                        .expect("rename state was checked immediately above");
                                    let text_id = layer_rename_text_id(frame_index, i);
                                    let response = ui
                                        .add(
                                            egui::TextEdit::singleline(&mut draft.name)
                                                .id(text_id)
                                                .desired_width(ui.available_width())
                                                .text_color(Color32::WHITE)
                                                .hint_text("Layer name"),
                                        )
                                        .on_hover_text("Rename layer");
                                    let escape_pressed = response.has_focus()
                                        && ui.input(|input| input.key_pressed(egui::Key::Escape));
                                    let should_commit = !escape_pressed
                                        && ((response.has_focus()
                                            && ui.input(|input| {
                                                input.key_pressed(egui::Key::Enter)
                                            }))
                                            || response.lost_focus());

                                    if escape_pressed {
                                        clear_layer_rename_draft(ctx);
                                    } else if should_commit {
                                        clear_layer_rename_draft(ctx);
                                        if draft.name != layer_name {
                                            rename_change = Some((i, draft.name));
                                        }
                                    } else {
                                        ctx.data_mut(|data| {
                                            data.insert_temp(layer_rename_draft_id(), draft)
                                        });
                                    }
                                } else {
                                    // Keep selection and renaming separate:
                                    // click selects, double-click opens an inline editor.
                                    let response = ui
                                        .selectable_label(
                                            is_active,
                                            egui::RichText::new(&layer_name).color(if is_active {
                                                Color32::WHITE
                                            } else {
                                                ui.visuals().text_color()
                                            }),
                                        )
                                        .on_hover_text("Double-click to rename layer");
                                    if response.clicked() {
                                        new_active = i;
                                    }
                                    if response.double_clicked() {
                                        ctx.data_mut(|data| {
                                            data.insert_temp(
                                                layer_rename_draft_id(),
                                                LayerRenameDraft {
                                                    frame_index,
                                                    layer_index: i,
                                                    original_name: layer_name.clone(),
                                                    name: layer_name,
                                                },
                                            )
                                        });
                                        ui.memory_mut(|memory| {
                                            memory.request_focus(layer_rename_text_id(
                                                frame_index,
                                                i,
                                            ));
                                        });
                                        ctx.request_repaint();
                                    }
                                }
                            });
                        });
                    }
                });

            // Apply visibility changes
            for (idx, visible) in &visibility_changes {
                if app
                    .editor
                    .mutate_document("Toggle layer visibility", |document| {
                        let Some(layer) = document.layers.get_mut(*idx) else {
                            return false;
                        };
                        if layer.visible == *visible {
                            return false;
                        }
                        layer.visible = *visible;
                        true
                    })
                {
                    app.texture_dirty = true;
                }
            }

            if let Some((idx, name)) = rename_change {
                // This uses a snapshot command rather than a direct field
                // write so Rename participates in undo/redo and dirty state.
                let _ = app.editor.mutate_document("Rename layer", move |document| {
                    let Some(layer) = document.layers.get_mut(idx) else {
                        return false;
                    };
                    if layer.name == name {
                        return false;
                    }
                    layer.name = name;
                    true
                });
            }

            // Apply active layer selection
            if new_active != active_idx {
                app.editor.document_mut().active_layer_index = new_active;
            }

            ui.separator();

            // Layer action buttons
            ui.horizontal_wrapped(|ui| {
                let button_size = egui::vec2(32.0, 24.0);
                let icon_size = egui::vec2(18.0, 18.0);
                let text_color = ui.visuals().text_color();

                let add_img = egui::Image::new(egui::include_image!("../../assets/icons/plus.svg"))
                    .tint(text_color)
                    .fit_to_exact_size(icon_size);
                if ui
                    .add(egui::Button::image(add_img).min_size(button_size))
                    .on_hover_text("Add Layer")
                    .clicked()
                    && app.editor.mutate_document("Add layer", |document| {
                        document.add_layer();
                        true
                    })
                {
                    clear_layer_rename_draft(ctx);
                    app.texture_dirty = true;
                }

                let del_img =
                    egui::Image::new(egui::include_image!("../../assets/icons/trash.svg"))
                        .tint(text_color)
                        .fit_to_exact_size(icon_size);
                if ui
                    .add_enabled(
                        layers_count > 1,
                        egui::Button::image(del_img).min_size(button_size),
                    )
                    .on_hover_text("Delete Layer")
                    .clicked()
                {
                    let active_layer = app.editor.document().active_layer_index;
                    if app.editor.mutate_document("Delete layer", |document| {
                        if document.layers.len() <= 1 || active_layer >= document.layers.len() {
                            return false;
                        }
                        document.remove_layer(active_layer);
                        true
                    }) {
                        clear_layer_rename_draft(ctx);
                        app.texture_dirty = true;
                    }
                }

                let dup_img = egui::Image::new(egui::include_image!("../../assets/icons/copy.svg"))
                    .tint(text_color)
                    .fit_to_exact_size(icon_size);
                if ui
                    .add(egui::Button::image(dup_img).min_size(button_size))
                    .on_hover_text("Duplicate Layer")
                    .clicked()
                {
                    let active_layer = app.editor.document().active_layer_index;
                    if app.editor.mutate_document("Duplicate layer", |document| {
                        if active_layer >= document.layers.len() {
                            return false;
                        }
                        document.duplicate_layer(active_layer);
                        true
                    }) {
                        clear_layer_rename_draft(ctx);
                        app.texture_dirty = true;
                    }
                }

                let up_img =
                    egui::Image::new(egui::include_image!("../../assets/icons/arrow-up.svg"))
                        .tint(text_color)
                        .fit_to_exact_size(icon_size);
                if ui
                    .add(egui::Button::image(up_img).min_size(button_size))
                    .on_hover_text("Move Up")
                    .clicked()
                {
                    let idx = app.editor.document().active_layer_index;
                    if idx + 1 < layers_count
                        && app.editor.mutate_document("Move layer up", |document| {
                            document.move_layer(idx, idx + 1);
                            true
                        })
                    {
                        clear_layer_rename_draft(ctx);
                        app.texture_dirty = true;
                    }
                }

                let down_img =
                    egui::Image::new(egui::include_image!("../../assets/icons/arrow-down.svg"))
                        .tint(text_color)
                        .fit_to_exact_size(icon_size);
                if ui
                    .add(egui::Button::image(down_img).min_size(button_size))
                    .on_hover_text("Move Down")
                    .clicked()
                {
                    let idx = app.editor.document().active_layer_index;
                    if idx > 0
                        && app.editor.mutate_document("Move layer down", |document| {
                            document.move_layer(idx, idx - 1);
                            true
                        })
                    {
                        clear_layer_rename_draft(ctx);
                        app.texture_dirty = true;
                    }
                }
            });

            ui.separator();
            ui.add_space(8.0);

            // Active layer properties
            if layers_count > 0 {
                let active = app.editor.document().active_layer_index;

                ui.label("Opacity")
                    .on_hover_text("Layer opacity (0 = transparent, 1 = opaque)");
                let mut opacity = app.editor.document().layers[active].opacity;
                if ui
                    .add(egui::Slider::new(&mut opacity, 0.0..=1.0).fixed_decimals(2))
                    .changed()
                {
                    app.editor.document_mut().layers[active].opacity = opacity;
                    app.texture_dirty = true;
                }

                let mut locked = app.editor.document().layers[active].locked;
                if ui
                    .checkbox(&mut locked, "Lock layer")
                    .on_hover_text("Prevent accidental edits to this layer")
                    .changed()
                    && app.editor.mutate_document("Lock layer", |document| {
                        let Some(layer) = document.layers.get_mut(active) else {
                            return false;
                        };
                        if layer.locked == locked {
                            return false;
                        };
                        layer.locked = locked;
                        true
                    })
                {
                    app.texture_dirty = true;
                }

                ui.label("Blend Mode")
                    .on_hover_text("How this layer's pixels combine with layers below");
                let current_mode = app.editor.document().layers[active].blend_mode;
                let mode_label = match current_mode {
                    BlendMode::Normal => "Normal",
                    BlendMode::Multiply => "Multiply",
                    BlendMode::Screen => "Screen",
                    BlendMode::Overlay => "Overlay",
                };
                egui::ComboBox::from_id_salt("blend_mode")
                    .selected_text(mode_label)
                    .show_ui(ui, |ui| {
                        for (mode, label) in [
                            (BlendMode::Normal, "Normal"),
                            (BlendMode::Multiply, "Multiply"),
                            (BlendMode::Screen, "Screen"),
                            (BlendMode::Overlay, "Overlay"),
                        ] {
                            if ui.selectable_label(current_mode == mode, label).clicked()
                                && app.editor.mutate_document("Change blend mode", |document| {
                                    let Some(layer) = document.layers.get_mut(active) else {
                                        return false;
                                    };
                                    if layer.blend_mode == mode {
                                        return false;
                                    }
                                    layer.blend_mode = mode;
                                    true
                                })
                            {
                                app.texture_dirty = true;
                            }
                        }
                    });
            }

            ui.add_space(12.0);

            let header_rect = section_header(ui, "Palette");
            let mut child_ui = ui.new_child(
                egui::UiBuilder::new()
                    .max_rect(header_rect)
                    .layout(egui::Layout::right_to_left(egui::Align::Center)),
            );

            let mut color32 = egui::Color32::from_rgba_unmultiplied(
                app.editor.primary_color[0],
                app.editor.primary_color[1],
                app.editor.primary_color[2],
                app.editor.primary_color[3],
            );

            // Add a little margin on the right so it doesn't touch the very edge
            child_ui.add_space(8.0);

            if compact_primary_color_picker(&mut child_ui, &mut color32).changed() {
                let arr = color32.to_array();
                app.editor.set_primary_color(arr);
            }
            ui.separator();

            let selected = app.editor.document().palette.selected_index;
            let palette_len = app.editor.document().palette.colors.len();
            let mut palette_action = None;

            egui::ScrollArea::vertical()
                .id_salt("palette_scroll")
                .max_height(220.0)
                .show(ui, |ui| {
                    let palette_grid_spacing = ui.spacing().item_spacing;
                    let palette_columns =
                        palette_grid_column_count(ui.available_width(), palette_grid_spacing.x);
                    egui::Grid::new("palette_grid")
                        .num_columns(palette_columns)
                        .min_col_width(PALETTE_SWATCH_SIZE)
                        .spacing(palette_grid_spacing)
                        .show(ui, |ui| {
                            for i in 0..palette_len {
                                let color = app.editor.document().palette.colors[i];
                                let egui_color = egui::Color32::from_rgba_unmultiplied(
                                    color[0], color[1], color[2], color[3],
                                );

                                let stroke = if i == selected {
                                    egui::Stroke::new(2.0_f32, ui.visuals().selection.bg_fill)
                                } else {
                                    egui::Stroke::new(1.0_f32, egui::Color32::from_gray(60))
                                };

                                let (swatch_rect, response) = ui.allocate_exact_size(
                                    egui::vec2(PALETTE_SWATCH_SIZE, PALETTE_SWATCH_SIZE),
                                    egui::Sense::click(),
                                );

                                ui.painter().rect(
                                    swatch_rect,
                                    2.0,
                                    egui_color,
                                    stroke,
                                    egui::StrokeKind::Inside,
                                );
                                let response = response.on_hover_text(
                                    "Left-click: set primary · Right-click: options",
                                );

                                if response.clicked() {
                                    app.editor.document_mut().palette.set_selected(i);
                                    app.editor.set_primary_color(color);
                                }
                                response.context_menu(|ui| {
                                    if ui.button("Set as secondary color").clicked() {
                                        palette_action =
                                            Some(PaletteAction::SetSecondaryColor(color));
                                        ui.close_menu();
                                    }

                                    ui.separator();

                                    let move_up_target = palette_vertical_move_target(
                                        i,
                                        palette_len,
                                        palette_columns,
                                        PaletteVerticalDirection::Up,
                                    );
                                    if ui
                                        .add_enabled(
                                            move_up_target.is_some(),
                                            egui::Button::new("Move up"),
                                        )
                                        .on_hover_text("Move this color one palette row up")
                                        .clicked()
                                    {
                                        palette_action = Some(PaletteAction::Move {
                                            from: i,
                                            to: move_up_target
                                                .expect("enabled only when an upper row exists"),
                                        });
                                        ui.close_menu();
                                    }

                                    let move_down_target = palette_vertical_move_target(
                                        i,
                                        palette_len,
                                        palette_columns,
                                        PaletteVerticalDirection::Down,
                                    );
                                    if ui
                                        .add_enabled(
                                            move_down_target.is_some(),
                                            egui::Button::new("Move down"),
                                        )
                                        .on_hover_text("Move this color one palette row down")
                                        .clicked()
                                    {
                                        palette_action = Some(PaletteAction::Move {
                                            from: i,
                                            to: move_down_target
                                                .expect("enabled only when a lower row exists"),
                                        });
                                        ui.close_menu();
                                    }

                                    ui.separator();

                                    if ui
                                        .add_enabled(i > 0, egui::Button::new("Move left"))
                                        .on_hover_text(
                                            "Move this color one place earlier in the palette",
                                        )
                                        .clicked()
                                    {
                                        palette_action =
                                            Some(PaletteAction::Move { from: i, to: i - 1 });
                                        ui.close_menu();
                                    }

                                    if ui
                                        .add_enabled(
                                            i + 1 < palette_len,
                                            egui::Button::new("Move right"),
                                        )
                                        .on_hover_text(
                                            "Move this color one place later in the palette",
                                        )
                                        .clicked()
                                    {
                                        palette_action =
                                            Some(PaletteAction::Move { from: i, to: i + 1 });
                                        ui.close_menu();
                                    }

                                    ui.separator();

                                    if ui
                                        .add_enabled(
                                            palette_len > 1,
                                            egui::Button::new("Remove color"),
                                        )
                                        .on_hover_text("Remove this color from the palette")
                                        .clicked()
                                    {
                                        palette_action = Some(PaletteAction::Remove { index: i });
                                        ui.close_menu();
                                    }
                                });

                                if (i + 1) % palette_columns == 0 {
                                    ui.end_row();
                                }
                            }
                        });
                });

            if let Some(action) = palette_action {
                match action {
                    PaletteAction::SetSecondaryColor(color) => {
                        app.editor.set_secondary_color(color);
                    }
                    PaletteAction::Move { from, to } => {
                        if app
                            .editor
                            .mutate_document("Move palette color", |document| {
                                document.palette.move_color(from, to)
                            })
                        {
                            app.texture_dirty = true;
                        }
                    }
                    PaletteAction::Remove { index } => {
                        if app
                            .editor
                            .mutate_document("Remove palette color", |document| {
                                document.palette.remove_color(index)
                            })
                        {
                            app.texture_dirty = true;
                        }
                    }
                }
            }

            ui.add_space(6.0);
            if ui
                .button("+ Add Color")
                .on_hover_text("Add active primary color to palette")
                .clicked()
            {
                let primary_color = app.editor.primary_color;
                if app.editor.mutate_document("Add palette color", |document| {
                    document.palette.add_color(primary_color);
                    true
                }) {
                    app.texture_dirty = true;
                }
            }

            ui.add_space(12.0);
            ui.separator();
            egui::CollapsingHeader::new(egui::RichText::new("HISTORY").strong().size(13.0))
                .default_open(false)
                .show(ui, |ui| {
                    let undo_descs = app.editor.history.undo_descriptions();
                    let redo_descs = app.editor.history.redo_descriptions();

                    egui::ScrollArea::vertical()
                        .id_salt("history_scroll")
                        .max_height(140.0)
                        .show(ui, |ui| {
                            if undo_descs.is_empty() && redo_descs.is_empty() {
                                ui.label(
                                    egui::RichText::new("No actions yet")
                                        .small()
                                        .color(egui::Color32::GRAY),
                                );
                            } else {
                                for (idx, desc) in undo_descs.iter().enumerate() {
                                    let is_latest = idx + 1 == undo_descs.len();
                                    let text = format!("{}. {}", idx + 1, desc);
                                    let label = if is_latest {
                                        egui::RichText::new(&text)
                                            .strong()
                                            .color(ui.visuals().selection.bg_fill)
                                    } else {
                                        egui::RichText::new(&text)
                                    };

                                    if ui
                                        .selectable_label(is_latest, label)
                                        .on_hover_text("Click to jump to this point in history")
                                        .clicked()
                                    {
                                        if app.editor.jump_to_undo_index(idx) {
                                            app.texture_dirty = true;
                                        }
                                    }
                                }

                                for (idx, desc) in redo_descs.iter().enumerate() {
                                    let text =
                                        format!("↷ {}. {}", undo_descs.len() + idx + 1, desc);
                                    ui.label(
                                        egui::RichText::new(text)
                                            .italics()
                                            .color(egui::Color32::from_gray(100)),
                                    );
                                }
                            }
                        });
                });
        });
}

#[cfg(test)]
mod tests {
    use super::{
        compact_picker_marker_radius, palette_grid_column_count, palette_vertical_move_target,
        PaletteVerticalDirection, COMPACT_COLOR_PICKER_MARKER_RADIUS,
    };

    #[test]
    fn palette_grid_column_count_accounts_for_inter_swatch_spacing() {
        // Six 22px swatches and five 8px gaps need 172px in total.
        assert_eq!(palette_grid_column_count(172.0, 8.0), 6);
        assert_eq!(palette_grid_column_count(171.9, 8.0), 5);
        assert_eq!(palette_grid_column_count(0.0, 8.0), 1);
    }

    #[test]
    fn vertical_palette_moves_stay_in_the_same_visual_column() {
        let palette_len = 14;
        let columns = 6;

        assert_eq!(
            palette_vertical_move_target(7, palette_len, columns, PaletteVerticalDirection::Up),
            Some(1)
        );
        assert_eq!(
            palette_vertical_move_target(1, palette_len, columns, PaletteVerticalDirection::Down),
            Some(7)
        );
    }

    #[test]
    fn vertical_palette_moves_are_unavailable_without_a_matching_row_slot() {
        let palette_len = 14;
        let columns = 6;

        assert_eq!(
            palette_vertical_move_target(1, palette_len, columns, PaletteVerticalDirection::Up),
            None
        );
        assert_eq!(
            palette_vertical_move_target(8, palette_len, columns, PaletteVerticalDirection::Down),
            None
        );
        assert_eq!(
            palette_vertical_move_target(0, palette_len, 0, PaletteVerticalDirection::Down),
            None
        );
    }

    #[test]
    fn compact_picker_marker_stays_small_and_fits_tight_color_fields() {
        let normal_field = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(100.0, 100.0));
        assert_eq!(
            compact_picker_marker_radius(normal_field),
            COMPACT_COLOR_PICKER_MARKER_RADIUS
        );

        let constrained_field = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(6.0, 8.0));
        assert_eq!(compact_picker_marker_radius(constrained_field), 3.0);
    }
}

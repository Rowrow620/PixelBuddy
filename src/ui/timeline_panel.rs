use crate::app::PixelBuddyApp;

/// A timeline header selects only the frame. A grid-cell click additionally
/// selects a layer in that target frame; keeping the layer optional prevents
/// a header click from copying the old frame's active-layer index onto it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct FrameSelectionRequest {
    frame_index: usize,
    active_layer_index: Option<usize>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct FrameRangeSelection {
    anchor: usize,
    focus: usize,
}

#[derive(Clone, Debug, PartialEq)]
struct TagEditorDraft {
    tag_index: Option<usize>,
    name: String,
    color: [f32; 3],
    start_frame: usize,
    end_frame: usize,
}

impl TagEditorDraft {
    fn for_creation(selection: FrameRangeSelection, frame_count: usize) -> Option<Self> {
        let (from, to) = selection.bounds(frame_count)?;
        Some(Self {
            tag_index: None,
            name: "New Tag".to_owned(),
            color: [0.8, 0.2, 0.2],
            start_frame: from + 1,
            end_frame: to + 1,
        })
    }

    fn for_edit(index: usize, tag: &crate::document::animation::FrameTag) -> Self {
        Self {
            tag_index: Some(index),
            name: tag.name.clone(),
            color: tag.color,
            start_frame: tag.from_frame + 1,
            end_frame: tag.to_frame + 1,
        }
    }

    fn pending_action(&self, frame_count: usize) -> Option<PendingTagAction> {
        if self.name.trim().is_empty()
            || self.start_frame == 0
            || self.end_frame == 0
            || self.start_frame > self.end_frame
            || self.end_frame > frame_count
        {
            return None;
        }
        let tag = crate::document::animation::FrameTag {
            name: self.name.trim().to_owned(),
            color: self.color,
            from_frame: self.start_frame - 1,
            to_frame: self.end_frame - 1,
        };
        tag.validate(frame_count).ok()?;
        Some(match self.tag_index {
            Some(index) => PendingTagAction::Update { index, tag },
            None => PendingTagAction::Create(tag),
        })
    }
}

impl FrameRangeSelection {
    fn single(frame_index: usize) -> Self {
        Self {
            anchor: frame_index,
            focus: frame_index,
        }
    }

    fn extend_to(self, frame_index: usize) -> Self {
        Self {
            focus: frame_index,
            ..self
        }
    }

    fn bounds(self, frame_count: usize) -> Option<(usize, usize)> {
        let last_frame = frame_count.checked_sub(1)?;
        let anchor = self.anchor.min(last_frame);
        let focus = self.focus.min(last_frame);
        Some((anchor.min(focus), anchor.max(focus)))
    }

    fn contains(self, frame_index: usize, frame_count: usize) -> bool {
        self.bounds(frame_count)
            .is_some_and(|(from, to)| (from..=to).contains(&frame_index))
    }
}

#[derive(Clone, Debug, PartialEq)]
enum PendingTagAction {
    Create(crate::document::animation::FrameTag),
    Update {
        index: usize,
        tag: crate::document::animation::FrameTag,
    },
    Remove(usize),
}
fn timeline_tag_editor_id(document_session_id: u64) -> egui::Id {
    egui::Id::new("pixelbuddy.timeline_tag_editor").with(document_session_id)
}

fn timeline_frame_range_id(document_session_id: u64) -> egui::Id {
    egui::Id::new("pixelbuddy.timeline_frame_range").with(document_session_id)
}

fn normalize_tag_range(tag: &mut crate::document::animation::FrameTag, frame_count: usize) -> bool {
    let Some(last_frame) = frame_count.checked_sub(1) else {
        return false;
    };
    tag.from_frame = tag.from_frame.min(last_frame);
    tag.to_frame = tag.to_frame.min(last_frame);
    if tag.from_frame > tag.to_frame {
        std::mem::swap(&mut tag.from_frame, &mut tag.to_frame);
    }
    true
}

fn frame_has_layer(app: &PixelBuddyApp, frame_index: usize, layer_index: usize) -> bool {
    app.editor
        .animation
        .frames
        .get(frame_index)
        .and_then(|frame| frame.document.layers.get(layer_index))
        .is_some()
}

const TIMELINE_THUMBNAIL_SIZE: usize = 24;
const TIMELINE_HEADER_HEIGHT: f32 = 38.0;
const TIMELINE_ROW_HEIGHT: f32 = 32.0;
const TIMELINE_PANEL_BASE_HEIGHT: f32 = 96.0;
const TIMELINE_PANEL_MIN_HEIGHT: f32 = 112.0;
const TIMELINE_PANEL_MAX_SCREEN_FRACTION: f32 = 0.65;
const TIMELINE_PANEL_ABSOLUTE_MAX_HEIGHT: f32 = 560.0;
const TIMELINE_DEFAULT_VISIBLE_LAYERS: usize = 4;
const TIMELINE_WHEEL_SCALE: f32 = 0.25;
const TIMELINE_WHEEL_MAX_STEP: f32 = TIMELINE_ROW_HEIGHT;
/// At 24×24 RGBA pixels this keeps live GPU thumbnail texels near 1.2 MiB,
/// excluding small renderer/handle overhead.
const MAX_TIMELINE_THUMBNAILS: usize = 512;

fn timeline_panel_max_height(screen_height: f32) -> f32 {
    if !screen_height.is_finite() || screen_height <= 0.0 {
        return TIMELINE_PANEL_MIN_HEIGHT;
    }
    (screen_height * TIMELINE_PANEL_MAX_SCREEN_FRACTION).clamp(
        TIMELINE_PANEL_MIN_HEIGHT,
        TIMELINE_PANEL_ABSOLUTE_MAX_HEIGHT,
    )
}

fn timeline_panel_default_height(layer_count: usize, max_height: f32) -> f32 {
    let visible_layers = layer_count.clamp(1, TIMELINE_DEFAULT_VISIBLE_LAYERS);
    (TIMELINE_PANEL_BASE_HEIGHT + visible_layers as f32 * TIMELINE_ROW_HEIGHT).clamp(
        TIMELINE_PANEL_MIN_HEIGHT,
        max_height.max(TIMELINE_PANEL_MIN_HEIGHT),
    )
}

fn scaled_timeline_wheel_delta(delta: f32) -> f32 {
    if !delta.is_finite() {
        return 0.0;
    }
    (delta * TIMELINE_WHEEL_SCALE).clamp(-TIMELINE_WHEEL_MAX_STEP, TIMELINE_WHEEL_MAX_STEP)
}

fn moderate_timeline_wheel_scroll(ui: &mut egui::Ui) {
    if !ui.rect_contains_pointer(ui.available_rect_before_wrap()) {
        return;
    }

    ui.ctx().input_mut(|input| {
        input.smooth_scroll_delta.y = scaled_timeline_wheel_delta(input.smooth_scroll_delta.y);
    });
}

fn use_compact_timeline_scrollbars(ui: &mut egui::Ui) {
    let scroll = &mut ui.spacing_mut().scroll;
    scroll.floating = true;
    scroll.bar_width = 6.0;
    scroll.floating_width = 3.0;
    scroll.floating_allocated_width = 3.0;
    scroll.handle_min_length = 12.0;
    scroll.foreground_color = true;
}

#[derive(Clone)]
struct TimelineLayerThumbnailCache {
    document_session_id: u64,
    revision: u64,
    access_clock: u64,
    textures: std::collections::HashMap<(usize, usize), (egui::TextureHandle, u64)>,
}

impl TimelineLayerThumbnailCache {
    fn new(document_session_id: u64, revision: u64) -> Self {
        Self {
            document_session_id,
            revision,
            access_clock: 0,
            textures: std::collections::HashMap::new(),
        }
    }

    fn matches(&self, document_session_id: u64, revision: u64) -> bool {
        self.document_session_id == document_session_id && self.revision == revision
    }
}

fn timeline_layer_thumbnail_cache_id() -> egui::Id {
    egui::Id::new("pixelbuddy.timeline_layer_thumbnails")
}

fn layer_thumbnail_image(canvas: &crate::document::Canvas) -> egui::ColorImage {
    let mut image = egui::ColorImage::new(
        [TIMELINE_THUMBNAIL_SIZE, TIMELINE_THUMBNAIL_SIZE],
        egui::Color32::TRANSPARENT,
    );
    let width = canvas.width() as usize;
    let height = canvas.height() as usize;
    let scale = (TIMELINE_THUMBNAIL_SIZE as f32 / width as f32)
        .min(TIMELINE_THUMBNAIL_SIZE as f32 / height as f32);
    let draw_width = ((width as f32 * scale).round() as usize).clamp(1, TIMELINE_THUMBNAIL_SIZE);
    let draw_height = ((height as f32 * scale).round() as usize).clamp(1, TIMELINE_THUMBNAIL_SIZE);
    let offset_x = (TIMELINE_THUMBNAIL_SIZE - draw_width) / 2;
    let offset_y = (TIMELINE_THUMBNAIL_SIZE - draw_height) / 2;

    for target_y in 0..draw_height {
        let source_y = (target_y * height / draw_height).min(height - 1);
        for target_x in 0..draw_width {
            let source_x = (target_x * width / draw_width).min(width - 1);
            let color = canvas.get_pixel(source_x as u32, source_y as u32);
            image.pixels[(offset_y + target_y) * TIMELINE_THUMBNAIL_SIZE + offset_x + target_x] =
                egui::Color32::from_rgba_unmultiplied(color[0], color[1], color[2], color[3]);
        }
    }
    image
}

fn timeline_layer_thumbnail(
    ctx: &egui::Context,
    app: &PixelBuddyApp,
    frame_index: usize,
    layer_index: usize,
) -> Option<egui::TextureHandle> {
    let layer = app
        .editor
        .animation
        .frames
        .get(frame_index)?
        .document
        .layers
        .get(layer_index)?;
    let document_session_id = app.document_session_id();
    let revision = app.editor.revision();
    let cache_id = timeline_layer_thumbnail_cache_id();
    let key = (frame_index, layer_index);

    if let Some(texture) = ctx.data_mut(|data| {
        let mut cache = data
            .get_temp::<TimelineLayerThumbnailCache>(cache_id)
            .filter(|cache| cache.matches(document_session_id, revision))
            .unwrap_or_else(|| TimelineLayerThumbnailCache::new(document_session_id, revision));
        cache.access_clock = cache.access_clock.wrapping_add(1);
        let access = cache.access_clock;
        let texture = cache.textures.get_mut(&key).map(|(texture, last_used)| {
            *last_used = access;
            texture.clone()
        });
        data.insert_temp(cache_id, cache);
        texture
    }) {
        return Some(texture);
    }

    let options = egui::TextureOptions {
        magnification: egui::TextureFilter::Nearest,
        minification: egui::TextureFilter::Nearest,
        ..Default::default()
    };
    let texture = ctx.load_texture(
        format!(
            "pixelbuddy_timeline_layer_{document_session_id}_{revision}_{frame_index}_{layer_index}"
        ),
        layer_thumbnail_image(&layer.canvas),
        options,
    );
    ctx.data_mut(|data| {
        let mut cache = data
            .get_temp::<TimelineLayerThumbnailCache>(cache_id)
            .filter(|cache| cache.matches(document_session_id, revision))
            .unwrap_or_else(|| TimelineLayerThumbnailCache::new(document_session_id, revision));
        cache.access_clock = cache.access_clock.wrapping_add(1);
        let access = cache.access_clock;
        cache.textures.insert(key, (texture.clone(), access));
        while cache.textures.len() > MAX_TIMELINE_THUMBNAILS {
            let Some(oldest) = cache
                .textures
                .iter()
                .min_by_key(|(_, (_, last_used))| *last_used)
                .map(|(key, _)| *key)
            else {
                break;
            };
            cache.textures.remove(&oldest);
        }
        data.insert_temp(cache_id, cache);
    });
    Some(texture)
}

fn frame_index_at_x(frame_rects: &[egui::Rect], pointer_x: f32) -> Option<usize> {
    if !pointer_x.is_finite() {
        return None;
    }
    let first = frame_rects.first()?;
    let last_index = frame_rects.len() - 1;
    if pointer_x <= first.left() {
        return Some(0);
    }
    if pointer_x >= frame_rects[last_index].right() {
        return Some(last_index);
    }
    frame_rects
        .iter()
        .position(|rect| pointer_x < rect.right())
        .or(Some(last_index))
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum PendingFrameAction {
    SetPlayback { playing: bool, current_time: f64 },
    Stop,
    Add,
    Duplicate,
    Remove,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PendingLayerStructureAction {
    Add,
    Remove,
}

#[derive(Default)]
struct TimelinePendingActions {
    layer_visibility_changes: Vec<(usize, bool)>,
    layer_rename_change: Option<(usize, String)>,
    new_active_layer: Option<usize>,
    tag_action: Option<PendingTagAction>,
    layer_structure_action: Option<PendingLayerStructureAction>,
    frame_selection: Option<FrameSelectionRequest>,
    frame_action: Option<PendingFrameAction>,
}

fn apply_frame_selection(app: &mut PixelBuddyApp, request: FrameSelectionRequest) {
    app.select_frame(request.frame_index);

    if app.editor.animation.current_frame_index != request.frame_index {
        return;
    }

    let Some(layer_index) = request.active_layer_index else {
        return;
    };
    let max_layer = app.editor.document().layers.len().saturating_sub(1);
    let layer_index = layer_index.min(max_layer);
    if app.editor.document().active_layer_index != layer_index {
        app.select_layer_current_frame(layer_index);
    }
}

fn apply_pending_timeline_actions(app: &mut PixelBuddyApp, actions: TimelinePendingActions) {
    // Resolve edits against the frame/layer identity that rendered their UI
    // before any structural operation can shift indices underneath them.
    for (index, visible) in actions.layer_visibility_changes {
        app.set_layer_visibility_current_frame(index, visible);
    }

    if let Some((index, name)) = actions.layer_rename_change {
        app.rename_layer_current_frame(index, name);
    }

    if let Some(index) = actions.new_active_layer {
        let max_layer = app.editor.document().layers.len().saturating_sub(1);
        app.select_layer_current_frame(index.min(max_layer));
    }

    match actions.tag_action {
        Some(PendingTagAction::Create(mut tag)) => {
            if normalize_tag_range(&mut tag, app.editor.animation.frames.len()) {
                app.create_animation_tag(tag);
            }
        }
        Some(PendingTagAction::Update { index, mut tag }) => {
            if normalize_tag_range(&mut tag, app.editor.animation.frames.len()) {
                app.update_animation_tag(index, tag);
            }
        }
        Some(PendingTagAction::Remove(index)) => {
            app.remove_animation_tag(index);
        }
        None => {}
    }
    match actions.layer_structure_action {
        Some(PendingLayerStructureAction::Add) => {
            app.add_layer_all_frames();
        }
        Some(PendingLayerStructureAction::Remove) => {
            app.remove_active_layer_all_frames();
        }
        None => {}
    }

    if let Some(frame_action) = actions.frame_action {
        match frame_action {
            PendingFrameAction::SetPlayback {
                playing,
                current_time,
            } => {
                if app.editor.animation.is_playing != playing {
                    app.toggle_animation_playback(current_time);
                }
            }
            PendingFrameAction::Stop => {
                app.stop_animation();
            }
            PendingFrameAction::Add => {
                app.add_frame();
            }
            PendingFrameAction::Duplicate => {
                app.duplicate_frame();
            }
            PendingFrameAction::Remove => {
                app.remove_current_frame();
            }
        }
    } else if let Some(frame_selection) = actions.frame_selection {
        apply_frame_selection(app, frame_selection);
    }
}

pub fn show(ctx: &egui::Context, app: &mut PixelBuddyApp) {
    let mut pending_tag_action = None;
    let mut frame_selection = None;
    let mut layer_visibility_changes = Vec::new();
    let mut layer_rename_change = None;
    let mut new_active_layer = None;
    let mut pending_frame_action = None;
    let mut pending_layer_structure_action = None;

    let frame_count = app.editor.animation.frames.len();
    let current_frame = app.editor.animation.current_frame_index;
    let layers_count = app.editor.document().layers.len();
    let panel_max_height = timeline_panel_max_height(ctx.screen_rect().height());
    let panel_default_height = timeline_panel_default_height(layers_count, panel_max_height);
    let frame_range_id = timeline_frame_range_id(app.document_session_id());
    let tag_editor_id = timeline_tag_editor_id(app.document_session_id());
    let mut tag_editor = ctx.data(|data| data.get_temp::<TagEditorDraft>(tag_editor_id));
    let mut close_tag_editor = false;
    let mut save_tag_editor = false;

    let mut frame_range_selection = ctx
        .data(|data| data.get_temp::<FrameRangeSelection>(frame_range_id))
        .filter(|selection| selection.bounds(frame_count).is_some())
        .unwrap_or_else(|| FrameRangeSelection::single(current_frame));

    egui::TopBottomPanel::bottom("timeline_panel")
        .default_height(panel_default_height)
        .min_height(TIMELINE_PANEL_MIN_HEIGHT)
        .max_height(panel_max_height)
        .resizable(true)
        .show(ctx, |ui| {
            use_compact_timeline_scrollbars(ui);
            // TOP CONTROLS BAR (Play, FPS, Onion, etc.)
            ui.horizontal_wrapped(|ui| {
                ui.add_space(4.0);
                let play_icon = if app.editor.animation.is_playing { "⏸" } else { "▶" };
                if ui.button(play_icon).on_hover_text("Play/Pause Animation (Space)").clicked() {
                    let current_time = ctx.input(|input| input.time);
                    let playing = !app.editor.animation.is_playing;
                    pending_frame_action = Some(PendingFrameAction::SetPlayback {
                        playing,
                        current_time,
                    });
                }
                if ui.button("◼").on_hover_text("Stop Animation").clicked() {
                    pending_frame_action = Some(PendingFrameAction::Stop);
                }

                ui.separator();

                ui.label(egui::RichText::new("FPS:").size(11.0));
                let mut fps = app.editor.animation.fps as i32;
                if ui.add(egui::Slider::new(&mut fps, 1..=30).suffix(" fps")).changed() {
                    app.set_animation_fps(fps as u32, ctx.input(|input| input.time));
                }

                ui.separator();

                let mut onion_skin_enabled = app.editor.animation.onion_skin_enabled;
                if ui.checkbox(&mut onion_skin_enabled, "Onion Skin").clicked() {
                    app.set_onion_skin_enabled(onion_skin_enabled);
                }
                if onion_skin_enabled {
                    let mut onion_skin_opacity = app.editor.animation.onion_skin_opacity;
                    if ui.add(egui::Slider::new(&mut onion_skin_opacity, 0.0..=1.0).text("Opacity").show_value(false)).changed() {
                        app.set_onion_skin_opacity(onion_skin_opacity);
                        ctx.request_repaint();
                    }
                }

                ui.separator();

                let icon_size = egui::vec2(16.0, 16.0);
                let button_size = egui::vec2(24.0, 24.0);
                let text_color = ui.visuals().text_color();

                let add_img = egui::Image::new(egui::include_image!("../../assets/icons/plus.svg")).tint(text_color).fit_to_exact_size(icon_size);
                if ui.add(egui::Button::image(add_img).min_size(button_size)).on_hover_text("Add new blank frame").clicked() {
                    pending_frame_action = Some(PendingFrameAction::Add);
                }

                let dup_img = egui::Image::new(egui::include_image!("../../assets/icons/copy.svg")).tint(text_color).fit_to_exact_size(icon_size);
                if ui.add(egui::Button::image(dup_img).min_size(button_size)).on_hover_text("Duplicate current frame").clicked() {
                    pending_frame_action = Some(PendingFrameAction::Duplicate);
                }

                let del_img = egui::Image::new(egui::include_image!("../../assets/icons/trash.svg")).tint(text_color).fit_to_exact_size(icon_size);
                if ui.add(egui::Button::image(del_img).min_size(button_size)).on_hover_text("Delete current frame").clicked() {
                    pending_frame_action = Some(PendingFrameAction::Remove);
                }
                if let Some((from, to)) = frame_range_selection.bounds(frame_count) {
                    ui.separator();
                    let button_text = if from == to {
                        "+ Tag".to_owned()
                    } else {
                        format!("+ Tag {}–{}", from + 1, to + 1)
                    };
                    let hover_text = if from == to {
                        format!("Create a tag for frame {}", from + 1)
                    } else {
                        format!("Create a tag for frames {}–{}", from + 1, to + 1)
                    };
                    if ui.button(button_text).on_hover_text(hover_text).clicked() {
                        tag_editor = TagEditorDraft::for_creation(frame_range_selection, frame_count);
                    }
                }
            });
            ui.separator();

            // GRID LAYOUT
            let frame_count = app.editor.animation.frames.len();
            let current_frame = app.editor.animation.current_frame_index;
            let layers_count = app.editor.document().layers.len();
            let active_idx = app.editor.document().active_layer_index;

            moderate_timeline_wheel_scroll(ui);
            egui::ScrollArea::vertical()
                .id_salt("timeline_vscroll")
                .auto_shrink([false, false])
                .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing = egui::vec2(0.0, 0.0);
                    // LEFT COLUMN (Layers)
                    ui.vertical(|ui| {
                        ui.set_width(220.0);
                        // Header space
                        ui.allocate_ui(egui::vec2(220.0, TIMELINE_HEADER_HEIGHT), |ui| {
                            ui.with_layout(egui::Layout::bottom_up(egui::Align::LEFT), |ui| {
                                ui.label(egui::RichText::new("FRAMES").strong());
                            });
                        });

                        // Layer rows
                        for i in (0..layers_count).rev() {
                            ui.allocate_ui(egui::vec2(220.0, TIMELINE_ROW_HEIGHT), |ui| {
                                crate::ui::layers_panel::draw_layer_row_ui(
                                    ctx,
                                    app,
                                    ui,
                                    crate::ui::layers_panel::LayerRowUi {
                                        layer_index: i,
                                        active_layer_index: active_idx,
                                        frame_index: current_frame,
                                        visibility_changes: &mut layer_visibility_changes,
                                        rename_change: &mut layer_rename_change,
                                        new_active: &mut new_active_layer,
                                    },
                                );
                            });
                        }

                        ui.add_space(8.0);
                        ui.horizontal(|ui| {
                            let icon_size = egui::vec2(14.0, 14.0);
                            let text_color = ui.visuals().text_color();
                            let add_img = egui::Image::new(egui::include_image!("../../assets/icons/plus.svg")).tint(text_color).fit_to_exact_size(icon_size);
                            if ui.add(egui::Button::image(add_img)).on_hover_text("Add Layer").clicked() {
                                pending_layer_structure_action =
                                    Some(PendingLayerStructureAction::Add);
                            }

                            let del_img = egui::Image::new(egui::include_image!("../../assets/icons/trash.svg")).tint(text_color).fit_to_exact_size(icon_size);
                            if ui.add(egui::Button::image(del_img)).on_hover_text("Delete Layer").clicked() {
                                pending_layer_structure_action =
                                    Some(PendingLayerStructureAction::Remove);
                            }
                        });
                    });

                    ui.separator();

                    // RIGHT COLUMN (Frames)
                    egui::ScrollArea::horizontal().id_salt("timeline_hscroll").show(ui, |ui| {
                        ui.vertical(|ui| {
                            ui.spacing_mut().item_spacing = egui::vec2(0.0, 0.0);
                            // Compact frame-number headers. Thumbnails made this
                            // header look like a nonexistent extra layer track.
                            let tags_rect_start = ui.cursor().min;
                            let mut frame_rects = Vec::new();
                            let mut header_drag_active = false;
                            ui.add_space(16.0);
                            ui.horizontal(|ui| {
                                for f in 0..frame_count {
                                    let is_active = f == current_frame;
                                    let is_range_selected =
                                        frame_range_selection.contains(f, frame_count);
                                    let hover_text = format!(
                                        "Frame {}\nShift-click or drag to select a consecutive range\nRight-click to create a tag",
                                        f + 1
                                    );
                                    let frame_response = ui
                                        .add(
                                            egui::Button::new(
                                                egui::RichText::new((f + 1).to_string()).size(11.0),
                                            )
                                            .min_size(egui::vec2(32.0, 20.0))
                                            .sense(egui::Sense::click_and_drag()),
                                        )
                                        .on_hover_text(hover_text);

                                    if is_active || is_range_selected {
                                        let stroke_width = if is_active { 2.0_f32 } else { 1.0_f32 };
                                        let stroke_color = if is_active {
                                            ui.visuals().selection.bg_fill
                                        } else {
                                            ui.visuals().selection.stroke.color
                                        };
                                        ui.painter().rect_stroke(
                                            frame_response.rect,
                                            2.0,
                                            egui::Stroke::new(stroke_width, stroke_color),
                                            egui::StrokeKind::Inside,
                                        );
                                    }
                                    frame_rects.push(frame_response.rect);

                                    frame_response.context_menu(|ui| {
                                        let selected_range = if frame_range_selection
                                            .contains(f, frame_count)
                                        {
                                            frame_range_selection
                                        } else {
                                            FrameRangeSelection::single(f)
                                        };
                                        let (from, to) = selected_range
                                            .bounds(frame_count)
                                            .expect("the timeline always has a frame");
                                        let create_label = if from == to {
                                            format!("Create Tag for Frame {}", from + 1)
                                        } else {
                                            format!(
                                                "Create Tag for Frames {}–{}",
                                                from + 1,
                                                to + 1
                                            )
                                        };
                                        if ui.button(create_label).clicked() {
                                            frame_range_selection = selected_range;
                                            tag_editor = TagEditorDraft::for_creation(
                                                selected_range,
                                                frame_count,
                                            );
                                            ui.close_menu();
                                        }
                                    });

                                    if frame_response.drag_started() {
                                        let extend_existing =
                                            ui.input(|input| input.modifiers.shift);
                                        frame_range_selection = if extend_existing {
                                            frame_range_selection.extend_to(f)
                                        } else {
                                            FrameRangeSelection::single(f)
                                        };
                                    }
                                    header_drag_active |= frame_response.dragged();

                                    if frame_response.clicked() {
                                        let extend_existing =
                                            ui.input(|input| input.modifiers.shift);
                                        frame_range_selection = if extend_existing {
                                            frame_range_selection.extend_to(f)
                                        } else {
                                            FrameRangeSelection::single(f)
                                        };
                                        frame_selection = Some(FrameSelectionRequest {
                                            frame_index: f,
                                            active_layer_index: None,
                                        });
                                    }
                                }
                            });
                            if header_drag_active {
                                if let Some(pointer) = ctx.pointer_interact_pos() {
                                    if let Some(frame_index) =
                                        frame_index_at_x(&frame_rects, pointer.x)
                                    {
                                        frame_range_selection =
                                            frame_range_selection.extend_to(frame_index);
                                    }
                                }
                            }

                            // Render tags from an immutable snapshot. Edits are
                            // applied after the panel closes so tag controls cannot
                            // invalidate indices while they are still being drawn.
                            let painter = ui.painter().clone();
                            let tags = app.editor.animation.tags.clone();
                            for (tag_idx, tag) in tags.iter().enumerate() {
                                if tag.from_frame <= tag.to_frame
                                    && tag.to_frame < frame_rects.len()
                                {
                                    let start_rect = frame_rects[tag.from_frame];
                                    let end_rect = frame_rects[tag.to_frame];
                                    let tag_rect = egui::Rect::from_min_max(
                                        egui::pos2(start_rect.left(), tags_rect_start.y),
                                        egui::pos2(
                                            end_rect.right(),
                                            tags_rect_start.y + 16.0,
                                        ),
                                    );
                                    let color = egui::Color32::from_rgb(
                                        (tag.color[0] * 255.0) as u8,
                                        (tag.color[1] * 255.0) as u8,
                                        (tag.color[2] * 255.0) as u8,
                                    );
                                    painter.rect_filled(tag_rect, 4.0, color);
                                    painter.text(
                                        tag_rect.min + egui::vec2(4.0, 2.0),
                                        egui::Align2::LEFT_TOP,
                                        &tag.name,
                                        egui::FontId::proportional(10.0),
                                        egui::Color32::WHITE,
                                    );

                                    let tag_id = ui.id().with("tag").with(tag_idx);
                                    let tag_response = ui
                                        .interact(tag_rect, tag_id, egui::Sense::click())
                                        .on_hover_text(
                                            "Click to select this range; right-click to edit",
                                        );
                                    if tag_response.clicked() {
                                        frame_range_selection = FrameRangeSelection {
                                            anchor: tag.from_frame,
                                            focus: tag.to_frame,
                                        };
                                    }

                                    if tag_response.double_clicked() {
                                        tag_editor = Some(TagEditorDraft::for_edit(tag_idx, tag));
                                    }
                                    tag_response.context_menu(|ui| {
                                        if ui.button("Edit Tag…").clicked() {
                                            tag_editor =
                                                Some(TagEditorDraft::for_edit(tag_idx, tag));
                                            ui.close_menu();
                                        }
                                        if ui.button("Delete Tag").clicked() {
                                            pending_tag_action =
                                                Some(PendingTagAction::Remove(tag_idx));
                                            ui.close_menu();
                                        }
                                    });
                                }
                            }
                            // Layer rows (Grid)
                            for i in (0..layers_count).rev() {
                                ui.horizontal(|ui| {
                                    for f in 0..frame_count {
                                        ui.allocate_ui(
                                            egui::vec2(32.0, TIMELINE_ROW_HEIGHT),
                                            |ui| {
                                            if frame_has_layer(app, f, i) {
                                                ui.centered_and_justified(|ui| {
                                                    let (rect, response) = ui.allocate_exact_size(
                                                        egui::vec2(24.0, 24.0),
                                                        egui::Sense::click(),
                                                    );
                                                    let is_current_frame = f == current_frame;
                                                    let bg =
                                                        if is_current_frame && i == active_idx {
                                                            ui.visuals().selection.bg_fill
                                                        } else {
                                                            egui::Color32::from_gray(60)
                                                        };
                                                    ui.painter().rect_filled(rect, 2.0, bg);
                                                    if ui.is_rect_visible(rect) {
                                                        if let Some(texture) =
                                                            timeline_layer_thumbnail(ctx, app, f, i)
                                                        {
                                                            ui.painter().image(
                                                                texture.id(),
                                                                rect.shrink(1.0),
                                                                egui::Rect::from_min_max(
                                                                    egui::Pos2::ZERO,
                                                                    egui::pos2(1.0, 1.0),
                                                                ),
                                                                egui::Color32::WHITE,
                                                            );
                                                        }
                                                    }

                                                    if response.clicked() {
                                                        frame_selection =
                                                            Some(FrameSelectionRequest {
                                                                frame_index: f,
                                                                active_layer_index: Some(i),
                                                            });
                                                    }
                                                });
                                            }
                                            },
                                        );
                                    }
                                });
                            }
                        });
                    });
                });
                });
        });

    if let Some(draft) = tag_editor.as_mut() {
        app.canvas_input_blocked = true;
        let title = if draft.tag_index.is_some() {
            "Edit Animation Tag"
        } else {
            "Create Animation Tag"
        };
        let tag_capacity_available = draft.tag_index.is_some()
            || app.editor.animation.tags.len() < crate::document::animation::MAX_ANIMATION_TAGS;
        let valid = tag_capacity_available && draft.pending_action(frame_count).is_some();
        let modal = egui::Modal::new(tag_editor_id).show(ctx, |ui| {
            ui.set_min_width(300.0);
            ui.heading(title);
            ui.add_space(8.0);

            ui.label("Name");
            ui.text_edit_singleline(&mut draft.name);

            ui.add_space(6.0);
            ui.label("Color");
            ui.color_edit_button_rgb(&mut draft.color);

            ui.add_space(6.0);
            ui.label("Frames");
            ui.horizontal(|ui| {
                ui.label("Start");
                ui.add(egui::DragValue::new(&mut draft.start_frame).range(1..=frame_count.max(1)));
                ui.label("End");
                ui.add(egui::DragValue::new(&mut draft.end_frame).range(1..=frame_count.max(1)));
            });
            if draft.start_frame > draft.end_frame {
                ui.colored_label(
                    ui.visuals().error_fg_color,
                    "The start frame must not be after the end frame.",
                );
            } else if draft.name.trim().is_empty() {
                ui.colored_label(ui.visuals().error_fg_color, "Enter a tag name.");
            } else if draft.name.trim().len() > crate::document::animation::MAX_TAG_NAME_BYTES
                || draft.name.trim().chars().count()
                    > crate::document::animation::MAX_TAG_NAME_CHARS
            {
                ui.colored_label(
                    ui.visuals().error_fg_color,
                    format!(
                        "Tag names are limited to {} characters / {} UTF-8 bytes.",
                        crate::document::animation::MAX_TAG_NAME_CHARS,
                        crate::document::animation::MAX_TAG_NAME_BYTES
                    ),
                );
            } else if draft.name.trim().chars().any(char::is_control) {
                ui.colored_label(
                    ui.visuals().error_fg_color,
                    "Tag names cannot contain control characters.",
                );
            } else if !tag_capacity_available {
                ui.colored_label(
                    ui.visuals().error_fg_color,
                    format!(
                        "Animations are limited to {} tags.",
                        crate::document::animation::MAX_ANIMATION_TAGS
                    ),
                );
            }

            ui.add_space(10.0);
            ui.horizontal(|ui| {
                if ui.button("Cancel").clicked() {
                    close_tag_editor = true;
                }
                let save_label = if draft.tag_index.is_some() {
                    "Save"
                } else {
                    "Create"
                };
                if ui
                    .add_enabled(valid, egui::Button::new(save_label))
                    .clicked()
                {
                    save_tag_editor = true;
                }
            });
        });
        close_tag_editor |= modal.should_close();
    }

    if save_tag_editor {
        pending_tag_action = tag_editor
            .as_ref()
            .and_then(|draft| draft.pending_action(frame_count));
        close_tag_editor = true;
    }

    ctx.data_mut(|data| {
        data.insert_temp(frame_range_id, frame_range_selection);
        if close_tag_editor {
            data.remove::<TagEditorDraft>(tag_editor_id);
        } else if let Some(draft) = tag_editor {
            data.insert_temp(tag_editor_id, draft);
        }
    });
    apply_pending_timeline_actions(
        app,
        TimelinePendingActions {
            layer_visibility_changes,
            layer_rename_change,
            new_active_layer,
            tag_action: pending_tag_action,
            layer_structure_action: pending_layer_structure_action,
            frame_selection,
            frame_action: pending_frame_action,
        },
    );
}

#[cfg(test)]
mod tests {
    use super::{
        apply_frame_selection, apply_pending_timeline_actions, frame_has_layer, frame_index_at_x,
        layer_thumbnail_image, scaled_timeline_wheel_delta, timeline_layer_thumbnail,
        timeline_layer_thumbnail_cache_id, timeline_panel_default_height,
        timeline_panel_max_height, FrameRangeSelection, FrameSelectionRequest, PendingFrameAction,
        PendingLayerStructureAction, PendingTagAction, TagEditorDraft, TimelineLayerThumbnailCache,
        TimelinePendingActions, MAX_TIMELINE_THUMBNAILS, TIMELINE_PANEL_ABSOLUTE_MAX_HEIGHT,
        TIMELINE_PANEL_MIN_HEIGHT, TIMELINE_ROW_HEIGHT,
    };
    use crate::app::PixelBuddyApp;
    use crate::document::animation::{FrameTag, MAX_TAG_NAME_BYTES, MAX_TAG_NAME_CHARS};

    fn app_with_distinct_frame_layer_selections() -> PixelBuddyApp {
        let mut app = PixelBuddyApp::new(2, 2);
        app.editor.document_mut().add_layer();
        app.editor.document_mut().active_layer_index = 0;
        app.editor.duplicate_frame();
        app.editor.document_mut().active_layer_index = 1;
        assert!(app.editor.select_frame(0));
        app
    }

    #[test]
    fn timeline_panel_height_is_layer_aware_and_screen_bounded() {
        let max_height = timeline_panel_max_height(600.0);
        assert!(max_height > TIMELINE_PANEL_MIN_HEIGHT);
        assert!(max_height <= TIMELINE_PANEL_ABSOLUTE_MAX_HEIGHT);

        let one_layer = timeline_panel_default_height(1, max_height);
        let two_layers = timeline_panel_default_height(2, max_height);
        let many_layers = timeline_panel_default_height(100, max_height);
        assert!(one_layer >= TIMELINE_PANEL_MIN_HEIGHT);
        assert!(two_layers >= one_layer + TIMELINE_ROW_HEIGHT);
        assert!(many_layers <= max_height);
    }

    #[test]
    fn timeline_wheel_delta_is_gradual_and_bounded_to_one_row_per_frame() {
        assert_eq!(scaled_timeline_wheel_delta(8.0), 2.0);
        assert_eq!(scaled_timeline_wheel_delta(120.0), 30.0);
        assert_eq!(scaled_timeline_wheel_delta(1_000.0), TIMELINE_ROW_HEIGHT);
        assert_eq!(scaled_timeline_wheel_delta(-1_000.0), -TIMELINE_ROW_HEIGHT);
        assert_eq!(scaled_timeline_wheel_delta(f32::NAN), 0.0);
    }

    #[test]
    fn layer_thumbnail_preserves_sprite_pixels_and_transparency() {
        let mut canvas = crate::document::Canvas::new(2, 1);
        canvas.set_pixel(0, 0, [220, 30, 40, 255]);

        let image = layer_thumbnail_image(&canvas);

        assert_eq!(image.size, [24, 24]);
        assert_eq!(image.pixels[6 * 24], egui::Color32::from_rgb(220, 30, 40));
        assert_eq!(image.pixels[6 * 24 + 23], egui::Color32::TRANSPARENT);
    }

    #[test]
    fn timeline_thumbnail_cache_contains_only_layers_that_exist_in_each_frame() {
        let ctx = egui::Context::default();
        let mut app = PixelBuddyApp::new(2, 2);
        app.editor.duplicate_frame();
        app.editor.document_mut().add_layer();

        assert!(timeline_layer_thumbnail(&ctx, &app, 0, 0).is_some());
        assert!(timeline_layer_thumbnail(&ctx, &app, 0, 1).is_none());
        let blank_texture = timeline_layer_thumbnail(&ctx, &app, 1, 1)
            .expect("the second frame has a second layer")
            .id();
        let cache = ctx
            .data(|data| {
                data.get_temp::<TimelineLayerThumbnailCache>(timeline_layer_thumbnail_cache_id())
            })
            .expect("requested thumbnails should be cached");
        assert_eq!(cache.textures.len(), 2);

        app.apply_tool_changes(vec![(0, 0, [10, 20, 30, 255])]);
        let changed_texture = timeline_layer_thumbnail(&ctx, &app, 1, 1)
            .expect("the edited layer still exists")
            .id();
        assert_ne!(changed_texture, blank_texture);
    }

    #[test]
    fn timeline_thumbnail_cache_evicts_old_entries_at_its_gpu_budget() {
        let ctx = egui::Context::default();
        let mut app = PixelBuddyApp::new(1, 1);
        for _ in 0..MAX_TIMELINE_THUMBNAILS {
            app.editor.animation.duplicate_frame();
        }
        for frame_index in 0..app.editor.animation.frames.len() {
            assert!(timeline_layer_thumbnail(&ctx, &app, frame_index, 0).is_some());
        }

        let cache = ctx
            .data(|data| {
                data.get_temp::<TimelineLayerThumbnailCache>(timeline_layer_thumbnail_cache_id())
            })
            .expect("requested thumbnails should be cached");
        assert_eq!(cache.textures.len(), MAX_TIMELINE_THUMBNAILS);
        assert!(!cache.textures.contains_key(&(0, 0)));
        assert!(cache
            .textures
            .contains_key(&(app.editor.animation.frames.len() - 1, 0)));
    }
    #[test]
    fn tag_editor_prompts_with_selection_and_commits_exact_one_based_range() {
        let selection = FrameRangeSelection::single(0).extend_to(6);
        let mut draft = TagEditorDraft::for_creation(selection, 9).expect("frames exist");
        assert_eq!((draft.start_frame, draft.end_frame), (1, 7));

        draft.tag_index = Some(0);
        draft.name = "Run".to_owned();
        let action = draft.pending_action(9).expect("valid range");
        assert_eq!(
            action,
            PendingTagAction::Update {
                index: 0,
                tag: FrameTag {
                    name: "Run".to_owned(),
                    color: [0.8, 0.2, 0.2],
                    from_frame: 0,
                    to_frame: 6,
                },
            }
        );

        draft.start_frame = 8;
        draft.end_frame = 7;
        assert!(draft.pending_action(9).is_none());
    }

    #[test]
    fn tag_editor_rejects_overlong_and_control_character_names() {
        let mut draft =
            TagEditorDraft::for_creation(FrameRangeSelection::single(0), 1).expect("frame exists");

        draft.name = "x".repeat(MAX_TAG_NAME_BYTES + 1);
        assert!(draft.pending_action(1).is_none());

        draft.name = "é".repeat(MAX_TAG_NAME_CHARS + 1);
        assert!(draft.pending_action(1).is_none());

        draft.name = "Run\nFast".to_owned();
        assert!(draft.pending_action(1).is_none());
    }

    #[test]
    fn frame_range_selection_is_always_one_consecutive_interval() {
        let selection = FrameRangeSelection::single(4).extend_to(1);

        assert_eq!(selection.bounds(8), Some((1, 4)));
        assert!(selection.contains(1, 8));
        assert!(selection.contains(3, 8));
        assert!(selection.contains(4, 8));
        assert!(!selection.contains(0, 8));
        assert!(!selection.contains(5, 8));

        // Structural frame edits clamp stale endpoints without creating gaps.
        assert_eq!(selection.bounds(3), Some((1, 2)));
        assert_eq!(selection.bounds(0), None);
    }

    #[test]
    fn frame_range_drag_lookup_clamps_and_uses_the_positive_side_of_a_seam() {
        let frame_rects = [
            egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(32.0, 32.0)),
            egui::Rect::from_min_max(egui::pos2(32.0, 0.0), egui::pos2(64.0, 32.0)),
            egui::Rect::from_min_max(egui::pos2(64.0, 0.0), egui::pos2(96.0, 32.0)),
        ];

        assert_eq!(frame_index_at_x(&frame_rects, -20.0), Some(0));
        assert_eq!(frame_index_at_x(&frame_rects, 31.9), Some(0));
        assert_eq!(frame_index_at_x(&frame_rects, 32.0), Some(1));
        assert_eq!(frame_index_at_x(&frame_rects, 200.0), Some(2));
        assert_eq!(frame_index_at_x(&[], 10.0), None);
        assert_eq!(frame_index_at_x(&frame_rects, f32::NAN), None);
    }

    #[test]
    fn tag_actions_create_edit_and_delete_consecutive_dirty_ranges() {
        let mut app = PixelBuddyApp::new(2, 2);
        for _ in 1..5 {
            app.editor.animation.duplicate_frame();
        }
        app.editor.mark_saved();

        apply_pending_timeline_actions(
            &mut app,
            TimelinePendingActions {
                tag_action: Some(PendingTagAction::Create(FrameTag {
                    name: "Run".to_owned(),
                    color: [0.8, 0.2, 0.2],
                    from_frame: 4,
                    to_frame: 1,
                })),
                ..TimelinePendingActions::default()
            },
        );

        assert_eq!(
            app.editor.animation.tags,
            vec![FrameTag {
                name: "Run".to_owned(),
                color: [0.8, 0.2, 0.2],
                from_frame: 1,
                to_frame: 4,
            }]
        );
        assert!(app.editor.is_dirty());

        app.editor.mark_saved();
        let revision_before_edit = app.editor.revision();
        let edited = FrameTag {
            name: "Run Loop".to_owned(),
            color: [0.2, 0.7, 0.9],
            from_frame: 2,
            to_frame: 3,
        };
        apply_pending_timeline_actions(
            &mut app,
            TimelinePendingActions {
                tag_action: Some(PendingTagAction::Update {
                    index: 0,
                    tag: edited.clone(),
                }),
                ..TimelinePendingActions::default()
            },
        );
        assert_eq!(app.editor.animation.tags, vec![edited.clone()]);
        assert_eq!(app.editor.revision(), revision_before_edit.wrapping_add(1));
        assert!(app.editor.is_dirty());

        app.editor.mark_saved();
        let revision_before_noop = app.editor.revision();
        apply_pending_timeline_actions(
            &mut app,
            TimelinePendingActions {
                tag_action: Some(PendingTagAction::Update {
                    index: 0,
                    tag: edited,
                }),
                ..TimelinePendingActions::default()
            },
        );
        assert_eq!(app.editor.revision(), revision_before_noop);
        assert!(!app.editor.is_dirty());

        apply_pending_timeline_actions(
            &mut app,
            TimelinePendingActions {
                tag_action: Some(PendingTagAction::Remove(0)),
                ..TimelinePendingActions::default()
            },
        );
        assert!(app.editor.animation.tags.is_empty());
        assert!(app.editor.is_dirty());
    }

    #[test]
    fn nonexistent_layers_do_not_have_timeline_cells() {
        let mut app = PixelBuddyApp::new(2, 2);
        app.editor.animation.duplicate_frame();
        app.editor.document_mut().add_layer();

        assert!(frame_has_layer(&app, 0, 0));
        assert!(!frame_has_layer(&app, 0, 1));
        assert!(frame_has_layer(&app, 1, 0));
        assert!(frame_has_layer(&app, 1, 1));
        assert!(!frame_has_layer(&app, 99, 0));
    }

    #[test]
    fn frame_header_selection_preserves_the_target_frames_active_layer() {
        let mut app = app_with_distinct_frame_layer_selections();

        apply_frame_selection(
            &mut app,
            FrameSelectionRequest {
                frame_index: 1,
                active_layer_index: None,
            },
        );

        assert_eq!(app.editor.animation.current_frame_index, 1);
        assert_eq!(app.editor.document().active_layer_index, 1);
    }

    #[test]
    fn timeline_grid_selection_chooses_the_requested_target_layer() {
        let mut app = app_with_distinct_frame_layer_selections();

        apply_frame_selection(
            &mut app,
            FrameSelectionRequest {
                frame_index: 1,
                active_layer_index: Some(0),
            },
        );

        assert_eq!(app.editor.animation.current_frame_index, 1);
        assert_eq!(app.editor.document().active_layer_index, 0);
    }

    #[test]
    fn timeline_selection_consumes_the_app_level_transition_effects() {
        let mut app = app_with_distinct_frame_layer_selections();
        app.apply_tool_changes(vec![(0, 0, [1, 2, 3, 255])]);
        app.editor.selection.set_rect(0, 0, 0, 0);
        app.begin_canvas_action(1, 1);
        app.preview_changes.push((1, 1, [4, 5, 6, 255]));
        app.texture_dirty = false;
        let generation = app.active_frame_generation();

        apply_frame_selection(
            &mut app,
            FrameSelectionRequest {
                frame_index: 1,
                active_layer_index: None,
            },
        );

        assert_eq!(app.editor.animation.current_frame_index, 1);
        assert!(!app.editor.history.can_undo());
        assert!(!app.editor.selection.active);
        assert!(!app.is_drawing);
        assert!(app.preview_changes.is_empty());
        assert!(app.last_canvas_pixel.is_none());
        assert!(app.texture_dirty);
        assert_eq!(app.active_frame_generation(), generation.wrapping_add(1));
    }

    #[test]
    fn source_frame_edits_are_applied_before_timeline_selection() {
        let mut app = app_with_distinct_frame_layer_selections();
        app.editor.animation.frames[0].document.layers[0].name = "Source".to_owned();
        app.editor.animation.frames[1].document.layers[0].name = "Target".to_owned();

        apply_pending_timeline_actions(
            &mut app,
            TimelinePendingActions {
                layer_visibility_changes: vec![(0, false)],
                layer_rename_change: Some((0, "Hero".to_owned())),
                frame_selection: Some(FrameSelectionRequest {
                    frame_index: 1,
                    active_layer_index: None,
                }),
                ..TimelinePendingActions::default()
            },
        );

        assert_eq!(app.editor.animation.current_frame_index, 1);
        assert_eq!(
            app.editor.animation.frames[0].document.layers[0].name,
            "Hero"
        );
        assert!(!app.editor.animation.frames[0].document.layers[0].visible);
        assert_eq!(
            app.editor.animation.frames[1].document.layers[0].name,
            "Target"
        );
        assert!(app.editor.animation.frames[1].document.layers[0].visible);
    }

    #[test]
    fn source_rename_precedes_frame_structure_and_stop_actions() {
        let mut added = app_with_distinct_frame_layer_selections();
        apply_pending_timeline_actions(
            &mut added,
            TimelinePendingActions {
                layer_rename_change: Some((0, "Hero".to_owned())),
                frame_action: Some(PendingFrameAction::Add),
                ..TimelinePendingActions::default()
            },
        );
        assert_eq!(
            added.editor.animation.frames[0].document.layers[0].name,
            "Hero"
        );
        assert_eq!(added.editor.document().layers[0].name, "Hero");

        let mut stopped = app_with_distinct_frame_layer_selections();
        assert!(stopped.select_frame(1));
        apply_pending_timeline_actions(
            &mut stopped,
            TimelinePendingActions {
                layer_rename_change: Some((0, "Preview".to_owned())),
                frame_action: Some(PendingFrameAction::Stop),
                ..TimelinePendingActions::default()
            },
        );
        assert_eq!(stopped.editor.animation.current_frame_index, 0);
        assert_eq!(
            stopped.editor.animation.frames[1].document.layers[0].name,
            "Preview"
        );
        assert_ne!(
            stopped.editor.animation.frames[0].document.layers[0].name,
            "Preview"
        );

        let mut played = app_with_distinct_frame_layer_selections();
        apply_pending_timeline_actions(
            &mut played,
            TimelinePendingActions {
                layer_rename_change: Some((0, "Playback".to_owned())),
                frame_action: Some(PendingFrameAction::SetPlayback {
                    playing: true,
                    current_time: 5.0,
                }),
                ..TimelinePendingActions::default()
            },
        );
        assert_eq!(played.editor.document().layers[0].name, "Playback");
        assert!(played.editor.animation.is_playing);

        let mut paused = app_with_distinct_frame_layer_selections();
        paused.toggle_animation_playback(0.0);
        assert!(paused.editor.animation.is_playing);
        apply_pending_timeline_actions(
            &mut paused,
            TimelinePendingActions {
                layer_rename_change: Some((0, "Paused".to_owned())),
                frame_action: Some(PendingFrameAction::SetPlayback {
                    playing: false,
                    current_time: 1.0,
                }),
                ..TimelinePendingActions::default()
            },
        );
        assert_eq!(paused.editor.document().layers[0].name, "Paused");
        assert!(!paused.editor.animation.is_playing);
    }

    #[test]
    fn layer_delete_cannot_apply_a_pending_rename_to_the_shifted_neighbor() {
        let mut app = PixelBuddyApp::new(2, 2);
        app.editor.document_mut().layers[0].name = "A".to_owned();
        app.editor.document_mut().add_layer();
        app.editor.document_mut().layers[1].name = "B".to_owned();
        app.editor.document_mut().add_layer();
        app.editor.document_mut().layers[2].name = "C".to_owned();
        app.editor.document_mut().active_layer_index = 1;

        apply_pending_timeline_actions(
            &mut app,
            TimelinePendingActions {
                layer_rename_change: Some((1, "Hero".to_owned())),
                layer_structure_action: Some(PendingLayerStructureAction::Remove),
                ..TimelinePendingActions::default()
            },
        );

        let names: Vec<_> = app
            .editor
            .document()
            .layers
            .iter()
            .map(|layer| layer.name.as_str())
            .collect();
        assert_eq!(names, vec!["A", "C"]);
    }
}

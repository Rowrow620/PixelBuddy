#[cfg(not(target_arch = "wasm32"))]
use super::WindowPresentation;
use super::{
    load_recovery_snapshot, load_view_preferences, parse_raster_export_dimension,
    parse_raster_export_scale, recovery_snapshot_within_budget, shortcut_permissions,
    DocumentReplacement, EditEffects, FrameThumbnailInvalidation, PixelBuddyApp, RasterExportKind,
    RasterExportSizing, ShortcutPermissions, SpriteSheetImportMode, TileMode, ViewPreferences,
    MAX_TILE_PREVIEW_COUNT, RECOVERY_STORAGE_KEY, VIEW_PREFERENCES_STORAGE_KEY,
};
use crate::document::{AnimationManager, BlendMode, Document};
use crate::editor::{ClipboardBuffer, EditorState};
use crate::io::{ExportFormat, ProjectSaveSource};
use eframe::Storage;
use std::collections::BTreeMap;

#[derive(Default)]
struct TestStorage {
    values: BTreeMap<String, String>,
    flush_count: usize,
}

impl eframe::Storage for TestStorage {
    fn get_string(&self, key: &str) -> Option<String> {
        self.values.get(key).cloned()
    }

    fn set_string(&mut self, key: &str, value: String) {
        self.values.insert(key.to_owned(), value);
    }

    fn flush(&mut self) {
        self.flush_count += 1;
    }
}

#[derive(Debug, PartialEq)]
struct FrameTransitionState {
    encoded_project: Vec<u8>,
    revision: u64,
    current_frame_index: usize,
    undo: Vec<String>,
    redo: Vec<String>,
    selection_active: bool,
    pixel_clipboard_present: bool,
    frame_clipboard_present: bool,
    is_drawing: bool,
    stroke_points: Vec<(u32, u32)>,
    shape_start: Option<(i32, i32)>,
    last_canvas_pixel: Option<(i32, i32)>,
    preview_changes: Vec<crate::tools::PixelChange>,
    texture_dirty: bool,
    onion_texture_pair: Option<(usize, usize)>,
    active_frame_generation: u64,
    is_playing: bool,
}

fn frame_transition_state(app: &PixelBuddyApp) -> FrameTransitionState {
    FrameTransitionState {
        encoded_project: crate::io::project::encode_editor_bytes(&app.editor)
            .expect("the frame-transition fixture should encode"),
        revision: app.editor.revision(),
        current_frame_index: app.editor.animation.current_frame_index,
        undo: app.editor.history.undo_descriptions(),
        redo: app.editor.history.redo_descriptions(),
        selection_active: app.editor.selection.active,
        pixel_clipboard_present: app.editor.clipboard.is_some(),
        frame_clipboard_present: app.editor.has_copied_frame(),
        is_drawing: app.is_drawing,
        stroke_points: app.stroke_points.clone(),
        shape_start: app.shape_start,
        last_canvas_pixel: app.last_canvas_pixel,
        preview_changes: app.preview_changes.clone(),
        texture_dirty: app.texture_dirty,
        onion_texture_pair: app.onion_texture_pair,
        active_frame_generation: app.active_frame_generation(),
        is_playing: app.editor.animation.is_playing,
    }
}

fn two_frame_app(active_frame_index: usize) -> PixelBuddyApp {
    let mut app = PixelBuddyApp::new(2, 2);
    app.editor.add_frame();
    if active_frame_index == 0 {
        assert!(app.editor.select_frame(0));
    }
    app.editor.mark_saved();
    app
}

fn two_frame_animation_with_pixel(pixel: [u8; 4]) -> AnimationManager {
    let mut animation = AnimationManager::new(Document::new(2, 2));
    animation
        .current_doc_mut()
        .active_layer_mut()
        .canvas
        .set_pixel(0, 0, pixel);
    animation.duplicate_frame();
    animation
        .current_doc_mut()
        .active_layer_mut()
        .canvas
        .set_pixel(0, 0, pixel);
    animation
}

fn seed_frame_local_state(app: &mut PixelBuddyApp) {
    app.apply_tool_changes(vec![(0, 0, [255, 0, 0, 255])]);
    app.apply_tool_changes(vec![(0, 0, [0, 0, 255, 255])]);
    assert!(app.editor.undo());
    app.editor.selection.set_rect(0, 0, 0, 0);
    app.editor.clipboard = Some(ClipboardBuffer {
        pixels: vec![[1, 2, 3, 255]],
        width: 1,
        height: 1,
    });
    app.editor.copy_current_frame();
    app.begin_canvas_action(1, 1);
    app.preview_changes.push((1, 1, [4, 5, 6, 255]));
    app.texture_dirty = false;
    app.onion_texture_pair = Some((7, 9));
    app.editor.mark_saved();
}

#[test]
fn cancelling_a_canvas_action_discards_only_transient_state() {
    let mut app = PixelBuddyApp::new(8, 8);
    app.begin_canvas_action_on_tile((3, 4), (1, 0), (11, 4));
    app.preview_changes.push((3, 4, [1, 2, 3, 255]));

    app.cancel_canvas_action();

    assert!(!app.is_drawing);
    assert!(app.shape_start.is_none());
    assert!(app.stroke_points.is_empty());
    assert!(app.preview_changes.is_empty());
    assert_eq!(app.last_canvas_pixel, Some((3, 4)));
    assert!(app.canvas_action_last_pixel.is_none());
    assert!(app.canvas_action_tile_offset.is_none());
    assert!(app.canvas_action_virtual_points.is_empty());
}

#[test]
fn starting_playback_cancels_a_partial_marquee() {
    let mut app = two_frame_app(0);
    app.set_active_tool(crate::editor::ToolType::Marquee);
    app.begin_canvas_action(0, 0);
    app.editor.selection.set_rect(0, 0, 1, 1);

    app.toggle_animation_playback(0.0);

    assert!(app.editor.animation.is_playing);
    assert!(!app.is_drawing);
    assert!(!app.editor.selection.active);
    assert!(app.stroke_points.is_empty());
    assert!(app.preview_changes.is_empty());
}

#[test]
fn manual_frame_navigation_does_not_wrap_or_disturb_boundary_state() {
    let mut first = two_frame_app(0);
    seed_frame_local_state(&mut first);
    first.editor.animation.toggle_play(0.0);
    let before_first = frame_transition_state(&first);

    assert!(!first.select_previous_frame());
    assert_eq!(frame_transition_state(&first), before_first);

    let mut last = two_frame_app(1);
    seed_frame_local_state(&mut last);
    last.editor.animation.toggle_play(0.0);
    let before_last = frame_transition_state(&last);

    assert!(!last.select_next_frame());
    assert_eq!(frame_transition_state(&last), before_last);

    let mut single = PixelBuddyApp::new(2, 2);
    seed_frame_local_state(&mut single);
    let before_single = frame_transition_state(&single);

    assert!(!single.select_previous_frame());
    assert!(!single.select_next_frame());
    assert!(!single.select_frame(0));
    assert!(!single.select_frame(1));
    assert!(!single.remove_current_frame());
    assert_eq!(frame_transition_state(&single), before_single);
}

#[test]
fn keyboard_and_timeline_frame_commands_have_identical_safe_effects() {
    let mut keyboard = two_frame_app(0);
    seed_frame_local_state(&mut keyboard);
    keyboard.editor.animation.toggle_play(10.0);
    let keyboard_revision = keyboard.editor.revision();
    let keyboard_generation = keyboard.active_frame_generation();

    let mut timeline = two_frame_app(0);
    seed_frame_local_state(&mut timeline);
    timeline.editor.animation.toggle_play(10.0);
    let timeline_revision = timeline.editor.revision();
    let timeline_generation = timeline.active_frame_generation();

    assert!(keyboard.select_next_frame());
    assert!(timeline.select_frame(1));

    assert_eq!(
        frame_transition_state(&keyboard),
        frame_transition_state(&timeline)
    );
    assert_eq!(
        keyboard.editor.revision(),
        keyboard_revision.wrapping_add(1)
    );
    assert_eq!(
        timeline.editor.revision(),
        timeline_revision.wrapping_add(1)
    );
    assert_eq!(
        keyboard.active_frame_generation(),
        keyboard_generation.wrapping_add(1)
    );
    assert_eq!(
        timeline.active_frame_generation(),
        timeline_generation.wrapping_add(1)
    );
    assert!(!keyboard.editor.history.can_undo());
    assert!(!keyboard.editor.history.can_redo());
    assert!(!keyboard.editor.selection.active);
    assert!(!keyboard.is_drawing);
    assert!(keyboard.preview_changes.is_empty());
    assert!(keyboard.last_canvas_pixel.is_none());
    assert!(keyboard.texture_dirty);
    assert!(keyboard.onion_texture_pair.is_none());
    assert!(!keyboard.editor.animation.is_playing);
    assert!(keyboard.editor.clipboard.is_some());
    assert!(keyboard.editor.has_copied_frame());
    assert_eq!(
        keyboard
            .editor
            .document()
            .active_layer()
            .canvas
            .get_pixel(0, 0),
        [0, 0, 0, 0]
    );
}

#[test]
fn playback_invalidates_the_dirty_outgoing_thumbnail() {
    let mut app = two_frame_app(0);
    let ctx = egui::Context::default();
    let texture = |name: &str| {
        ctx.load_texture(
            name,
            egui::ColorImage::new([1, 1], egui::Color32::WHITE),
            egui::TextureOptions::NEAREST,
        )
    };
    app.frame_thumbnails = vec![
        Some(texture("dirty_outgoing")),
        Some(texture("clean_target")),
    ];
    app.texture_dirty = true;
    app.editor.animation.toggle_play(0.0);

    assert!(app.update_animation_playback(0.2));
    assert!(app.frame_thumbnails[0].is_none());
    assert!(app.frame_thumbnails[1].is_some());
}

#[test]
fn playback_advance_uses_safe_effects_without_dirtying_the_project() {
    let mut app = two_frame_app(0);
    seed_frame_local_state(&mut app);
    let encoded_selection = crate::io::project::encode_editor_bytes(&app.editor)
        .expect("the editing selection should encode");
    let revision = app.editor.revision();
    let generation = app.active_frame_generation();
    app.editor.animation.toggle_play(0.0);

    assert!(app.update_animation_playback(0.2));

    assert_eq!(app.editor.animation.current_frame_index, 1);
    assert!(app.editor.animation.is_playing);
    assert_eq!(app.editor.revision(), revision);
    assert!(!app.editor.is_dirty());
    assert_eq!(
        crate::io::project::encode_editor_bytes(&app.editor)
            .expect("preview playback should remain encodable"),
        encoded_selection
    );
    assert!(!app.editor.history.can_undo());
    assert!(!app.editor.history.can_redo());
    assert!(!app.editor.selection.active);
    assert!(!app.is_drawing);
    assert!(app.preview_changes.is_empty());
    assert!(app.last_canvas_pixel.is_none());
    assert!(app.texture_dirty);
    assert!(app.onion_texture_pair.is_none());
    assert_eq!(app.active_frame_generation(), generation.wrapping_add(1));
    assert!(!app.current_project_import_is_current(
        Some(app.document_session_id()),
        None,
        Some(generation),
        None,
        "image",
    ));
}

#[test]
fn pausing_playback_adopts_the_preview_frame_as_a_dirty_selection() {
    let mut app = two_frame_app(0);
    let encoded_selection = crate::io::project::encode_editor_bytes(&app.editor)
        .expect("the original editing selection should encode");
    let revision = app.editor.revision();

    app.toggle_animation_playback(0.0);
    assert!(app.update_animation_playback(0.2));
    assert!(!app.editor.is_dirty());

    app.toggle_animation_playback(0.2);

    assert!(!app.editor.animation.is_playing);
    assert_eq!(app.editor.animation.current_frame_index, 1);
    assert_eq!(app.editor.revision(), revision.wrapping_add(1));
    assert!(app.editor.is_dirty());
    assert_ne!(
        crate::io::project::encode_editor_bytes(&app.editor)
            .expect("the adopted editing selection should encode"),
        encoded_selection
    );
}

#[test]
fn stopping_preview_restores_the_clean_editing_selection() {
    let mut app = two_frame_app(0);
    let encoded_selection = crate::io::project::encode_editor_bytes(&app.editor)
        .expect("the original editing selection should encode");
    let revision = app.editor.revision();
    app.toggle_animation_playback(0.0);
    assert!(app.update_animation_playback(0.2));

    assert!(app.stop_animation());

    assert_eq!(app.editor.animation.current_frame_index, 0);
    assert!(!app.editor.animation.is_playing);
    assert_eq!(app.editor.revision(), revision);
    assert!(!app.editor.is_dirty());
    assert_eq!(
        crate::io::project::encode_editor_bytes(&app.editor)
            .expect("stopping preview should restore the saved selection"),
        encoded_selection
    );
}

#[test]
fn playback_full_loop_still_clears_frame_bound_state() {
    let mut app = two_frame_app(0);
    app.editor.animation.frames[0].duration_ms = 100;
    app.editor.animation.frames[1].duration_ms = 100;
    seed_frame_local_state(&mut app);
    let generation = app.active_frame_generation();
    app.editor.animation.toggle_play(0.0);

    assert!(app.update_animation_playback(0.21));

    // Two playback steps return to the same numeric index, but the app
    // still crossed frame boundaries and must invalidate transient state.
    assert_eq!(app.editor.animation.current_frame_index, 0);
    assert!(!app.editor.history.can_undo());
    assert!(!app.editor.history.can_redo());
    assert!(!app.editor.selection.active);
    assert!(!app.is_drawing);
    assert!(app.preview_changes.is_empty());
    assert!(app.last_canvas_pixel.is_none());
    assert!(app.texture_dirty);
    assert!(app.onion_texture_pair.is_none());
    assert_eq!(app.active_frame_generation(), generation.wrapping_add(1));
    assert!(!app.current_project_import_is_current(
        Some(app.document_session_id()),
        None,
        Some(generation),
        None,
        "image",
    ));
}

#[test]
fn stopping_animation_returns_to_frame_zero_through_the_safe_transition() {
    let mut app = two_frame_app(1);
    seed_frame_local_state(&mut app);
    app.editor.animation.toggle_play(0.0);
    let revision = app.editor.revision();

    assert!(app.stop_animation());

    assert_eq!(app.editor.animation.current_frame_index, 0);
    assert!(!app.editor.animation.is_playing);
    assert_eq!(app.editor.revision(), revision.wrapping_add(1));
    assert!(!app.editor.history.can_undo());
    assert!(!app.editor.history.can_redo());
    assert!(!app.editor.selection.active);
    assert!(!app.is_drawing);
    assert!(app.last_canvas_pixel.is_none());
    assert!(app.texture_dirty);
    assert!(app.onion_texture_pair.is_none());
}

#[test]
fn structural_frame_edit_during_playback_targets_the_visible_preview() {
    let mut app = two_frame_app(0);
    app.editor.animation.frames[0]
        .document
        .active_layer_mut()
        .canvas
        .set_pixel(0, 0, [255, 0, 0, 255]);
    app.editor.animation.frames[1]
        .document
        .active_layer_mut()
        .canvas
        .set_pixel(0, 0, [0, 0, 255, 255]);
    app.toggle_animation_playback(0.0);
    assert!(app.update_animation_playback(0.2));
    assert_eq!(app.editor.animation.current_frame_index, 1);

    app.duplicate_frame();

    assert!(!app.editor.animation.is_playing);
    assert_eq!(app.editor.animation.current_frame_index, 2);
    assert_eq!(app.editor.animation.selected_frame_index(), 2);
    assert_eq!(app.editor.animation.frames.len(), 3);
    assert_eq!(
        app.editor.animation.frames[0]
            .document
            .active_layer()
            .canvas
            .get_pixel(0, 0),
        [255, 0, 0, 255]
    );
    for frame_index in [1, 2] {
        assert_eq!(
            app.editor.animation.frames[frame_index]
                .document
                .active_layer()
                .canvas
                .get_pixel(0, 0),
            [0, 0, 255, 255]
        );
    }
}

#[test]
fn deleting_the_active_frame_clears_state_even_when_the_index_stays_zero() {
    let mut app = two_frame_app(0);
    seed_frame_local_state(&mut app);
    let ctx = egui::Context::default();
    let texture = |name: &str| {
        ctx.load_texture(
            name,
            egui::ColorImage::new([1, 1], egui::Color32::WHITE),
            egui::TextureOptions::NEAREST,
        )
    };
    app.frame_thumbnails = vec![
        Some(texture("delete_frame_zero")),
        Some(texture("delete_frame_one")),
    ];
    app.editor.animation.toggle_play(0.0);
    let generation = app.active_frame_generation();

    assert!(app.remove_current_frame());

    assert_eq!(app.editor.animation.frames.len(), 1);
    assert_eq!(app.editor.animation.current_frame_index, 0);
    assert!(!app.editor.animation.is_playing);
    assert!(!app.editor.history.can_undo());
    assert!(!app.editor.history.can_redo());
    assert!(!app.editor.selection.active);
    assert!(!app.is_drawing);
    assert!(app.last_canvas_pixel.is_none());
    assert_eq!(app.frame_thumbnails.len(), 1);
    assert!(app.frame_thumbnails.iter().all(Option::is_none));
    assert!(app.onion_texture_pair.is_none());
    assert_eq!(
        app.editor.document().active_layer().canvas.get_pixel(0, 0),
        [0, 0, 0, 0]
    );
    assert_eq!(app.active_frame_generation(), generation.wrapping_add(1));
}

#[test]
fn paste_prefers_selection_then_last_canvas_pixel() {
    let mut app = PixelBuddyApp::new(8, 8);
    app.last_canvas_pixel = Some((6, 5));
    assert_eq!(app.paste_origin(2, 2), (6, 5));

    app.editor.selection.set_rect(2, 1, 5, 4);
    assert_eq!(app.paste_origin(2, 2), (2, 1));
}

#[test]
fn dirty_project_replacement_waits_and_cancel_preserves_the_project() {
    let mut app = PixelBuddyApp::new(8, 8);
    app.apply_tool_changes(vec![(0, 0, [1, 2, 3, 255])]);
    app.editor.selection.set_rect(1, 1, 3, 3);
    let before = crate::io::project::encode_editor_bytes(&app.editor)
        .expect("the source project should encode");
    let before_revision = app.editor.revision();
    let before_history = app.editor.history.undo_descriptions();

    app.request_new_document(4, 4, super::PalettePolicy::UseDefault);

    assert_eq!(
        (app.editor.document().width, app.editor.document().height),
        (8, 8)
    );
    assert!(matches!(
        app.pending_replacement,
        Some(DocumentReplacement::NewDocument {
            width: 4,
            height: 4,
            palette_policy: super::PalettePolicy::UseDefault,
        })
    ));

    app.cancel_pending_document_replacement();

    assert!(app.pending_replacement.is_none());
    assert_eq!(app.editor.revision(), before_revision);
    assert_eq!(app.editor.history.undo_descriptions(), before_history);
    assert!(app.editor.selection.active);
    assert_eq!(
        crate::io::project::encode_editor_bytes(&app.editor)
            .expect("the cancelled project should still encode"),
        before
    );
}

#[test]
fn later_replacement_request_cannot_change_the_pending_target() {
    let mut app = PixelBuddyApp::new(8, 8);
    app.editor.mark_dirty();

    app.request_new_document(4, 4, super::PalettePolicy::UseDefault);
    app.request_new_document(6, 6, super::PalettePolicy::UseDefault);

    assert!(matches!(
        app.pending_replacement,
        Some(DocumentReplacement::NewDocument {
            width: 4,
            height: 4,
            palette_policy: super::PalettePolicy::UseDefault,
        })
    ));
    assert!(app
        .status_message
        .as_ref()
        .is_some_and(|(message, is_error)| *is_error
            && message.contains("Finish the current project-replacement confirmation")));

    app.confirm_pending_document_replacement();
    assert_eq!(
        (app.editor.document().width, app.editor.document().height),
        (4, 4)
    );
}

#[test]
fn confirmed_replacement_resets_runtime_state_dialogs_and_caches() {
    let mut app = PixelBuddyApp::new(8, 8);
    app.apply_tool_changes(vec![(0, 0, [9, 8, 7, 255])]);
    app.editor.selection.set_rect(0, 0, 2, 2);
    app.editor.clipboard = Some(ClipboardBuffer {
        pixels: vec![[1, 2, 3, 255]],
        width: 1,
        height: 1,
    });
    app.editor.copy_current_frame();
    app.begin_canvas_action(3, 4);
    app.preview_changes.push((3, 4, [1, 1, 1, 255]));
    app.pending_resize = Some((16, 16));
    app.open_png_export_dialog();
    app.show_spritesheet_import_dialog = true;
    app.spritesheet_import_source_session_id = Some(app.document_session_id());
    app.spritesheet_import_source_revision = Some(app.editor.revision());
    app.spritesheet_import_source_frame_generation = Some(app.active_frame_generation());
    app.spritesheet_import_source_active_layer_index =
        Some(app.editor.document().active_layer_index);
    app.spritesheet_import_data = Some((vec![1, 2, 3], "old.png".to_owned()));
    app.spritesheet_import_columns = "4".to_owned();
    app.spritesheet_import_rows = "2".to_owned();
    app.spritesheet_import_error = Some("old error".to_owned());
    app.horizontal_guides.push(2);
    app.vertical_guides.push(5);
    app.dragging_guide = Some((true, 0));
    app.pan_offset = egui::vec2(12.0, 8.0);
    app.canvas_rect = Some(egui::Rect::from_min_size(
        egui::pos2(1.0, 2.0),
        egui::vec2(10.0, 10.0),
    ));
    app.recovery_snapshot = Some("old recovery".to_owned());

    let ctx = egui::Context::default();
    let texture = |name: &str| {
        ctx.load_texture(
            name,
            egui::ColorImage::new([1, 1], egui::Color32::WHITE),
            egui::TextureOptions::NEAREST,
        )
    };
    app.canvas_texture = Some(texture("old_canvas"));
    app.onion_previous_texture = Some(texture("old_onion_previous"));
    app.onion_next_texture = Some(texture("old_onion_next"));
    app.onion_texture_pair = Some((0, 0));
    app.frame_thumbnails = vec![Some(texture("old_thumbnail"))];
    app.spritesheet_import_texture = Some(texture("old_import_preview"));

    let previous_session = app.document_session_id();
    app.request_new_document(4, 4, super::PalettePolicy::UseDefault);
    app.confirm_pending_document_replacement();

    assert_eq!(
        (app.editor.document().width, app.editor.document().height),
        (4, 4)
    );
    assert!(!app.editor.is_dirty());
    assert!(!app.editor.history.can_undo());
    assert!(!app.editor.history.can_redo());
    assert!(!app.editor.selection.active);
    assert!(app.editor.clipboard.is_none());
    assert!(!app.editor.has_copied_frame());
    assert!(!app.is_drawing);
    assert!(app.stroke_points.is_empty());
    assert!(app.preview_changes.is_empty());
    assert!(app.last_canvas_pixel.is_none());
    assert_eq!(app.pan_offset, egui::Vec2::ZERO);
    assert!(app.canvas_rect.is_none());
    assert!(app.auto_fit_requested);
    assert!(app.texture_dirty);
    assert!(app.canvas_texture.is_none());
    assert!(app.onion_previous_texture.is_none());
    assert!(app.onion_next_texture.is_none());
    assert!(app.onion_texture_pair.is_none());
    assert_eq!(app.frame_thumbnails.len(), 1);
    assert!(app.frame_thumbnails.iter().all(Option::is_none));
    assert!(app.pending_resize.is_none());
    assert!(app.export_resolution_dialog.is_none());
    assert!(!app.show_spritesheet_import_dialog);
    assert!(app.spritesheet_import_source_session_id.is_none());
    assert!(app.spritesheet_import_source_revision.is_none());
    assert!(app.spritesheet_import_source_frame_generation.is_none());
    assert!(app.spritesheet_import_source_active_layer_index.is_none());
    assert!(app.spritesheet_import_data.is_none());
    assert!(app.spritesheet_import_texture.is_none());
    assert!(app.spritesheet_import_error.is_none());
    assert!(app.horizontal_guides.is_empty());
    assert!(app.vertical_guides.is_empty());
    assert!(app.dragging_guide.is_none());
    assert!(app.recovery_snapshot.is_none());
    assert_eq!(app.document_session_id(), previous_session.wrapping_add(1));
}

#[test]
fn replacement_sources_have_explicit_saved_state_and_name_policies() {
    let mut new_project = PixelBuddyApp::new(8, 8);
    new_project.request_new_document(4, 4, super::PalettePolicy::UseDefault);
    assert!(!new_project.editor.is_dirty());
    assert!(new_project.editor.project_name.is_none());

    let mut raster_import = PixelBuddyApp::new(8, 8);
    raster_import.editor.primary_color = [10, 20, 30, 255];
    raster_import.editor.secondary_color = [40, 50, 60, 255];
    raster_import.editor.active_tool = crate::editor::ToolType::Ellipse;
    raster_import.editor.brush_size = 7;
    raster_import.request_imported_image(
        Document::new(3, 2),
        "sprite.png".to_owned(),
        super::PalettePolicy::UseDefault,
    );
    assert!(raster_import.editor.is_dirty());
    assert!(raster_import.editor.project_name.is_none());
    assert_eq!(raster_import.editor.primary_color, [0, 0, 0, 255]);
    assert_eq!(raster_import.editor.secondary_color, [255, 255, 255, 255]);
    assert_eq!(
        raster_import.editor.active_tool,
        crate::editor::ToolType::Pencil
    );
    assert_eq!(raster_import.editor.brush_size, 1);

    let mut animation_import = PixelBuddyApp::new(8, 8);
    animation_import.editor.primary_color = [10, 20, 30, 255];
    animation_import.editor.active_tool = crate::editor::ToolType::Move;
    animation_import.editor.brush_size = 5;
    let animation = AnimationManager::new(Document::new(6, 5));
    animation_import.request_imported_animation(
        animation,
        "walk.png".to_owned(),
        super::PalettePolicy::UseDefault,
    );
    assert!(animation_import.editor.is_dirty());
    assert!(animation_import.editor.project_name.is_none());
    assert!(animation_import.show_timeline);
    assert_eq!(animation_import.editor.primary_color, [0, 0, 0, 255]);
    assert_eq!(
        animation_import.editor.active_tool,
        crate::editor::ToolType::Pencil
    );
    assert_eq!(animation_import.editor.brush_size, 1);

    let mut opened_project = PixelBuddyApp::new(8, 8);
    let mut loaded_editor = EditorState::new(7, 3);
    loaded_editor.mutate_document("Rename layer", |document| {
        document.active_layer_mut().name = "Loaded Layer".to_owned();
        true
    });
    loaded_editor.selection.set_rect(0, 0, 1, 1);
    loaded_editor.clipboard = Some(ClipboardBuffer {
        pixels: vec![[4, 5, 6, 255]],
        width: 1,
        height: 1,
    });
    loaded_editor.copy_current_frame();
    loaded_editor.duplicate_frame();
    loaded_editor.animation.toggle_play(0.0);
    opened_project.request_opened_project(loaded_editor, "hero.pbud".to_owned());
    assert!(!opened_project.editor.is_dirty());
    assert_eq!(
        opened_project.editor.project_name.as_deref(),
        Some("hero.pbud")
    );
    assert!(!opened_project.editor.selection.active);
    assert!(opened_project.editor.clipboard.is_none());
    assert!(!opened_project.editor.has_copied_frame());
    assert!(!opened_project.editor.animation.is_playing);
    assert!(!opened_project.editor.history.can_undo());
    assert!(opened_project.show_timeline);
}

#[test]
fn recovery_uses_the_dirty_guard_and_only_consumes_snapshot_on_commit() {
    let mut app = PixelBuddyApp::new(8, 8);
    app.editor
        .document_mut()
        .active_layer_mut()
        .canvas
        .set_pixel(0, 0, [1, 2, 3, 255]);
    let before = crate::io::project::encode_editor_bytes(&app.editor)
        .expect("the active project should encode");
    app.recovery_snapshot = Some("retained until commit".to_owned());

    let mut recovered = EditorState::new(5, 4);
    recovered
        .document_mut()
        .active_layer_mut()
        .canvas
        .set_pixel(1, 1, [9, 8, 7, 255]);
    app.request_recovered_project(recovered);

    assert!(matches!(
        app.pending_replacement,
        Some(DocumentReplacement::RecoveredProject { .. })
    ));
    assert!(app.recovery_snapshot.is_some());
    assert_eq!(
        crate::io::project::encode_editor_bytes(&app.editor)
            .expect("the guarded project should encode"),
        before
    );

    app.confirm_pending_document_replacement();

    assert_eq!(
        (app.editor.document().width, app.editor.document().height),
        (5, 4)
    );
    assert_eq!(
        app.editor.document().active_layer().canvas.get_pixel(1, 1),
        [9, 8, 7, 255]
    );
    assert!(app.editor.is_dirty());
    assert!(app.editor.project_name.is_none());
    assert!(app.recovery_snapshot.is_none());
}

#[test]
fn current_project_import_rejects_a_stale_document_session() {
    let mut app = PixelBuddyApp::new(8, 8);
    let source_session = app.document_session_id();
    assert!(app.current_project_import_is_current(Some(source_session), None, None, None, "image",));

    app.request_new_document(4, 4, super::PalettePolicy::UseDefault);

    assert!(!app.current_project_import_is_current(
        Some(source_session),
        None,
        None,
        None,
        "image",
    ));
    assert_eq!(
        (app.editor.document().width, app.editor.document().height),
        (4, 4)
    );
    assert!(app
        .status_message
        .as_ref()
        .is_some_and(|(message, is_error)| *is_error
            && message.contains("active project, frame, or target layer changed")));
}

#[test]
fn current_frame_import_rejects_a_stale_frame_generation() {
    let mut app = two_frame_app(0);
    let source_session = app.document_session_id();
    let source_frame_generation = app.active_frame_generation();
    assert!(app.current_project_import_is_current(
        Some(source_session),
        None,
        Some(source_frame_generation),
        None,
        "image",
    ));

    assert!(app.select_next_frame());

    assert!(!app.current_project_import_is_current(
        Some(source_session),
        None,
        Some(source_frame_generation),
        None,
        "image",
    ));
    assert_eq!(app.document_session_id(), source_session);
    assert!(app
        .status_message
        .as_ref()
        .is_some_and(|(message, is_error)| *is_error
            && message.contains("active project, frame, or target layer changed")));
}

#[test]
fn opened_image_action_cannot_mutate_a_later_selected_frame() {
    let mut imported_document = Document::new(2, 2);
    imported_document
        .active_layer_mut()
        .canvas
        .set_pixel(1, 1, [9, 8, 7, 255]);
    let png = crate::io::png::export_document_to_png(&imported_document)
        .expect("the import fixture should encode");

    let mut app = two_frame_app(0);
    let source_session = app.document_session_id();
    let stale_generation = app.active_frame_generation();
    assert!(app.select_next_frame());
    app.editor.mark_saved();
    let before = crate::io::project::encode_editor_bytes(&app.editor)
        .expect("the target project should encode");
    let before_revision = app.editor.revision();

    app.handle_opened_image(
        png.clone(),
        "stale.png".to_owned(),
        false,
        source_session,
        stale_generation,
    );

    assert_eq!(app.editor.revision(), before_revision);
    assert_eq!(
        crate::io::project::encode_editor_bytes(&app.editor)
            .expect("a rejected import should leave the project encodable"),
        before
    );
    assert_eq!(app.editor.document().layers.len(), 1);

    let current_generation = app.active_frame_generation();
    app.handle_opened_image(
        png,
        "fresh.png".to_owned(),
        false,
        source_session,
        current_generation,
    );

    assert_eq!(app.editor.animation.current_frame_index, 1);
    assert_eq!(app.editor.document().layers.len(), 2);
    assert_eq!(
        app.editor.document().active_layer().canvas.get_pixel(1, 1),
        [9, 8, 7, 255]
    );
    assert!(app.editor.history.can_undo());
    assert!(app.undo_current_frame());
    assert_eq!(app.editor.document().layers.len(), 1);
    assert!(app.redo_current_frame());
    assert_eq!(app.editor.document().layers.len(), 2);
    assert!(app.select_previous_frame());
    assert_eq!(app.editor.document().layers.len(), 1);
}

#[test]
fn appending_sprite_frames_pauses_on_the_visible_preview_frame() {
    let mut app = two_frame_app(0);
    app.toggle_animation_playback(0.0);
    assert!(app.update_animation_playback(0.2));
    assert_eq!(app.editor.animation.current_frame_index, 1);
    assert!(app.editor.animation.is_playing);

    app.append_imported_animation_frames(AnimationManager::new(Document::new(2, 2)));

    assert!(!app.editor.animation.is_playing);
    assert_eq!(app.editor.animation.current_frame_index, 1);
    assert_eq!(app.editor.animation.selected_frame_index(), 1);
    assert_eq!(app.editor.animation.frames.len(), 3);
    assert!(app.editor.is_dirty());
    assert!(app.frame_thumbnails.iter().all(Option::is_none));
    assert!(app.onion_texture_pair.is_none());
}

#[test]
fn active_layer_sprite_import_pauses_and_adopts_the_visible_preview_frame() {
    let mut app = two_frame_app(0);
    app.toggle_animation_playback(0.0);
    assert!(app.update_animation_playback(0.2));
    let imported = two_frame_animation_with_pixel([12, 34, 56, 255]);

    assert_eq!(app.import_animation_into_active_layer(imported), Ok(2));

    assert!(!app.editor.animation.is_playing);
    assert_eq!(app.editor.animation.current_frame_index, 1);
    assert_eq!(app.editor.animation.selected_frame_index(), 1);
    for frame in &app.editor.animation.frames {
        assert_eq!(
            frame.document.active_layer().canvas.get_pixel(0, 0),
            [12, 34, 56, 255]
        );
    }
    assert!(app.editor.is_dirty());
    assert!(app.frame_thumbnails.iter().all(Option::is_none));
    assert!(app.onion_texture_pair.is_none());
}

#[test]
fn active_layer_sprite_import_preflights_every_target_atomically() {
    let mut locked = two_frame_app(0);
    locked.editor.animation.frames[1].document.layers[0].locked = true;
    let locked_before = crate::io::project::encode_editor_bytes(&locked.editor)
        .expect("the locked project should encode");
    let locked_revision = locked.editor.revision();
    let error = locked
        .import_animation_into_active_layer(two_frame_animation_with_pixel([90, 80, 70, 255]))
        .expect_err("a locked destination must reject the entire import");
    assert!(error.contains("locked"));
    assert_eq!(locked.editor.revision(), locked_revision);
    assert_eq!(
        crate::io::project::encode_editor_bytes(&locked.editor)
            .expect("a rejected import should leave the project encodable"),
        locked_before
    );

    let mut missing = two_frame_app(0);
    missing.editor.animation.frames[0].document.add_layer();
    let missing_before = crate::io::project::encode_editor_bytes(&missing.editor)
        .expect("the uneven-layer project should encode");
    let missing_revision = missing.editor.revision();
    let error = missing
        .import_animation_into_active_layer(two_frame_animation_with_pixel([90, 80, 70, 255]))
        .expect_err("a missing destination must reject the entire import");
    assert!(error.contains("no longer has target layer"));
    assert_eq!(missing.editor.revision(), missing_revision);
    assert_eq!(
        crate::io::project::encode_editor_bytes(&missing.editor)
            .expect("a rejected import should not partially modify frames"),
        missing_before
    );
}

#[test]
fn transparent_active_layer_import_preserves_clean_history_and_revision() {
    let mut app = two_frame_app(0);
    seed_frame_local_state(&mut app);
    let before = crate::io::project::encode_editor_bytes(&app.editor)
        .expect("the clean project should encode");
    let revision = app.editor.revision();
    let undo = app.editor.history.undo_descriptions();
    let redo = app.editor.history.redo_descriptions();
    let texture_dirty = app.texture_dirty;
    let onion_texture_pair = app.onion_texture_pair;

    assert_eq!(
        app.import_animation_into_active_layer(two_frame_animation_with_pixel([0, 0, 0, 0])),
        Ok(2)
    );

    assert_eq!(app.editor.revision(), revision);
    assert!(!app.editor.is_dirty());
    assert_eq!(app.editor.history.undo_descriptions(), undo);
    assert_eq!(app.editor.history.redo_descriptions(), redo);
    assert_eq!(
        crate::io::project::encode_editor_bytes(&app.editor)
            .expect("a no-op import should leave the project encodable"),
        before
    );
    assert_eq!(app.texture_dirty, texture_dirty);
    assert_eq!(app.onion_texture_pair, onion_texture_pair);
}

#[test]
fn append_import_ignores_revision_changes_but_active_layer_rejects_them() {
    let mut app = PixelBuddyApp::new(2, 2);
    app.spritesheet_import_source_session_id = Some(app.document_session_id());
    app.spritesheet_import_source_revision = Some(app.editor.revision());
    app.spritesheet_import_source_frame_generation = Some(app.active_frame_generation());
    app.spritesheet_import_source_active_layer_index =
        Some(app.editor.document().active_layer_index);
    app.editor.mark_dirty();

    app.spritesheet_import_mode = SpriteSheetImportMode::AppendFrames;
    assert!(app.current_spritesheet_import_is_current());

    app.spritesheet_import_mode = SpriteSheetImportMode::ActiveLayer;
    assert!(!app.current_spritesheet_import_is_current());
}

#[test]
fn active_layer_import_rejects_a_changed_target_layer() {
    let mut app = PixelBuddyApp::new(2, 2);
    app.editor.document_mut().add_layer();
    app.editor.document_mut().active_layer_index = 0;
    let source_session = app.document_session_id();
    let source_frame_generation = app.active_frame_generation();

    app.editor.document_mut().active_layer_index = 1;

    assert!(!app.current_project_import_is_current(
        Some(source_session),
        None,
        Some(source_frame_generation),
        Some(0),
        "sprite-sheet",
    ));
    assert_eq!(app.editor.document().active_layer_index, 1);
}

#[test]
fn active_layer_import_rejects_a_reused_layer_index_after_topology_changes() {
    let mut app = PixelBuddyApp::new(2, 2);
    app.editor.document_mut().add_layer();
    app.editor.document_mut().add_layer();
    app.editor.document_mut().active_layer_index = 1;
    let source_session = app.document_session_id();
    let source_revision = app.editor.revision();
    let source_frame_generation = app.active_frame_generation();

    app.editor.document_mut().remove_layer(1);
    assert_eq!(app.editor.document().active_layer_index, 1);

    assert!(!app.current_project_import_is_current(
        Some(source_session),
        Some(source_revision),
        Some(source_frame_generation),
        Some(1),
        "sprite-sheet",
    ));
    assert!(app.editor.revision() > source_revision);
    assert_eq!(app.active_frame_generation(), source_frame_generation);
}

#[test]
fn delayed_save_completion_cannot_mutate_a_replacement_project() {
    let mut app = PixelBuddyApp::new(8, 8);
    app.editor.mark_dirty();
    let old_source = ProjectSaveSource::new(app.document_session_id(), app.editor.revision(), 1);

    app.request_new_document(4, 4, super::PalettePolicy::UseDefault);
    app.confirm_pending_document_replacement();
    app.editor.mark_dirty();
    assert_eq!(app.editor.revision(), old_source.revision());

    app.handle_export_completed(
        ExportFormat::Project,
        "old-project.pbud".to_owned(),
        Some(old_source),
    );

    assert!(app.editor.is_dirty());
    assert!(app.editor.project_name.is_none());
    assert!(app
        .status_message
        .as_ref()
        .is_some_and(|(message, _)| message.contains("active project was not changed")));

    let mut current_app = PixelBuddyApp::new(2, 2);
    current_app.editor.mark_dirty();
    let current_source = ProjectSaveSource::new(
        current_app.document_session_id(),
        current_app.editor.revision(),
        1,
    );
    current_app.handle_export_completed(
        ExportFormat::Project,
        "current-project.pbud".to_owned(),
        Some(current_source),
    );
    assert!(!current_app.editor.is_dirty());
    assert_eq!(
        current_app.editor.project_name.as_deref(),
        Some("current-project.pbud")
    );
}

#[test]
fn older_same_project_save_cannot_override_a_newer_completion() {
    let mut app = PixelBuddyApp::new(2, 2);
    app.editor.mark_dirty();
    let first = ProjectSaveSource::new(app.document_session_id(), app.editor.revision(), 1);
    app.editor.mark_dirty();
    let second = ProjectSaveSource::new(app.document_session_id(), app.editor.revision(), 2);

    app.handle_export_completed(
        ExportFormat::Project,
        "second.pbud".to_owned(),
        Some(second),
    );
    assert!(!app.editor.is_dirty());
    assert_eq!(app.editor.project_name.as_deref(), Some("second.pbud"));

    app.handle_export_completed(ExportFormat::Project, "first.pbud".to_owned(), Some(first));

    assert!(!app.editor.is_dirty());
    assert_eq!(app.editor.project_name.as_deref(), Some("second.pbud"));
    assert!(app
        .status_message
        .as_ref()
        .is_some_and(|(message, _)| message.contains("newer save result remains active")));
}

#[test]
fn status_toast_expires_after_six_seconds() {
    assert!(!PixelBuddyApp::status_toast_expired(10.0, 15.999));
    assert!(PixelBuddyApp::status_toast_expired(10.0, 16.0));
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn fullscreen_toggle_inverts_known_state_and_enters_from_unknown_state() {
    assert!(super::next_fullscreen_state(None));
    assert!(super::next_fullscreen_state(Some(false)));
    assert!(!super::next_fullscreen_state(Some(true)));
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn window_presentation_prioritizes_fullscreen_and_limits_resize_handles() {
    assert_eq!(
        WindowPresentation::from_viewport(None, None),
        WindowPresentation::Windowed
    );
    assert_eq!(
        WindowPresentation::from_viewport(Some(true), Some(false)),
        WindowPresentation::Maximized
    );
    assert_eq!(
        WindowPresentation::from_viewport(Some(false), Some(true)),
        WindowPresentation::Fullscreen
    );
    assert_eq!(
        WindowPresentation::from_viewport(Some(true), Some(true)),
        WindowPresentation::Fullscreen
    );
    assert!(WindowPresentation::Windowed.allows_resize_handles());
    assert!(!WindowPresentation::Maximized.allows_resize_handles());
    assert!(!WindowPresentation::Fullscreen.allows_resize_handles());
}

#[cfg(not(target_arch = "wasm32"))]
fn maximize_or_restore_commands(
    maximized: Option<bool>,
    fullscreen: Option<bool>,
) -> Vec<egui::ViewportCommand> {
    let ctx = egui::Context::default();
    let mut input = egui::RawInput::default();
    let viewport = input
        .viewports
        .get_mut(&egui::ViewportId::ROOT)
        .expect("raw input contains the root viewport");
    viewport.maximized = maximized;
    viewport.fullscreen = fullscreen;
    ctx.begin_pass(input);

    let mut app = PixelBuddyApp::new(16, 16);
    app.auto_fit_requested = false;
    PixelBuddyApp::toggle_maximize_or_restore(&ctx, &mut app);
    assert!(app.auto_fit_requested);

    let output = ctx.end_pass();
    output
        .viewport_output
        .get(&egui::ViewportId::ROOT)
        .expect("the root viewport has output after a UI pass")
        .commands
        .clone()
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn window_control_maximizes_a_windowed_viewport() {
    assert_eq!(
        maximize_or_restore_commands(Some(false), Some(false)),
        vec![egui::ViewportCommand::Maximized(true)]
    );
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn window_control_restores_a_maximized_viewport() {
    assert_eq!(
        maximize_or_restore_commands(Some(true), Some(false)),
        vec![egui::ViewportCommand::Maximized(false)]
    );
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn window_control_exits_fullscreen_and_clears_underlying_maximization() {
    let expected = vec![
        egui::ViewportCommand::Fullscreen(false),
        egui::ViewportCommand::Maximized(false),
    ];
    assert_eq!(
        maximize_or_restore_commands(Some(false), Some(true)),
        expected
    );
    assert_eq!(
        maximize_or_restore_commands(Some(true), Some(true)),
        expected
    );
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn fullscreen_toggle_sends_a_root_viewport_command() {
    let ctx = egui::Context::default();
    ctx.begin_pass(egui::RawInput::default());

    let mut app = PixelBuddyApp::new(16, 16);
    PixelBuddyApp::toggle_fullscreen(&ctx, &mut app);

    let output = ctx.end_pass();
    let root_viewport = output
        .viewport_output
        .get(&egui::ViewportId::ROOT)
        .expect("the root viewport has output after a UI pass");
    assert!(root_viewport
        .commands
        .contains(&egui::ViewportCommand::Fullscreen(true)));
}

#[test]
fn status_toast_is_anchored_just_above_the_canvas_upper_right_corner() {
    let canvas = egui::Rect::from_min_max(egui::pos2(100.0, 120.0), egui::pos2(500.0, 420.0));
    let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1_920.0, 1_080.0));

    let (position, pivot) = PixelBuddyApp::status_toast_anchor(Some(canvas), screen);

    assert_eq!(position, egui::pos2(500.0, 110.0));
    assert_eq!(pivot, egui::Align2::RIGHT_BOTTOM);
}

#[test]
fn status_toast_uses_workspace_fallback_before_canvas_layout() {
    let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1_920.0, 1_080.0));

    let (position, pivot) = PixelBuddyApp::status_toast_anchor(None, screen);

    assert_eq!(position, egui::pos2(1_704.0, 48.0));
    assert_eq!(pivot, egui::Align2::RIGHT_TOP);
}

#[test]
fn raster_export_scales_report_the_actual_output_dimensions() {
    assert_eq!(
        RasterExportKind::Png.output_dimensions(16, 8, 1, 4),
        Some((64, 32))
    );
    assert_eq!(
        RasterExportKind::Gif.output_dimensions(16, 8, 3, 2),
        Some((32, 16))
    );
    assert_eq!(
        RasterExportKind::SpriteSheetPng.output_dimensions(16, 8, 3, 2),
        Some((96, 16))
    );
}

#[test]
fn raster_export_scale_requires_a_positive_whole_number() {
    assert_eq!(parse_raster_export_scale(" 8 "), Ok(8));
    assert!(parse_raster_export_scale("0").is_err());
    assert!(parse_raster_export_scale("1.5").is_err());
    assert!(parse_raster_export_scale("pixels").is_err());
}

#[test]
fn raster_export_dimensions_require_positive_whole_pixels() {
    assert_eq!(parse_raster_export_dimension(" 1024 ", "width"), Ok(1024));
    assert!(parse_raster_export_dimension("0", "height").is_err());
    assert!(parse_raster_export_dimension("10.5", "width").is_err());
}

#[test]
fn opening_a_raster_export_chooser_defaults_to_one_x() {
    let mut app = PixelBuddyApp::new(16, 16);

    app.open_sprite_sheet_export_dialog();

    let dialog = app
        .export_resolution_dialog
        .as_ref()
        .expect("sprite-sheet export should open the scale chooser");
    assert_eq!(dialog.kind, RasterExportKind::SpriteSheetPng);
    assert_eq!(dialog.sizing, RasterExportSizing::Scale);
    assert_eq!(dialog.scale_text, "1");
    assert_eq!(dialog.width_text, "16");
    assert_eq!(dialog.height_text, "16");
    assert!(dialog.error.is_none());
}

#[test]
fn resize_is_one_dirty_mutation_and_invalidates_every_frame_cache() {
    let mut app = PixelBuddyApp::new(2, 2);
    app.editor.duplicate_frame();
    app.apply_tool_changes(vec![(0, 0, [1, 2, 3, 255])]);
    app.editor.selection.set_rect(0, 0, 0, 0);
    app.editor.mark_saved();
    let revision = app.editor.revision();
    let ctx = egui::Context::default();
    let texture = |name: &str| {
        ctx.load_texture(
            name,
            egui::ColorImage::new([1, 1], egui::Color32::WHITE),
            egui::TextureOptions::NEAREST,
        )
    };
    app.frame_thumbnails = vec![Some(texture("resize_a")), Some(texture("resize_b"))];
    app.onion_texture_pair = Some((0, 1));
    app.texture_dirty = false;
    app.pan_offset = egui::vec2(8.0, -3.0);
    app.auto_fit_requested = false;

    assert!(app.resize_canvas(3, 4));

    assert!(app.editor.is_dirty());
    assert_eq!(app.editor.revision(), revision.wrapping_add(1));
    assert!(app
        .editor
        .animation
        .frames
        .iter()
        .all(|frame| (frame.document.width, frame.document.height) == (3, 4)));
    assert!(!app.editor.history.can_undo());
    assert!(!app.editor.history.can_redo());
    assert!(!app.editor.selection.active);
    assert!(app.frame_thumbnails.iter().all(Option::is_none));
    assert!(app.onion_texture_pair.is_none());
    assert!(app.texture_dirty);
    assert_eq!(app.pan_offset, egui::Vec2::ZERO);
    assert!(app.auto_fit_requested);

    app.editor.mark_saved();
    let clean_revision = app.editor.revision();
    assert!(!app.resize_canvas(3, 4));
    assert_eq!(app.editor.revision(), clean_revision);
    assert!(!app.editor.is_dirty());
}

#[test]
fn invalid_resize_is_atomic_and_keeps_the_project_clean() {
    let mut app = PixelBuddyApp::new(2, 2);
    app.editor
        .document_mut()
        .active_layer_mut()
        .canvas
        .set_pixel(1, 1, [1, 2, 3, 255]);
    app.editor.mark_saved();
    let revision = app.editor.revision();
    let before =
        crate::io::project::encode_editor_bytes(&app.editor).expect("valid project should encode");

    assert!(!app.resize_canvas(crate::document::canvas::MAX_DIMENSION + 1, 1));

    assert_eq!(app.editor.revision(), revision);
    assert!(!app.editor.is_dirty());
    assert_eq!(
        crate::io::project::encode_editor_bytes(&app.editor)
            .expect("unchanged project should encode"),
        before
    );
    assert_eq!(
        (app.editor.document().width, app.editor.document().height),
        (2, 2)
    );
    assert_eq!(
        app.editor.document().active_layer().canvas.get_pixel(1, 1),
        [1, 2, 3, 255]
    );
    assert!(app.status_message.as_ref().is_some_and(|(_, error)| *error));
}

#[test]
fn all_frame_layer_structure_commands_are_dirty_and_invalidate_caches() {
    let mut app = PixelBuddyApp::new(2, 2);
    app.editor.duplicate_frame();
    app.frame_thumbnails = vec![None, None];
    app.editor.mark_saved();
    let initial_bytes =
        crate::io::project::encode_editor_bytes(&app.editor).expect("the project should encode");

    assert!(app.add_layer_all_frames());
    assert!(app.editor.is_dirty());
    assert!(app
        .editor
        .animation
        .frames
        .iter()
        .all(|frame| frame.document.layers.len() == 2));
    assert_ne!(
        crate::io::project::encode_editor_bytes(&app.editor).expect("the project should encode"),
        initial_bytes
    );
    assert!(app.frame_thumbnails.iter().all(Option::is_none));
    assert!(app.texture_dirty);

    app.editor.mark_saved();
    app.texture_dirty = false;
    assert!(app.duplicate_active_layer_all_frames());
    assert!(app.editor.is_dirty());
    assert!(app
        .editor
        .animation
        .frames
        .iter()
        .all(|frame| frame.document.layers.len() == 3));
    assert!(app.texture_dirty);

    app.editor.mark_saved();
    app.texture_dirty = false;
    assert!(app.remove_active_layer_all_frames());
    assert!(app.editor.is_dirty());
    assert!(app
        .editor
        .animation
        .frames
        .iter()
        .all(|frame| frame.document.layers.len() == 2));
    assert!(app.texture_dirty);
}
fn key_input(key: egui::Key) -> egui::RawInput {
    let mut input = egui::RawInput::default();
    input.events.push(egui::Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    });
    input
}

#[test]
fn shortcut_permissions_separate_safe_global_and_document_commands() {
    assert_eq!(
        shortcut_permissions(false, false, false, false),
        ShortcutPermissions {
            global: true,
            document: true,
        }
    );
    assert_eq!(
        shortcut_permissions(true, false, false, false),
        ShortcutPermissions {
            global: true,
            document: false,
        }
    );
    for blocked in [
        shortcut_permissions(false, true, false, false),
        shortcut_permissions(false, false, true, false),
        shortcut_permissions(false, false, false, true),
    ] {
        assert_eq!(
            blocked,
            ShortcutPermissions {
                global: false,
                document: false,
            }
        );
    }
}

#[test]
fn foreground_dialog_blocks_delete_from_reaching_the_document() {
    let ctx = egui::Context::default();
    let mut app = PixelBuddyApp::new(2, 2);
    app.apply_tool_changes(vec![(0, 0, [9, 8, 7, 255])]);
    app.editor.selection.set_rect(0, 0, 0, 0);
    app.show_new_dialog = true;

    ctx.begin_pass(key_input(egui::Key::Delete));
    app.handle_shortcuts(&ctx, 0.0);
    let _ = ctx.end_pass();

    assert!(app.editor.selection.active);
    assert_eq!(
        app.editor.document().active_layer().canvas.get_pixel(0, 0),
        [9, 8, 7, 255]
    );
}

#[test]
fn focused_text_field_blocks_delete_and_tool_shortcuts() {
    let ctx = egui::Context::default();
    let mut text = String::from("Layer 1");
    ctx.begin_pass(egui::RawInput::default());
    egui::CentralPanel::default().show(&ctx, |ui| {
        ui.text_edit_singleline(&mut text).request_focus();
    });
    let _ = ctx.end_pass();

    let mut app = PixelBuddyApp::new(2, 2);
    app.apply_tool_changes(vec![(0, 0, [9, 8, 7, 255])]);
    app.editor.selection.set_rect(0, 0, 0, 0);
    let tool = app.editor.active_tool;

    ctx.begin_pass(key_input(egui::Key::Delete));
    egui::CentralPanel::default().show(&ctx, |ui| {
        ui.text_edit_singleline(&mut text).request_focus();
    });
    assert!(ctx.wants_keyboard_input());
    app.handle_shortcuts(&ctx, 0.0);
    let _ = ctx.end_pass();
    assert!(app.editor.selection.active);
    assert_eq!(
        app.editor.document().active_layer().canvas.get_pixel(0, 0),
        [9, 8, 7, 255]
    );

    ctx.begin_pass(key_input(egui::Key::H));
    egui::CentralPanel::default().show(&ctx, |ui| {
        ui.text_edit_singleline(&mut text).request_focus();
    });
    assert!(ctx.wants_keyboard_input());
    app.handle_shortcuts(&ctx, 0.0);
    let _ = ctx.end_pass();
    assert_eq!(app.editor.active_tool, tool);
}
fn merge_fixture() -> PixelBuddyApp {
    let mut app = PixelBuddyApp::new(1, 1);
    app.editor.document_mut().layers[0].name = "Base".to_owned();
    app.editor.document_mut().layers[0]
        .canvas
        .set_pixel(0, 0, [220, 30, 20, 160]);
    app.editor.document_mut().layers[0].opacity = 0.65;
    app.editor.document_mut().add_layer();
    app.editor.document_mut().layers[1].name = "Ink".to_owned();
    app.editor.document_mut().layers[1]
        .canvas
        .set_pixel(0, 0, [20, 60, 240, 144]);
    app.editor.document_mut().layers[1].opacity = 0.75;
    app.editor.document_mut().active_layer_index = 1;
    app
}

#[test]
fn merge_down_targets_active_minus_one_and_is_one_undoable_transaction() {
    let mut app = merge_fixture();
    let before_composite = app.editor.document().composite_preview().pixels().to_vec();
    let before_bottom = app.editor.document().layers[0].clone();
    let before_top = app.editor.document().layers[1].clone();
    app.editor.mark_saved();
    let revision = app.editor.revision();

    assert!(app.merge_down());

    assert_eq!(app.editor.document().layers.len(), 1);
    assert_eq!(app.editor.document().active_layer_index, 0);
    let merged = &app.editor.document().layers[0];
    assert_eq!(merged.name, "Base");
    assert_eq!(merged.opacity, 1.0);
    assert_eq!(merged.blend_mode, BlendMode::Normal);
    assert!(merged.visible);
    assert!(!merged.locked);
    assert_eq!(
        app.editor.document().composite_preview().pixels(),
        before_composite
    );
    assert_eq!(app.editor.revision(), revision.wrapping_add(1));
    assert_eq!(app.editor.history.undo_descriptions(), vec!["Merge Down"]);

    assert!(app.editor.undo());
    assert_eq!(app.editor.document().layers.len(), 2);
    assert_eq!(app.editor.document().active_layer_index, 1);
    for (restored, expected) in app
        .editor
        .document()
        .layers
        .iter()
        .zip([before_bottom, before_top])
    {
        assert_eq!(restored.name, expected.name);
        assert_eq!(restored.opacity, expected.opacity);
        assert_eq!(restored.blend_mode, expected.blend_mode);
        assert_eq!(restored.visible, expected.visible);
        assert_eq!(restored.locked, expected.locked);
        assert_eq!(restored.canvas.pixels(), expected.canvas.pixels());
    }
    assert!(app.editor.redo());
    assert_eq!(app.editor.document().layers.len(), 1);
    assert_eq!(
        app.editor.document().composite_preview().pixels(),
        before_composite
    );
}

#[test]
fn merge_down_in_three_layers_keeps_the_lower_neighbor_untouched() {
    let mut app = merge_fixture();
    app.editor.document_mut().add_layer();
    app.editor.document_mut().layers[2].name = "Highlights".to_owned();
    app.editor.document_mut().layers[2]
        .canvas
        .set_pixel(0, 0, [255, 255, 255, 64]);
    app.editor.document_mut().active_layer_index = 2;
    let lowest_pixels = app.editor.document().layers[0].canvas.pixels().to_vec();
    let before = app.editor.document().composite_preview().pixels().to_vec();

    assert!(app.merge_down());

    assert_eq!(app.editor.document().layers.len(), 2);
    assert_eq!(app.editor.document().active_layer_index, 1);
    assert_eq!(app.editor.document().layers[0].name, "Base");
    assert_eq!(
        app.editor.document().layers[0].canvas.pixels(),
        lowest_pixels
    );
    assert_eq!(app.editor.document().layers[1].name, "Ink");
    // Reassociating two source-over operations can differ by one 8-bit
    // channel step because each layer composite is rounded. That is the
    // tightest possible visual equivalence without baking lower layers
    // into the merged layer.
    for (actual, expected) in app
        .editor
        .document()
        .composite_preview()
        .pixels()
        .iter()
        .zip(before)
    {
        assert!(actual.abs_diff(expected) <= 1, "{actual} vs {expected}");
    }
}

#[test]
fn merge_down_rejects_bottom_hidden_locked_and_non_normal_layers_cleanly() {
    let assert_rejected = |mut app: PixelBuddyApp, expected_reason: &str| {
        app.editor.mark_saved();
        let revision = app.editor.revision();
        let bytes = crate::io::project::encode_editor_bytes(&app.editor)
            .expect("the project should encode");
        assert_eq!(app.merge_down_unavailable_reason(), Some(expected_reason));
        assert!(!app.merge_down());
        assert_eq!(app.editor.revision(), revision);
        assert!(!app.editor.is_dirty());
        assert!(!app.editor.history.can_undo());
        assert_eq!(
            crate::io::project::encode_editor_bytes(&app.editor)
                .expect("the project should encode"),
            bytes
        );
    };

    assert_rejected(
        PixelBuddyApp::new(1, 1),
        "The bottom layer has no layer below it",
    );

    let mut hidden = merge_fixture();
    hidden.editor.document_mut().layers[1].visible = false;
    hidden.editor.history.clear();
    assert_rejected(hidden, "Both layers must be visible before merging");

    let mut locked = merge_fixture();
    locked.editor.document_mut().layers[0].locked = true;
    locked.editor.history.clear();
    assert_rejected(locked, "Unlock both layers before merging");

    let mut blended = merge_fixture();
    blended.editor.document_mut().layers[1].blend_mode = BlendMode::Multiply;
    blended.editor.history.clear();
    assert_rejected(
        blended,
        "Merge Down currently supports Normal blend mode only",
    );
}
#[test]
fn model_resource_limits_reject_user_mutations_without_dirtying_state() {
    let mut app = PixelBuddyApp::new(1, 1);
    let frame = app.editor.animation.frames[0].clone();
    app.editor.animation.frames = vec![frame; crate::document::animation::MAX_ANIMATION_FRAMES];
    app.editor.mark_saved();
    let clean_revision = app.editor.revision();
    assert!(!app.add_frame());
    assert!(!app.duplicate_frame());
    assert_eq!(app.editor.revision(), clean_revision);
    assert!(!app.editor.is_dirty());

    while app.editor.document().layers.len() < crate::document::MAX_LAYERS_PER_FRAME {
        app.editor.document_mut().add_layer();
    }
    app.editor.mark_saved();
    let layer_revision = app.editor.revision();
    assert!(!app.add_layer_all_frames());
    assert!(!app.duplicate_active_layer_all_frames());
    assert_eq!(app.editor.revision(), layer_revision);
    assert!(!app.editor.is_dirty());

    while app.editor.document().palette.colors.len() < crate::document::MAX_PALETTE_COLORS {
        app.editor.document_mut().palette.add_color([1, 2, 3, 255]);
    }
    app.editor.mark_saved();
    assert!(!app.add_palette_color_current_frame([4, 5, 6, 255]));
    assert!(!app.rename_layer_current_frame(0, "bad\nname".to_owned()));
    assert!(!app.editor.is_dirty());

    app.editor.animation.tags = (0..crate::document::animation::MAX_ANIMATION_TAGS)
        .map(|index| crate::document::animation::FrameTag {
            name: format!("Tag {index}"),
            color: [0.5, 0.5, 0.5],
            from_frame: 0,
            to_frame: 0,
        })
        .collect();
    app.editor.mark_saved();
    assert!(
        !app.create_animation_tag(crate::document::animation::FrameTag {
            name: "One too many".to_owned(),
            color: [0.5, 0.5, 0.5],
            from_frame: 0,
            to_frame: 0,
        })
    );
    assert!(!app.editor.is_dirty());
}
#[test]
fn current_frame_layer_edits_have_explicit_scope_and_cache_effects() {
    let ctx = egui::Context::default();
    let texture = |name: &str| {
        ctx.load_texture(
            name,
            egui::ColorImage::new([1, 1], egui::Color32::WHITE),
            egui::TextureOptions::NEAREST,
        )
    };
    let mut app = PixelBuddyApp::new(2, 2);
    app.editor.duplicate_frame();
    assert!(app.select_frame(0));
    app.editor.mark_saved();
    app.frame_thumbnails = vec![Some(texture("scope_0")), Some(texture("scope_1"))];
    app.onion_texture_pair = Some((0, 1));
    app.texture_dirty = false;

    assert!(app.set_layer_visibility_current_frame(0, false));
    assert!(!app.editor.animation.frames[0].document.layers[0].visible);
    assert!(app.editor.animation.frames[1].document.layers[0].visible);
    assert!(app.editor.is_dirty());
    assert!(app.editor.history.can_undo());
    assert!(app.frame_thumbnails[0].is_none());
    assert!(app.frame_thumbnails[1].is_some());
    assert!(app.texture_dirty);
    assert!(app.onion_texture_pair.is_none());

    assert!(app.undo_current_frame());
    app.editor.mark_saved();
    app.frame_thumbnails = vec![Some(texture("rename_0")), Some(texture("rename_1"))];
    app.onion_texture_pair = Some((0, 1));
    app.texture_dirty = false;

    assert!(app.rename_layer_current_frame(0, "Current only".to_owned()));
    assert_eq!(
        app.editor.animation.frames[0].document.layers[0].name,
        "Current only"
    );
    assert_ne!(
        app.editor.animation.frames[1].document.layers[0].name,
        "Current only"
    );
    assert!(app.editor.is_dirty());
    assert!(!app.texture_dirty);
    assert!(app.frame_thumbnails.iter().all(Option::is_some));
    assert_eq!(app.onion_texture_pair, Some((0, 1)));
}
#[test]
fn main_texture_updates_do_not_allocate_full_resolution_frame_thumbnails() {
    let ctx = egui::Context::default();
    let mut app = PixelBuddyApp::new(64, 64);
    app.editor.duplicate_frame();
    app.update_texture(&ctx);

    assert!(app.canvas_texture.is_some());
    assert_eq!(app.frame_thumbnails.len(), 2);
    assert!(app.frame_thumbnails.iter().all(Option::is_none));
}
#[test]
fn edit_effects_invalidate_current_explicit_and_structural_frame_sets() {
    let ctx = egui::Context::default();
    let texture = |name: &str| {
        ctx.load_texture(
            name,
            egui::ColorImage::new([1, 1], egui::Color32::WHITE),
            egui::TextureOptions::NEAREST,
        )
    };
    let mut app = PixelBuddyApp::new(2, 2);
    app.editor.duplicate_frame();
    app.editor.duplicate_frame();
    app.frame_thumbnails = vec![
        Some(texture("effect_0")),
        Some(texture("effect_1")),
        Some(texture("effect_2")),
    ];
    app.onion_texture_pair = Some((0, 2));
    app.texture_dirty = false;

    assert!(app.consume_edit_effects(EditEffects::current_frame_artwork(true)));
    assert!(app.frame_thumbnails[0].is_some());
    assert!(app.frame_thumbnails[1].is_some());
    assert!(app.frame_thumbnails[2].is_none());
    assert!(app.texture_dirty);
    assert!(app.onion_texture_pair.is_none());

    app.frame_thumbnails = vec![
        Some(texture("set_0")),
        Some(texture("set_1")),
        Some(texture("set_2")),
    ];
    assert!(app.consume_edit_effects(EditEffects {
        changed: true,
        current_texture_dirty: false,
        frame_thumbnails: FrameThumbnailInvalidation::Frames(vec![0, 2]),
        onion_skin_dirty: true,
    }));
    assert!(app.frame_thumbnails[0].is_none());
    assert!(app.frame_thumbnails[1].is_some());
    assert!(app.frame_thumbnails[2].is_none());

    app.frame_thumbnails = vec![Some(texture("stale_structure"))];
    assert!(app.consume_edit_effects(EditEffects::all_frame_artwork(true, true)));
    assert_eq!(app.frame_thumbnails.len(), 3);
    assert!(app.frame_thumbnails.iter().all(Option::is_none));
}

#[test]
fn failed_project_encoding_does_not_allocate_or_apply_a_save_request() {
    let mut app = PixelBuddyApp::new(2, 2);
    app.editor.animation.frames[0].duration_ms = 0;
    app.editor.mark_dirty();
    let request_id = app.next_project_save_request_id;
    let revision = app.editor.revision();
    let project_name = app.editor.project_name.clone();

    assert!(!app.command_save_project_as());

    assert_eq!(app.next_project_save_request_id, request_id);
    assert_eq!(app.editor.revision(), revision);
    assert!(app.editor.is_dirty());
    assert_eq!(app.editor.project_name, project_name);
    assert!(app
        .status_message
        .as_ref()
        .is_some_and(|(_, is_error)| *is_error));
}
#[test]
fn tile_view_preferences_default_and_normalize_stored_counts() {
    let app = PixelBuddyApp::new(16, 16);
    assert_eq!(app.tile_mode, TileMode::None);
    assert_eq!(app.tile_preview.columns(), 3);
    assert_eq!(app.tile_preview.rows(), 3);
    assert!(!app.fit_tile_preview_requested);
    assert!(!app.tile_preview_fit_active);
    assert!(!app.show_timeline);

    let mut storage = TestStorage::default();
    storage.values.insert(
        VIEW_PREFERENCES_STORAGE_KEY.to_owned(),
        "(tile_mode:Both,tile_preview:(columns:5,rows:7),show_timeline:true,future_toggle:true)"
            .to_owned(),
    );
    let loaded = load_view_preferences(Some(&storage));
    assert_eq!(loaded.tile_mode, TileMode::Both);
    assert_eq!(loaded.tile_preview.columns(), 5);
    assert_eq!(loaded.tile_preview.rows(), 7);
    assert!(loaded.show_timeline);

    storage.values.insert(
        VIEW_PREFERENCES_STORAGE_KEY.to_owned(),
        "(tile_mode:Both,tile_preview:(columns:0,rows:255))".to_owned(),
    );
    let clamped = load_view_preferences(Some(&storage));
    assert_eq!(clamped.tile_preview.columns(), 1);
    assert_eq!(clamped.tile_preview.rows(), MAX_TILE_PREVIEW_COUNT);

    storage
        .values
        .insert(VIEW_PREFERENCES_STORAGE_KEY.to_owned(), "()".to_owned());
    assert_eq!(
        load_view_preferences(Some(&storage)),
        ViewPreferences::default()
    );

    storage.values.insert(
        VIEW_PREFERENCES_STORAGE_KEY.to_owned(),
        "not valid RON".to_owned(),
    );
    assert_eq!(
        load_view_preferences(Some(&storage)),
        ViewPreferences::default()
    );
}

#[test]
fn recovery_snapshots_have_a_dedicated_size_boundary_and_ignore_empty_values() {
    let maximum = crate::io::project::MAX_RECOVERY_SNAPSHOT_BYTES;
    assert!(recovery_snapshot_within_budget(maximum));
    assert!(!recovery_snapshot_within_budget(maximum + 1));

    let mut storage = TestStorage::default();
    assert!(load_recovery_snapshot(Some(&storage)).is_none());
    storage
        .values
        .insert(RECOVERY_STORAGE_KEY.to_owned(), String::new());
    assert!(load_recovery_snapshot(Some(&storage)).is_none());
    storage.values.insert(
        RECOVERY_STORAGE_KEY.to_owned(),
        include_str!("../../tests/fixtures/truncated_recovery.pbud").to_owned(),
    );
    let snapshot = load_recovery_snapshot(Some(&storage))
        .expect("a bounded snapshot should be offered for validated restore");
    assert!(crate::io::project::decode_editor(&snapshot).is_err());
}
#[test]
fn tile_view_changes_do_not_change_project_or_export_content() {
    let mut app = PixelBuddyApp::new(2, 3);
    let project_before =
        crate::io::project::encode_editor_bytes(&app.editor).expect("the project should encode");
    let revision_before = app.editor.revision();
    let dirty_before = app.editor.is_dirty();

    app.tile_mode = TileMode::Both;
    app.tile_preview.set_columns(MAX_TILE_PREVIEW_COUNT);
    app.tile_preview.set_rows(MAX_TILE_PREVIEW_COUNT);
    app.show_timeline = true;

    assert_eq!(
        crate::io::project::encode_editor_bytes(&app.editor)
            .expect("view changes must not affect project encoding"),
        project_before
    );
    assert_eq!(app.editor.revision(), revision_before);
    assert_eq!(app.editor.is_dirty(), dirty_before);

    let png = crate::io::png::export_document_to_png(app.editor.document())
        .expect("the source canvas should export");
    let image = image::load_from_memory_with_format(&png, image::ImageFormat::Png)
        .expect("the exported PNG should decode");
    assert_eq!((image.width(), image.height()), (2, 3));
}

#[test]
fn tile_preferences_save_separately_from_recovery_data() {
    let mut app = PixelBuddyApp::new(4, 4);
    app.editor.mark_dirty();
    let mut storage = TestStorage::default();

    eframe::App::save(&mut app, &mut storage);
    let recovery_before = storage
        .get_string(RECOVERY_STORAGE_KEY)
        .expect("dirty project recovery should be stored");
    let preferences_before = storage
        .get_string(VIEW_PREFERENCES_STORAGE_KEY)
        .expect("view preferences should be stored");

    app.tile_mode = TileMode::Both;
    app.tile_preview.set_columns(5);
    app.tile_preview.set_rows(7);
    app.show_timeline = true;
    eframe::App::save(&mut app, &mut storage);

    assert_eq!(
        storage.get_string(RECOVERY_STORAGE_KEY),
        Some(recovery_before)
    );
    assert_ne!(
        storage.get_string(VIEW_PREFERENCES_STORAGE_KEY),
        Some(preferences_before)
    );
    let loaded = load_view_preferences(Some(&storage));
    assert_eq!(loaded.tile_mode, TileMode::Both);
    assert!(loaded.show_timeline);
    assert_eq!(
        loaded.tile_preview.effective_dimensions(loaded.tile_mode),
        (5, 7)
    );
    assert_eq!(storage.flush_count, 2);
}

#[test]
fn document_replacement_preserves_tile_view_preferences() {
    let mut app = PixelBuddyApp::new(16, 16);
    app.tile_mode = TileMode::Both;
    app.tile_preview.set_columns(5);
    app.tile_preview.set_rows(7);
    app.show_timeline = true;

    app.request_new_document(4, 6, super::PalettePolicy::UseDefault);

    assert_eq!(
        (app.editor.document().width, app.editor.document().height),
        (4, 6)
    );
    assert_eq!(app.tile_mode, TileMode::Both);
    assert_eq!(app.tile_preview.effective_dimensions(app.tile_mode), (5, 7));
    assert!(app.show_timeline);
}

#[test]
fn starting_an_effect_immediately_previews_parameterless_effects() {
    let mut app = PixelBuddyApp::new(2, 1);
    app.editor
        .document_mut()
        .active_layer_mut()
        .canvas
        .set_pixel(0, 0, [10, 20, 30, 255]);
    app.editor.mark_saved();
    let revision_before = app.editor.revision();

    app.start_effect(crate::effects::EffectType::InvertColors);

    assert_eq!(
        app.editor.document().active_layer().canvas.get_pixel(0, 0),
        [10, 20, 30, 255]
    );
    assert_eq!(
        app.active_effect
            .as_ref()
            .expect("effect preview should remain active")
            .preview_document
            .as_ref()
            .expect("effect should own a preview document")
            .active_layer()
            .canvas
            .get_pixel(0, 0),
        [245, 235, 225, 255]
    );
    assert!(app.active_effect.is_some());
    assert!(app.texture_dirty);
    assert_eq!(app.editor.revision(), revision_before);
    assert!(!app.editor.is_dirty());
}

#[test]
fn non_preset_canvas_dimensions_are_valid_for_new_and_resize_workflows() {
    assert!(PixelBuddyApp::can_create_canvas(580, 320).is_ok());
    assert!(PixelBuddyApp::can_create_canvas(333, 197).is_ok());

    let mut app = PixelBuddyApp::new(16, 16);
    app.request_new_document(580, 320, super::PalettePolicy::UseDefault);
    assert_eq!(
        (app.editor.document().width, app.editor.document().height),
        (580, 320)
    );

    assert!(app.resize_canvas(333, 197));
    assert_eq!(
        (app.editor.document().width, app.editor.document().height),
        (333, 197)
    );
}

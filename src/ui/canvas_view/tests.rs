use super::{
    active_canvas_action_endpoint, aligned_ruler_start, canvas_gesture_accepts_hit, canvas_hit_at,
    canvas_origin_for_layout, finish_canvas_action, for_each_signed_stroke_point,
    mirrored_preview_origin, pixel_preview_rects, pixel_preview_screen_rect, prepare_canvas_input,
    retain_preview_pixels_in_selection, ruler_steps, screen_space_preview_rects,
    stroke_preview_cache_key, tile_preview_fit_zoom, tiled_stroke_changes, tiled_stroke_pixels,
    wheel_zoom, CanvasActionEndpoint, PixelMask, PixelPreviewRect, TileLayout,
};
use crate::app::{PixelBuddyApp, TileMode, TilePreviewSettings, MIN_CANVAS_ZOOM};
use crate::editor::ToolType;
#[test]
fn foreground_ui_blocks_canvas_input_and_cancels_an_active_gesture() {
    let mut app = PixelBuddyApp::new(2, 2);
    app.begin_canvas_action(1, 1);
    app.preview_changes.push((1, 1, [1, 2, 3, 255]));
    app.canvas_input_blocked = true;

    assert!(!prepare_canvas_input(&mut app));
    assert!(!app.is_drawing);
    assert!(app.preview_changes.is_empty());

    app.canvas_input_blocked = false;
    assert!(prepare_canvas_input(&mut app));
}

use egui::{Pos2, Rect, Vec2};

fn tile_settings(columns: u8, rows: u8) -> TilePreviewSettings {
    let mut settings = TilePreviewSettings::default();
    settings.set_columns(columns);
    settings.set_rows(rows);
    settings
}

#[test]
fn default_tile_layouts_match_the_legacy_preview_sizes() {
    let settings = TilePreviewSettings::default();
    assert_eq!(
        TileLayout::new(TileMode::None, settings)
            .offsets()
            .collect::<Vec<_>>(),
        vec![(0, 0)]
    );
    assert_eq!(
        TileLayout::new(TileMode::XAxis, settings)
            .offsets()
            .collect::<Vec<_>>(),
        vec![(-1, 0), (0, 0), (1, 0)]
    );
    assert_eq!(
        TileLayout::new(TileMode::YAxis, settings)
            .offsets()
            .collect::<Vec<_>>(),
        vec![(0, -1), (0, 0), (0, 1)]
    );
    assert_eq!(
        TileLayout::new(TileMode::Both, settings)
            .offsets()
            .collect::<Vec<_>>(),
        vec![
            (-1, -1),
            (0, -1),
            (1, -1),
            (-1, 0),
            (0, 0),
            (1, 0),
            (-1, 1),
            (0, 1),
            (1, 1),
        ]
    );
}

#[test]
fn even_tile_counts_keep_the_source_and_bias_the_extra_copy_positive() {
    let layout = TileLayout::new(TileMode::Both, tile_settings(4, 2));
    assert_eq!(
        layout.offsets().collect::<Vec<_>>(),
        vec![
            (-1, 0),
            (0, 0),
            (1, 0),
            (2, 0),
            (-1, 1),
            (0, 1),
            (1, 1),
            (2, 1),
        ]
    );

    let viewport = Rect::from_min_size(Pos2::ZERO, Vec2::new(200.0, 100.0));
    let tile_size = Vec2::new(20.0, 10.0);
    let source_centered = layout.anchor_origin(viewport, tile_size, Vec2::ZERO);
    assert_eq!(source_centered, Pos2::new(90.0, 45.0));

    let preview_pan = layout.preview_centering_pan(tile_size);
    assert_eq!(preview_pan, Vec2::new(-10.0, -5.0));
    assert_eq!(
        layout.anchor_origin(viewport, tile_size, preview_pan),
        Pos2::new(80.0, 40.0)
    );
}

#[test]
fn fitted_even_preview_recomputes_centering_when_layout_changes() {
    let viewport = Rect::from_min_size(Pos2::ZERO, Vec2::new(200.0, 100.0));
    let tile_size = Vec2::new(20.0, 10.0);
    let even = TileLayout::new(TileMode::Both, tile_settings(4, 2));
    let odd = TileLayout::new(TileMode::Both, tile_settings(3, 3));
    let off = TileLayout::new(TileMode::None, tile_settings(4, 2));

    assert_eq!(
        canvas_origin_for_layout(even, viewport, tile_size, Vec2::ZERO, true),
        Pos2::new(80.0, 40.0)
    );
    assert_eq!(
        canvas_origin_for_layout(odd, viewport, tile_size, Vec2::ZERO, true),
        Pos2::new(90.0, 45.0)
    );
    assert_eq!(
        canvas_origin_for_layout(off, viewport, tile_size, Vec2::ZERO, true),
        Pos2::new(90.0, 45.0)
    );
}

#[test]
fn axis_modes_ignore_the_inactive_tile_count() {
    let settings = tile_settings(5, 7);
    assert_eq!(settings.effective_dimensions(TileMode::XAxis), (5, 1));
    assert_eq!(settings.effective_dimensions(TileMode::YAxis), (1, 7));
}

#[test]
fn non_stroke_preview_origin_is_mirrored_to_each_configured_copy() {
    let source_origin = Pos2::new(10.0, 20.0);
    let tile_size = Vec2::new(12.0, 8.0);

    let physical_x = [-1, 0, 1].map(|target_x| {
        let origin =
            mirrored_preview_origin(source_origin, tile_size, (target_x, 0), (0, 0), false);
        origin.x + 4.0
    });
    assert_eq!(physical_x, [2.0, 14.0, 26.0]);
}

#[test]
fn canvas_hit_resolves_source_and_configured_copy_coordinates() {
    let origin = Pos2::new(10.0, 20.0);
    let source_layout = TileLayout::new(TileMode::None, TilePreviewSettings::default());
    assert_eq!(
        canvas_hit_at(Pos2::new(14.1, 24.1), origin, 4.0, 3, 2, source_layout).map(|hit| hit.pixel),
        Some((1, 1))
    );
    assert_eq!(
        canvas_hit_at(Pos2::new(22.0, 20.0), origin, 4.0, 3, 2, source_layout),
        None
    );
    assert_eq!(
        canvas_hit_at(Pos2::new(10.0, 20.0), origin, 0.0, 3, 2, source_layout),
        None
    );

    let tiled_layout = TileLayout::new(TileMode::Both, TilePreviewSettings::default());
    let right_copy = canvas_hit_at(Pos2::new(26.1, 24.1), origin, 4.0, 3, 2, tiled_layout)
        .expect("the configured right-hand copy is interactive");
    assert_eq!(right_copy.pixel, (1, 1));
    assert_eq!(right_copy.tile_offset, (1, 0));
    assert_eq!(right_copy.virtual_pixel, (4, 1));

    let left_copy = canvas_hit_at(Pos2::new(-1.9, 20.1), origin, 4.0, 3, 2, tiled_layout)
        .expect("the configured left-hand copy is interactive");
    assert_eq!(left_copy.pixel, (0, 0));
    assert_eq!(left_copy.tile_offset, (-1, 0));
    assert_eq!(left_copy.virtual_pixel, (-3, 0));
}

#[test]
fn blank_space_beyond_the_configured_preview_is_not_editable() {
    let origin = Pos2::new(10.0, 20.0);
    let layout = TileLayout::new(TileMode::Both, TilePreviewSettings::default());
    assert_eq!(
        canvas_hit_at(Pos2::new(34.1, 20.1), origin, 4.0, 3, 2, layout),
        None
    );

    let seam = canvas_hit_at(Pos2::new(22.0, 20.1), origin, 4.0, 3, 2, layout)
        .expect("an internal seam belongs to the positive tile");
    assert_eq!(seam.pixel, (0, 0));
    assert_eq!(seam.tile_offset, (1, 0));
    assert_eq!(seam.virtual_pixel, (3, 0));
}

#[test]
fn fit_tile_preview_accounts_for_all_configured_rows_and_columns() {
    let viewport = Rect::from_min_size(Pos2::ZERO, Vec2::new(900.0, 600.0));
    let source = TileLayout::new(TileMode::None, TilePreviewSettings::default());
    let tiled = TileLayout::new(TileMode::Both, TilePreviewSettings::default());

    let source_zoom = tile_preview_fit_zoom(viewport, 100, 50, source).expect("source fits");
    let tiled_zoom = tile_preview_fit_zoom(viewport, 100, 50, tiled).expect("tiles fit");
    assert!((source_zoom - 8.55).abs() < 0.001);
    assert!((tiled_zoom - 2.85).abs() < 0.001);
    assert!(tiled_zoom < source_zoom);
}

#[test]
fn source_auto_fit_can_zoom_below_one_for_large_documents() {
    let viewport = Rect::from_min_size(Pos2::ZERO, Vec2::new(900.0, 600.0));
    let source = TileLayout::new(TileMode::None, TilePreviewSettings::default());
    let zoom =
        tile_preview_fit_zoom(viewport, 8_192, 8_192, source).expect("large source canvas fits");

    assert!(zoom < 1.0);
    assert!(8_192.0 * zoom <= viewport.height() * 0.95 + f32::EPSILON);
}

#[test]
fn maximum_tile_preview_count_fits_a_common_canvas() {
    let viewport = Rect::from_min_size(Pos2::ZERO, Vec2::new(900.0, 600.0));
    let layout = TileLayout::new(TileMode::Both, tile_settings(15, 15));
    let zoom = tile_preview_fit_zoom(viewport, 128, 128, layout).expect("preview fits");

    assert!((zoom - 0.296_875).abs() < 0.000_001);
    assert!(128.0 * 15.0 * zoom <= viewport.height() * 0.95 + f32::EPSILON);
}

#[test]
fn streamed_signed_strokes_match_the_existing_pixel_perfect_algorithm() {
    let cases = [
        vec![],
        vec![(0, 0)],
        vec![(2, 2), (2, 2)],
        vec![(0, 0), (2, 0), (0, 0)],
        vec![(0, 0), (1, 0), (1, 1)],
        vec![(0, 0), (3, 2)],
        vec![(0, 0), (3, 2), (5, 2), (4, 4)],
        vec![(2, 2), (2, 2), (5, 5), (5, 2)],
    ];

    for points in cases {
        let unsigned = points
            .iter()
            .map(|&(x, y)| (x as u32, y as u32))
            .collect::<Vec<_>>();
        let expected = crate::tools::pencil::draw_stroke(&unsigned, [1, 2, 3, 4])
            .into_iter()
            .map(|(x, y, _)| (x as i32, y as i32))
            .collect::<Vec<_>>();
        let mut actual = Vec::new();
        for_each_signed_stroke_point(&points, |x, y| {
            actual.push((x, y));
            true
        });

        assert_eq!(actual, expected, "points: {points:?}");
    }

    let mut prefix = Vec::new();
    for_each_signed_stroke_point(&[(0, 0), (9, 0)], |x, y| {
        prefix.push((x, y));
        prefix.len() < 3
    });
    assert_eq!(prefix, vec![(0, 0), (1, 0), (2, 0)]);

    let base = [(4, 5), (7, 7), (9, 6)];
    let translated = base.map(|(x, y)| (x - 8, y - 9));
    let mut base_output = Vec::new();
    let mut translated_output = Vec::new();
    for_each_signed_stroke_point(&base, |x, y| {
        base_output.push((x, y));
        true
    });
    for_each_signed_stroke_point(&translated, |x, y| {
        translated_output.push((x + 8, y + 9));
        true
    });
    assert_eq!(translated_output, base_output);
}

#[test]
fn exact_wrapped_preview_coalesces_source_pixels_without_losing_edges() {
    let pixels = tiled_stroke_pixels(&[(2, 0), (3, 0)], 1, TileMode::XAxis, 3, 1);
    assert_eq!(pixels, vec![(0, 0), (2, 0)]);
    assert_eq!(
        pixel_preview_rects(&pixels),
        vec![
            PixelPreviewRect {
                min_x: 0,
                min_y: 0,
                max_x: 1,
                max_y: 1,
            },
            PixelPreviewRect {
                min_x: 2,
                min_y: 0,
                max_x: 3,
                max_y: 1,
            },
        ]
    );

    let brush_pixels = tiled_stroke_pixels(&[(2, 0), (3, 0)], 2, TileMode::XAxis, 3, 2);
    assert_eq!(
        pixel_preview_rects(&brush_pixels),
        vec![PixelPreviewRect {
            min_x: 0,
            min_y: 0,
            max_x: 3,
            max_y: 2,
        }]
    );
}

#[test]
fn preview_run_coalescing_preserves_ragged_rows_and_gaps() {
    let pixels = vec![
        (0, 0),
        (1, 0),
        (3, 0),
        (0, 1),
        (1, 1),
        (3, 1),
        (0, 3),
        (1, 3),
        (3, 3),
        (0, 4),
    ];
    assert_eq!(
        pixel_preview_rects(&pixels),
        vec![
            PixelPreviewRect {
                min_x: 0,
                min_y: 0,
                max_x: 2,
                max_y: 2,
            },
            PixelPreviewRect {
                min_x: 3,
                min_y: 0,
                max_x: 4,
                max_y: 2,
            },
            PixelPreviewRect {
                min_x: 0,
                min_y: 3,
                max_x: 2,
                max_y: 4,
            },
            PixelPreviewRect {
                min_x: 3,
                min_y: 3,
                max_x: 4,
                max_y: 4,
            },
            PixelPreviewRect {
                min_x: 0,
                min_y: 4,
                max_x: 1,
                max_y: 5,
            },
        ]
    );
}

#[test]
fn preview_selection_and_outer_copy_projection_use_source_bounds() {
    let mut pixels = vec![(0, 0), (1, 1), (2, 2), (3, 3)];
    retain_preview_pixels_in_selection(&mut pixels, Some((1, 2, 1, 2)));
    assert_eq!(pixels, vec![(1, 1), (2, 2)]);

    let edge_rects = pixel_preview_rects(&[(0, 0), (2, 0)]);
    let source_origin = Pos2::new(10.0, 20.0);
    let tile_size = Vec2::new(12.0, 8.0);
    for offset in [-1.0, 1.0] {
        let tile_origin = source_origin + Vec2::new(offset * tile_size.x, 0.0);
        let tile_rect = Rect::from_min_size(tile_origin, tile_size);
        for edge in &edge_rects {
            let projected = pixel_preview_screen_rect(*edge, tile_origin, 4.0);
            assert!(projected.left() >= tile_rect.left());
            assert!(projected.right() <= tile_rect.right());
            assert!(projected.top() >= tile_rect.top());
            assert!(projected.bottom() <= tile_rect.bottom());
        }
    }
}

#[test]
fn subpixel_preview_geometry_is_bounded_by_display_rows() {
    let diagonal = (0..8_192)
        .map(|coordinate| PixelPreviewRect {
            min_x: coordinate,
            min_y: coordinate,
            max_x: coordinate + 1,
            max_y: coordinate + 1,
        })
        .collect::<Vec<_>>();
    let projected = screen_space_preview_rects(&diagonal, 0.005);

    assert!(projected.len() <= 128);
    assert!(projected
        .iter()
        .all(|rect| rect.max_x <= 41 && rect.max_y <= 41));
}

#[test]
fn large_canvas_pixel_masks_start_sparse_and_promote_when_dense() {
    let mut mask = PixelMask::new(16_777_216);
    assert!(matches!(mask, PixelMask::Sparse { .. }));
    assert!(mask.insert(42));
    assert!(!mask.insert(42));
    for index in 0..70_000 {
        mask.insert(index);
    }
    assert!(matches!(mask, PixelMask::Dense { .. }));
}

#[test]
fn seam_crossing_stroke_wraps_to_adjacent_source_pixels() {
    let color = [9, 8, 7, 255];
    let changes = tiled_stroke_changes(&[(2, 0), (3, 0)], color, 1, TileMode::XAxis, 3, 1);
    let coordinates = changes
        .into_iter()
        .map(|(x, y, _)| (x, y))
        .collect::<Vec<_>>();

    assert_eq!(coordinates, vec![(0, 0), (2, 0)]);
}
#[test]
fn stroke_preview_cache_keys_change_for_new_gestures_and_points() {
    let mut app = PixelBuddyApp::new(8, 8);
    app.set_active_tool(ToolType::Pencil);
    app.begin_canvas_action_on_tile((1, 1), (0, 0), (1, 1));
    let initial = stroke_preview_cache_key(&app, 8, 8);
    assert_eq!(stroke_preview_cache_key(&app, 8, 8), initial);

    app.canvas_action_virtual_points.push((2, 1));
    assert_ne!(stroke_preview_cache_key(&app, 8, 8), initial);

    app.cancel_canvas_action();
    app.begin_canvas_action_on_tile((1, 1), (0, 0), (1, 1));
    assert_ne!(stroke_preview_cache_key(&app, 8, 8), initial);
}

#[test]
fn repeated_stroke_output_is_bounded_by_source_pixels() {
    let mut points = Vec::with_capacity(201);
    points.push((0, 0));
    for index in 0..200 {
        points.push((if index % 2 == 0 { 122_879 } else { 0 }, 0));
    }
    let changes = tiled_stroke_changes(&points, [1, 2, 3, 255], 8, TileMode::XAxis, 8_192, 1);

    assert_eq!(changes.len(), 8_192);
}

#[test]
fn non_wrapping_gestures_stay_on_their_starting_copy() {
    let layout = TileLayout::new(TileMode::XAxis, TilePreviewSettings::default());
    let origin = Pos2::new(10.0, 20.0);
    let source =
        canvas_hit_at(Pos2::new(10.1, 20.1), origin, 4.0, 3, 2, layout).expect("source tile hit");
    let right =
        canvas_hit_at(Pos2::new(22.1, 20.1), origin, 4.0, 3, 2, layout).expect("right tile hit");

    assert!(canvas_gesture_accepts_hit(
        ToolType::Pencil,
        Some(source.tile_offset),
        right
    ));
    assert!(!canvas_gesture_accepts_hit(
        ToolType::Line,
        Some(source.tile_offset),
        right
    ));
}

#[test]
fn release_outside_canvas_uses_the_active_gestures_last_valid_pixel() {
    let mut app = PixelBuddyApp::new(8, 8);
    app.set_active_tool(ToolType::Pencil);
    app.begin_canvas_action_on_tile((3, 4), (1, 0), (11, 4));
    app.canvas_action_last_pixel = Some((5, 4));
    app.canvas_action_virtual_points.push((13, 4));

    assert_eq!(
        active_canvas_action_endpoint(&app, None),
        Some(CanvasActionEndpoint {
            pixel: (5, 4),
            virtual_pixel: (13, 4),
        })
    );
}

#[test]
fn successful_marquee_release_preserves_the_completed_selection() {
    let mut app = PixelBuddyApp::new(8, 8);
    app.set_active_tool(ToolType::Marquee);
    app.begin_canvas_action_on_tile((1, 1), (1, 0), (9, 1));

    finish_canvas_action(
        &mut app,
        CanvasActionEndpoint {
            pixel: (3, 4),
            virtual_pixel: (11, 4),
        },
        false,
        8,
        8,
        TileMode::Both,
    );

    assert!(!app.is_drawing);
    assert!(app.editor.selection.active);
    assert_eq!(app.editor.selection.min_x(), 1);
    assert_eq!(app.editor.selection.min_y(), 1);
    assert_eq!(app.editor.selection.max_x(), 3);
    assert_eq!(app.editor.selection.max_y(), 4);
}

#[test]
fn low_zoom_ruler_steps_remain_bounded() {
    let (label_step, tick_step) = ruler_steps(MIN_CANVAS_ZOOM).expect("valid zoom");
    assert_eq!((label_step, tick_step), (50_000, 10_000));
    assert!(label_step as f32 * MIN_CANVAS_ZOOM >= 35.0);
    assert_eq!(aligned_ruler_start(123, tick_step), 10_000);
    assert_eq!(ruler_steps(f32::NAN), None);
    assert_eq!(ruler_steps(0.0), None);
}

#[test]
fn wheel_zoom_is_a_single_gentle_step_even_for_large_raw_deltas() {
    let single_line = wheel_zoom(10.0, 1.0).expect("a non-zero wheel delta zooms");
    let large_wheel_notch = wheel_zoom(10.0, 120.0).expect("a wheel notch zooms");

    assert!((single_line - large_wheel_notch).abs() < f32::EPSILON);
    assert!(single_line > 10.0);
    assert!(single_line < 11.0);

    let restored = wheel_zoom(single_line, -120.0).expect("reverse scroll zooms out");
    assert!((restored - 10.0).abs() < 0.000_1);
}

#[test]
fn wheel_zoom_clamps_and_ignores_zero_delta() {
    assert_eq!(wheel_zoom(64.0, 1.0), Some(64.0));
    assert_eq!(wheel_zoom(MIN_CANVAS_ZOOM, -1.0), Some(MIN_CANVAS_ZOOM));
    assert_eq!(wheel_zoom(10.0, 0.0), None);
}

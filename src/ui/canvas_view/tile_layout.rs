use crate::app::{TileMode, TilePreviewSettings, MAX_CANVAS_ZOOM, MIN_CANVAS_ZOOM};
use egui::{Pos2, Rect, Vec2};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct TileLayout {
    min_column: i32,
    max_column: i32,
    min_row: i32,
    max_row: i32,
}

impl TileLayout {
    pub(super) fn new(mode: TileMode, settings: TilePreviewSettings) -> Self {
        let (columns, rows) = settings.effective_dimensions(mode);
        let (min_column, max_column) = centered_axis_bounds(columns);
        let (min_row, max_row) = centered_axis_bounds(rows);
        Self {
            min_column,
            max_column,
            min_row,
            max_row,
        }
    }

    fn columns(self) -> u8 {
        (self.max_column - self.min_column + 1) as u8
    }

    fn rows(self) -> u8 {
        (self.max_row - self.min_row + 1) as u8
    }

    fn contains(self, column: i32, row: i32) -> bool {
        (self.min_column..=self.max_column).contains(&column)
            && (self.min_row..=self.max_row).contains(&row)
    }

    pub(super) fn offsets(self) -> impl Iterator<Item = (i32, i32)> {
        (self.min_row..=self.max_row).flat_map(move |row| {
            (self.min_column..=self.max_column).map(move |column| (column, row))
        })
    }

    pub(super) fn anchor_origin(self, viewport: Rect, tile_size: Vec2, pan_offset: Vec2) -> Pos2 {
        Pos2::new(
            viewport.center().x - tile_size.x / 2.0 + pan_offset.x,
            viewport.center().y - tile_size.y / 2.0 + pan_offset.y,
        )
    }

    /// Pan that centers the complete configured preview while keeping tile
    /// offset zero as the source canvas coordinate origin.
    pub(super) fn preview_centering_pan(self, tile_size: Vec2) -> Vec2 {
        let center_column = (self.min_column + self.max_column) as f32 / 2.0;
        let center_row = (self.min_row + self.max_row) as f32 / 2.0;
        Vec2::new(-center_column * tile_size.x, -center_row * tile_size.y)
    }
}

pub(super) fn canvas_origin_for_layout(
    layout: TileLayout,
    viewport: Rect,
    tile_size: Vec2,
    user_pan: Vec2,
    preview_fit_active: bool,
) -> Pos2 {
    let preview_fit_pan = if preview_fit_active {
        layout.preview_centering_pan(tile_size)
    } else {
        Vec2::ZERO
    };
    layout.anchor_origin(viewport, tile_size, user_pan + preview_fit_pan)
}

pub(super) fn mirrored_preview_origin(
    source_origin: Pos2,
    tile_size: Vec2,
    target_tile: (i32, i32),
    action_tile: (i32, i32),
    wraps: bool,
) -> Pos2 {
    let offset = if wraps {
        (target_tile.0 - action_tile.0, target_tile.1 - action_tile.1)
    } else {
        target_tile
    };
    source_origin + Vec2::new(offset.0 as f32 * tile_size.x, offset.1 as f32 * tile_size.y)
}

fn centered_axis_bounds(count: u8) -> (i32, i32) {
    let count = i32::from(count.max(1));
    let start = -((count - 1) / 2);
    (start, start + count - 1)
}

pub(super) fn tile_preview_fit_zoom(
    viewport: Rect,
    canvas_width: u32,
    canvas_height: u32,
    layout: TileLayout,
) -> Option<f32> {
    if canvas_width == 0
        || canvas_height == 0
        || !viewport.width().is_finite()
        || !viewport.height().is_finite()
        || viewport.width() <= 0.0
        || viewport.height() <= 0.0
    {
        return None;
    }

    let tiled_width = canvas_width as f32 * layout.columns() as f32;
    let tiled_height = canvas_height as f32 * layout.rows() as f32;
    let fit_x = (viewport.width() * 0.95) / tiled_width;
    let fit_y = (viewport.height() * 0.95) / tiled_height;
    let zoom = fit_x.min(fit_y);
    zoom.is_finite()
        .then(|| zoom.clamp(MIN_CANVAS_ZOOM, MAX_CANVAS_ZOOM))
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct CanvasHit {
    pub(super) pixel: (i32, i32),
    pub(super) tile_offset: (i32, i32),
    pub(super) virtual_pixel: (i32, i32),
    pub(super) tile_origin: Pos2,
}

pub(super) fn canvas_hit_at(
    position: Pos2,
    canvas_origin: Pos2,
    zoom: f32,
    canvas_width: u32,
    canvas_height: u32,
    layout: TileLayout,
) -> Option<CanvasHit> {
    if !zoom.is_finite() || zoom <= 0.0 || canvas_width == 0 || canvas_height == 0 {
        return None;
    }

    let display_width = canvas_width as f32 * zoom;
    let display_height = canvas_height as f32 * zoom;
    let relative_column = ((position.x - canvas_origin.x) / display_width).floor();
    let relative_row = ((position.y - canvas_origin.y) / display_height).floor();
    if !relative_column.is_finite() || !relative_row.is_finite() {
        return None;
    }

    let column = relative_column as i32;
    let row = relative_row as i32;
    if !layout.contains(column, row) {
        return None;
    }

    let tile_origin = Pos2::new(
        canvas_origin.x + column as f32 * display_width,
        canvas_origin.y + row as f32 * display_height,
    );
    let x = ((position.x - tile_origin.x) / zoom).floor();
    let y = ((position.y - tile_origin.y) / zoom).floor();
    if !x.is_finite()
        || !y.is_finite()
        || x < 0.0
        || y < 0.0
        || x >= canvas_width as f32
        || y >= canvas_height as f32
    {
        return None;
    }

    let width = i32::try_from(canvas_width).ok()?;
    let height = i32::try_from(canvas_height).ok()?;
    let local_x = x as i32;
    let local_y = y as i32;
    let virtual_x = column.checked_mul(width)?.checked_add(local_x)?;
    let virtual_y = row.checked_mul(height)?.checked_add(local_y)?;

    Some(CanvasHit {
        pixel: (local_x, local_y),
        tile_offset: (column, row),
        virtual_pixel: (virtual_x, virtual_y),
        tile_origin,
    })
}

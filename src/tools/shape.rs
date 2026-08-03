use super::PixelChange;
use super::line::draw_line;

/// Draws a rectangle between two points.
pub fn draw_rectangle(x0: i32, y0: i32, x1: i32, y1: i32, color: [u8; 4], filled: bool) -> Vec<PixelChange> {
    let mut changes = Vec::new();
    let min_x = x0.min(x1);
    let max_x = x0.max(x1);
    let min_y = y0.min(y1);
    let max_y = y0.max(y1);

    if filled {
        for y in min_y..=max_y {
            for x in min_x..=max_x {
                if x >= 0 && y >= 0 {
                    changes.push((x as u32, y as u32, color));
                }
            }
        }
    } else {
        changes.extend(draw_line(min_x, min_y, max_x, min_y, color));
        changes.extend(draw_line(min_x, max_y, max_x, max_y, color));
        changes.extend(draw_line(min_x, min_y, min_x, max_y, color));
        changes.extend(draw_line(max_x, min_y, max_x, max_y, color));
    }
    
    changes.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));
    changes.dedup_by(|a, b| a.0 == b.0 && a.1 == b.1);
    changes
}

/// Draws an ellipse using the midpoint ellipse algorithm.
pub fn draw_ellipse(cx: i32, cy: i32, rx: i32, ry: i32, color: [u8; 4], filled: bool) -> Vec<PixelChange> {
    let mut changes = Vec::new();
    if rx < 0 || ry < 0 {
        return changes;
    }
    if rx == 0 && ry == 0 {
        if cx >= 0 && cy >= 0 {
            changes.push((cx as u32, cy as u32, color));
        }
        return changes;
    }
    
    let rx2 = (rx as i64) * (rx as i64);
    let ry2 = (ry as i64) * (ry as i64);
    let tworx2 = 2 * rx2;
    let twory2 = 2 * ry2;
    
    let mut x = 0;
    let mut y = ry as i64;
    let mut px = 0;
    let mut py = tworx2 * y;
    
    let mut p1 = (ry2 as f64 - (rx2 as f64) * (ry as f64) + (0.25 * rx2 as f64)) as i64;
    
    let mut points = Vec::new();

    while px < py {
        points.push((x as i32, y as i32));
        x += 1;
        px += twory2;
        if p1 < 0 {
            p1 += ry2 + px;
        } else {
            y -= 1;
            py -= tworx2;
            p1 += ry2 + px - py;
        }
    }
    
    let mut p2 = (ry2 as f64 * (x as f64 + 0.5) * (x as f64 + 0.5) + rx2 as f64 * (y as f64 - 1.0) * (y as f64 - 1.0) - (rx2 * ry2) as f64) as i64;
    while y >= 0 {
        points.push((x as i32, y as i32));
        y -= 1;
        py -= tworx2;
        if p2 > 0 {
            p2 += rx2 - py;
        } else {
            x += 1;
            px += twory2;
            p2 += rx2 - py + px;
        }
    }
    
    if filled {
        let mut min_x_for_y: std::collections::HashMap<i32, i32> = std::collections::HashMap::new();
        let mut max_x_for_y: std::collections::HashMap<i32, i32> = std::collections::HashMap::new();
        
        for (dx, dy) in points {
            let ys = [cy + dy, cy - dy];
            let xs = [cx + dx, cx - dx];
            for qy in ys {
                for qx in xs {
                    min_x_for_y.entry(qy).and_modify(|e| *e = (*e).min(qx)).or_insert(qx);
                    max_x_for_y.entry(qy).and_modify(|e| *e = (*e).max(qx)).or_insert(qx);
                }
            }
        }
        
        for (qy, minx) in min_x_for_y {
            let maxx = max_x_for_y[&qy];
            if qy >= 0 {
                for qx in minx..=maxx {
                    if qx >= 0 {
                        changes.push((qx as u32, qy as u32, color));
                    }
                }
            }
        }
    } else {
        for (dx, dy) in points {
            let qs = [
                (cx + dx, cy + dy),
                (cx - dx, cy + dy),
                (cx + dx, cy - dy),
                (cx - dx, cy - dy),
            ];
            for (qx, qy) in qs {
                if qx >= 0 && qy >= 0 {
                    changes.push((qx as u32, qy as u32, color));
                }
            }
        }
    }
    
    changes.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));
    changes.dedup_by(|a, b| a.0 == b.0 && a.1 == b.1);
    
    changes
}

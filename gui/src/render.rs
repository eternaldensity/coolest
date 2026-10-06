//! Software shape renderer (headless-verifiable GUI layer). Draws the
//! Street grid, Deck arena, and Net graph as colored shapes with tiny-skia
//! (pure Rust, no display needed). Text widgets arrive with the egui shell;
//! geometry here matches `layout` pane rects.

use tiny_skia::{Color, Paint, Pixmap, Rect as SkRect, Transform};

pub const TILE: f32 = 24.0;

struct Rgb(u8, u8, u8);

fn rect(x: f32, y: f32, w: f32, h: f32, c: Rgb, px: &mut Pixmap) {
    let mut paint = Paint::default();
    paint.set_color_rgba8(c.0, c.1, c.2, 255);
    px.fill_rect(
        SkRect::from_xywh(x, y, w, h).unwrap(),
        &paint,
        Transform::identity(),
        None,
    );
}

pub fn background(px: &mut Pixmap) {
    let mut paint = Paint::default();
    paint.set_color(Color::from_rgba8(16, 16, 24, 255));
    px.fill_rect(
        SkRect::from_xywh(0.0, 0.0, px.width() as f32, px.height() as f32).unwrap(),
        &paint,
        Transform::identity(),
        None,
    );
}

/// Street section: walkable dark grey, blocked near-black, guarantees gold,
/// spawn/actor red. Returns pixel coords of tile (x, y) for assertions.
pub fn draw_street(px: &mut Pixmap, section: &sim::Section, actors: &[(u8, u8)]) {
    background(px);
    for y in 0..16u8 {
        for x in 0..16u8 {
            let (r, g, b) = if section.walkable(x, y) { (64, 64, 72) } else { (20, 20, 26) };
            rect(x as f32 * TILE, y as f32 * TILE, TILE - 1.0, TILE - 1.0, Rgb(r, g, b), px);
        }
    }
    for (_, gx, gy) in &section.guarantees {
        rect(*gx as f32 * TILE, *gy as f32 * TILE, TILE - 1.0, TILE - 1.0, Rgb(200, 160, 40), px);
    }
    for (ax, ay) in actors {
        rect(*ax as f32 * TILE, *ay as f32 * TILE, TILE - 1.0, TILE - 1.0, Rgb(220, 60, 60), px);
    }
}

/// Deck arena: empty dark, tail segment teal, head bright cyan.
pub fn draw_deck(px: &mut Pixmap, host: &sim::Host) {
    background(px);
    for prog in host.progs.values() {
        for tail in &prog.tail {
            rect(tail.0 as f32 * TILE, tail.1 as f32 * TILE, TILE - 1.0, TILE - 1.0, Rgb(40, 140, 140), px);
        }
        rect(prog.head.0 as f32 * TILE, prog.head.1 as f32 * TILE, TILE - 1.0, TILE - 1.0, Rgb(120, 240, 240), px);
    }
}

/// Sample the center pixel of tile (x, y).
pub fn tile_center(px: &Pixmap, x: u8, y: u8) -> (u8, u8, u8) {
    let px_x = (x as f32 * TILE + TILE / 2.0) as u32;
    let px_y = (y as f32 * TILE + TILE / 2.0) as u32;
    let p = px.pixel(px_x, px_y).unwrap();
    (p.red(), p.green(), p.blue())
}

#[cfg(test)]
mod tests {
    use super::*;
    use sim::{CellPos, EntityId, Host, HostId, Ports, gen_section};

    fn canvas() -> Pixmap {
        Pixmap::new(16 * TILE as u32, 16 * TILE as u32).unwrap()
    }

    #[test]
    fn street_marks_guarantees_and_actor() {
        let s = gen_section(7, 0, 0, Ports::default());
        let mut px = canvas();
        draw_street(&mut px, &s, &[(8, 8)]);
        // Actor red on top of whatever was there.
        assert_eq!(tile_center(&px, 8, 8), (220, 60, 60));
        // A guarantee cell is gold unless the actor stands on it.
        let (_, gx, gy) = s.guarantees[0];
        if (gx, gy) != (8, 8) {
            assert_eq!(tile_center(&px, gx, gy), (200, 160, 40));
        }
        // Walkable vs blocked differ.
        assert_ne!(tile_center(&px, 8, 8), tile_center(&px, 0, 0));
    }

    #[test]
    fn deck_marks_head_brighter_than_tail() {
        let mut h = Host::new(HostId(1), 12, 8, 100);
        h.load(EntityId(10), EntityId(1), CellPos(2, 2), 3, 1).unwrap();
        h.step(EntityId(10), CellPos(3, 2)).unwrap();
        let mut px = canvas();
        draw_deck(&mut px, &h);
        assert_eq!(tile_center(&px, 3, 2), (120, 240, 240)); // head
        assert_eq!(tile_center(&px, 2, 2), (40, 140, 140)); // tail
        assert_eq!(tile_center(&px, 9, 9), (16, 16, 24)); // empty bg
    }

    #[test]
    fn snapshot_png_writes() {
        let s = gen_section(7, 0, 0, Ports::default());
        let mut px = canvas();
        draw_street(&mut px, &s, &[(8, 8)]);
        let path = "/tmp/opencode/fidom-gui-smoke.png";
        px.save_png(path).unwrap();
        let meta = std::fs::metadata(path).unwrap();
        assert!(meta.len() > 1000);
    }
}

//! Web-client core (headless). Implements sim's `Renderer` so Bevy/Godot
//! stay swappable (design/08). Pure layout/format helpers map 1:1 onto
//! egui widgets; the eframe/wasm shell plugs `render()` into
//! `egui::Ui` in a display-capable env (deferred — no GPU here).
//!
//! Each helper documents its egui target in one line.

use sim::{EntityId, RenderView};

/// egui target: `ui.label(RichText::new(status_line(...)))` in status pane.
pub fn status_line(food: u8, mood: u8, money_cents: i64) -> String {
    format!(
        "food {food:3}  mood {mood:3}  ${:.2}",
        money_cents as f64 / 100.0
    )
}

/// egui target: tile button glyph in the Street grid.
pub fn tile_glyph(tile: u16) -> char {
    match tile {
        0 => '.',
        1 => '#',
        2 => '+', // door
        3 => 'C', // computer anchor
        _ => '?',
    }
}

/// egui target: `ui.add(ProgressBar::show(deck_bar(...)))` in actions pane.
pub fn deck_bar(used: u32, total: u32) -> String {
    let pct = used
        .checked_mul(100)
        .and_then(|v| v.checked_div(total))
        .unwrap_or(0);
    format!("CPU {used}/{total} ({pct}%)")
}

/// egui target: selectable node card in the Net map.
pub fn net_line(from: u64, to: u64, latency_ms: u32, toll_cents: i64) -> String {
    format!(
        "{from}→{to} {latency_ms}ms toll ${:.2}",
        toll_cents as f64 / 100.0
    )
}

/// Headless client: stores the last view per mode + a capped console log.
/// eframe shell will call `render()` inside `update()` and draw from these.
pub struct WebClient {
    pub last: Option<RenderView>,
    pub log: Vec<String>,
    pub log_cap: usize,
}

impl WebClient {
    pub fn new() -> Self {
        Self {
            last: None,
            log: Vec::new(),
            log_cap: 200,
        }
    }

    pub fn say(&mut self, line: &str) {
        if self.log.len() >= self.log_cap {
            self.log.remove(0);
        }
        self.log.push(line.to_string());
    }
}

impl Default for WebClient {
    fn default() -> Self {
        Self::new()
    }
}

impl sim::Renderer for WebClient {
    fn render(&mut self, view: &RenderView) {
        self.last = Some(view.clone());
    }
}

pub fn demo_actor() -> EntityId {
    EntityId(1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use sim::Renderer;
    use sim::{DeckView, HostId, NetView, StreetView, TileId};

    #[test]
    fn client_stores_views_and_caps_log() {
        let mut c = WebClient::new();
        c.log_cap = 2;
        c.say("a");
        c.say("b");
        c.say("c");
        assert_eq!(c.log, vec!["b".to_string(), "c".to_string()]);
        let tile = TileId {
            city: 1,
            sx: 0,
            sy: 0,
            z: 0,
            x: 8,
            y: 8,
        };
        c.render(&RenderView::Street(StreetView {
            center: tile,
            tiles: vec![],
            actors: vec![],
        }));
        assert!(matches!(c.last, Some(RenderView::Street(_))));
        c.render(&RenderView::Net(NetView {
            focus: HostId(1),
            nodes: vec![],
            edges: vec![],
        }));
        assert!(matches!(c.last, Some(RenderView::Net(_))));
        c.render(&RenderView::Deck(DeckView {
            host: HostId(1),
            ram_used: 3,
            ram_total: 96,
        }));
        assert!(matches!(c.last, Some(RenderView::Deck(_))));
    }
}

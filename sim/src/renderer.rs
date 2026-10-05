use serde::{Deserialize, Serialize};
use crate::ids::{EntityId, HostId, TileId};

/// Serializable view snapshots. Sim produces these; clients render them.
/// Keeps Bevy / Godot / Web interchangeable (design/08).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RenderView {
    Street(StreetView),
    Net(NetView),
    Deck(DeckView),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreetView {
    pub center: TileId,
    pub tiles: Vec<(TileId, u16)>,
    pub actors: Vec<(EntityId, TileId)>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetView {
    pub focus: HostId,
    pub nodes: Vec<HostId>,
    pub edges: Vec<(HostId, HostId)>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeckView {
    pub host: HostId,
    pub ram_used: u32,
    pub ram_total: u32,
}

/// Client-side renderer. Implement for egui now, Bevy/Godot later.
pub trait Renderer {
    fn render(&mut self, view: &RenderView);
}

/// Headless null renderer for tests and server.
pub struct NullRenderer;
impl Renderer for NullRenderer {
    fn render(&mut self, _view: &RenderView) {}
}

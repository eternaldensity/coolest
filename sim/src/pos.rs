use crate::ids::TileId;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Pos {
    pub city: u32,
    pub sx: i32,
    pub sy: i32,
    pub z: i8,
    pub x: u8,
    pub y: u8,
}

impl Pos {
    pub fn tile_id(&self) -> TileId {
        TileId {
            city: self.city,
            sx: self.sx,
            sy: self.sy,
            z: self.z,
            x: self.x,
            y: self.y,
        }
    }

    pub fn debug_string(&self) -> String {
        format!(
            "city{}:S({},{}) :F{}:T({},{})",
            self.city, self.sx, self.sy, self.z, self.x, self.y
        )
    }
}

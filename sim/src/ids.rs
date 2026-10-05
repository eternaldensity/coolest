use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct EntityId(pub u64);

impl EntityId {
    pub fn new(shard: u8, kind: u8, counter: u64) -> Self {
        Self(((shard as u64) << 56) | ((kind as u64) << 48) | (counter & 0xFFFFFFFFFFFF))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct HostId(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TileId {
    pub city: u32,
    pub sx: i32,
    pub sy: i32,
    pub z: i8,
    pub x: u8,
    pub y: u8,
}

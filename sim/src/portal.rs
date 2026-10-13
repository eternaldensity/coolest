use crate::ids::EntityId;
use serde::{Deserialize, Serialize};

/// Multi-city portals (design/08 + roadmap): cities are shards with no
/// shared sim traffic. Portals move travelers (and mail) between them.
/// Identity travels as shard|kind|counter — the counter survives the jump,
/// so cross-server copies stay unambiguous (never pointer ids).

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Portal {
    pub from_city: u32,
    pub from_sx: i32,
    pub from_sy: i32,
    pub from_x: u8,
    pub from_y: u8,
    pub to_city: u32,
    pub to_sx: i32,
    pub to_sy: i32,
    pub to_x: u8,
    pub to_y: u8,
    pub toll_cents: i64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PortalNet {
    pub portals: Vec<Portal>,
}

impl PortalNet {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add(&mut self, portal: Portal) {
        self.portals.push(portal);
    }

    pub fn find(&self, city: u32, sx: i32, sy: i32, x: u8, y: u8) -> Option<&Portal> {
        self.portals.iter().find(|p| {
            p.from_city == city
                && p.from_sx == sx
                && p.from_sy == sy
                && p.from_x == x
                && p.from_y == y
        })
    }

    /// Rekey an entity into another shard, preserving kind + counter.
    pub fn rekey(id: EntityId, new_shard: u8) -> EntityId {
        EntityId(((new_shard as u64) << 56) | (id.0 & 0x00FFFFFFFFFFFFFF))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pos::Pos;

    fn portal() -> Portal {
        Portal {
            from_city: 1,
            from_sx: 0,
            from_sy: 0,
            from_x: 14,
            from_y: 8,
            to_city: 2,
            to_sx: 0,
            to_sy: 0,
            to_x: 1,
            to_y: 8,
            toll_cents: 50,
        }
    }

    #[test]
    fn find_and_travel_endpoints() {
        let mut net = PortalNet::new();
        net.add(portal());
        let p = net.find(1, 0, 0, 14, 8).unwrap();
        assert_eq!((p.to_city, p.to_sx, p.to_x), (2, 0, 1));
        assert!(net.find(1, 0, 0, 1, 1).is_none());
        assert!(net.find(2, 0, 0, 14, 8).is_none()); // one-way until linked back
                                                     // Destination tile struct for the receiving world to spawn into.
        let dest = Pos {
            city: p.to_city,
            sx: p.to_sx,
            sy: p.to_sy,
            z: 0,
            x: p.to_x,
            y: p.to_y,
        };
        assert_eq!((dest.city, dest.x), (2, 1));
    }

    #[test]
    fn rekey_preserves_kind_and_counter() {
        let id = EntityId::new(1, 7, 12345);
        let moved = PortalNet::rekey(id, 2);
        assert_eq!(moved.0 >> 56, 2);
        assert_eq!(moved.0 & 0x00FFFFFFFFFFFFFF, id.0 & 0x00FFFFFFFFFFFFFF);
    }
}

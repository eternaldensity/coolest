use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use crate::error::SimError;
use crate::ids::EntityId;

/// M5 Veil (design/01/03/08): helmet entry uploads a MindToken. Death
/// ejects with mindpower drain (not permadeath outside hardcore).
/// Inventory is mind-files: capacity by mindpower, single-use reads,
/// disk-clone erases source (no duping).
pub const ENTER_COST: u8 = 5;
pub const DEATH_DRAIN: u8 = 4;
pub const KICK_DRAIN: u8 = 2;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MindFile {
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct MindToken {
    hp: u8,
    mindpower: u8,
    files: Vec<MindFile>,
    controls: Option<EntityId>, // mental-transfer host
    area: u64,
}

/// One hosted domain, labeled by its host (design/08: 5-10 areas per host).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Area {
    pub name: String,
    pub exits: Vec<u64>,
    pub is_exit: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Veil {
    tokens: BTreeMap<u64, MindToken>,
    areas: BTreeMap<u64, Area>,
}

impl Veil {
    pub fn new() -> Self {
        Self::default()
    }

    /// Helmet entry. Needs mindpower >= 5; token starts at 10 mind-HP.
    pub fn enter(&mut self, owner: EntityId, mindpower: u8) -> Result<(), SimError> {
        if mindpower < ENTER_COST {
            return Err(SimError::new("weak_mind", "mindpower >= 5", "too low"));
        }
        if self.tokens.contains_key(&owner.0) {
            return Err(SimError::new("already_in", "single token", "duplicate"));
        }
        self.tokens.insert(owner.0, MindToken { hp: 10, mindpower, files: Vec::new(), controls: None, area: 0 });
        Ok(())
    }

    /// Register a hosted area. Area 0 always exists as the default lobby.
    pub fn add_area(&mut self, id: u64, name: &str, exits: Vec<u64>, is_exit: bool) {
        self.areas.insert(id, Area { name: name.to_string(), exits, is_exit });
    }

    /// Helmet entry directly into a named area.
    pub fn enter_at(&mut self, owner: EntityId, mindpower: u8, area: u64) -> Result<(), SimError> {
        self.enter(owner, mindpower)?;
        if area != 0 && !self.areas.contains_key(&area) {
            self.tokens.remove(&owner.0);
            return Err(SimError::new("no_area", "hosted area", "missing"));
        }
        self.tokens.get_mut(&owner.0).unwrap().area = area;
        Ok(())
    }

    pub fn area_of(&self, owner: EntityId) -> Option<u64> {
        self.tokens.get(&owner.0).map(|t| t.area)
    }

    /// Undirected neighbors: explicit exit links, plus the 0-lobby hub
    /// which touches every area (jack in/out point, never itself an exit).
    fn neighbors(&self, area: u64) -> Vec<u64> {
        let mut out = Vec::new();
        if area == 0 {
            out.extend(self.areas.keys().copied());
            return out;
        }
        if let Some(a) = self.areas.get(&area) {
            out.extend(a.exits.iter().copied());
        }
        for (id, a) in &self.areas {
            if a.exits.contains(&area) && !out.contains(id) {
                out.push(*id);
            }
        }
        out.push(0);
        out
    }

    /// Move along an exit link (undirected) or via the 0-lobby hub.
    pub fn move_to(&mut self, owner: EntityId, area: u64) -> Result<(), SimError> {
        if area != 0 && !self.areas.contains_key(&area) {
            return Err(SimError::new("no_area", "hosted area", "missing"));
        }
        let cur = self.area_of(owner)
            .ok_or(SimError::new("not_inside", "active token", "absent"))?;
        if cur != area && !self.neighbors(cur).contains(&area) {
            return Err(SimError::new("no_exit", "linked area", "unlinked"));
        }
        self.tokens.get_mut(&owner.0).unwrap().area = area;
        Ok(())
    }

    /// Nearest flagged exit by BFS. None = no graceful exit (hard eject —
    /// death still removes the token regardless).
    pub fn nearest_exit(&self, owner: EntityId) -> Option<u64> {
        let start = self.area_of(owner)?;
        use std::collections::{BTreeSet, VecDeque};
        let mut seen = BTreeSet::new();
        let mut q = VecDeque::from([start]);
        seen.insert(start);
        while let Some(a) = q.pop_front() {
            if a != 0 && self.areas.get(&a).map(|x| x.is_exit).unwrap_or(false) {
                return Some(a);
            }
            for n in self.neighbors(a) {
                if seen.insert(n) {
                    q.push_back(n);
                }
            }
        }
        None
    }

    pub fn present(&self, owner: EntityId) -> bool {
        self.tokens.contains_key(&owner.0)
    }

    pub fn mindpower(&self, owner: EntityId) -> Option<u8> {
        self.tokens.get(&owner.0).map(|t| t.mindpower)
    }

    /// Damage the mind. At 0 HP the token dies: eject + heavy drain.
    /// Returns the drain applied (0 if survived).
    pub fn damage(&mut self, owner: EntityId, n: u8) -> Result<u8, SimError> {
        let t = self.tokens.get_mut(&owner.0)
            .ok_or(SimError::new("not_inside", "active token", "absent"))?;
        t.hp = t.hp.saturating_sub(n);
        if t.hp == 0 {
            self.tokens.remove(&owner.0);
            Ok(DEATH_DRAIN)
        } else {
            Ok(0)
        }
    }

    /// Clean exit (or kickout at <5 mindpower): light drain.
    pub fn eject(&mut self, owner: EntityId) -> Result<u8, SimError> {
        self.tokens.remove(&owner.0)
            .map(|_| KICK_DRAIN)
            .ok_or(SimError::new("not_inside", "active token", "absent"))
    }

    /// Mental transfer: take over a nearby lower-mindpower host.
    /// Host takes the damage instead while controlled.
    pub fn transfer(&mut self, owner: EntityId, host: EntityId, host_mp: u8, in_range: bool) -> Result<(), SimError> {
        if !in_range {
            return Err(SimError::new("out_of_range", "nearby host", "too far"));
        }
        let t = self.tokens.get_mut(&owner.0)
            .ok_or(SimError::new("not_inside", "active token", "absent"))?;
        if host_mp >= t.mindpower {
            return Err(SimError::new("strong_host", "lower-mp host", "too strong"));
        }
        t.controls = Some(host);
        Ok(())
    }

    pub fn controlled(&self, owner: EntityId) -> Option<EntityId> {
        self.tokens.get(&owner.0).and_then(|t| t.controls)
    }

    /// Carry a mind-file. Capacity = current mindpower file count.
    /// Cloning onto disk erases the source (caller enforces: take then drop).
    pub fn carry(&mut self, owner: EntityId, name: &str) -> Result<(), SimError> {
        let t = self.tokens.get_mut(&owner.0)
            .ok_or(SimError::new("not_inside", "active token", "absent"))?;
        if t.files.len() >= t.mindpower as usize {
            return Err(SimError::new("mind_full", "capacity", "full"));
        }
        t.files.push(MindFile { name: name.to_string() });
        Ok(())
    }

    /// Read = delete-after-view (single-use). Returns the file name.
    pub fn read(&mut self, owner: EntityId, idx: usize) -> Result<String, SimError> {
        let t = self.tokens.get_mut(&owner.0)
            .ok_or(SimError::new("not_inside", "active token", "absent"))?;
        if idx >= t.files.len() {
            return Err(SimError::new("no_file", "carried file", "missing"));
        }
        Ok(t.files.remove(idx).name)
    }

    /// Power loss: blackout ejects everyone externals-first with kick drain.
    /// Returns owners ejected.
    pub fn blackout(&mut self) -> Vec<EntityId> {
        let owners: Vec<u64> = self.tokens.keys().copied().collect();
        self.tokens.clear();
        owners.into_iter().map(EntityId).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entry_needs_mindpower_5() {
        let mut v = Veil::new();
        assert!(v.enter(EntityId(1), 4).is_err());
        v.enter(EntityId(1), 5).unwrap();
        assert!(v.present(EntityId(1)));
        assert!(v.enter(EntityId(1), 9).is_err()); // duplicate
    }

    #[test]
    fn death_ejects_with_heavy_drain() {
        let mut v = Veil::new();
        v.enter(EntityId(1), 8).unwrap();
        assert_eq!(v.damage(EntityId(1), 3).unwrap(), 0);
        assert!(v.present(EntityId(1)));
        assert_eq!(v.damage(EntityId(1), 99).unwrap(), DEATH_DRAIN);
        assert!(!v.present(EntityId(1)));
    }

    #[test]
    fn transfer_needs_weaker_nearby_host() {
        let mut v = Veil::new();
        v.enter(EntityId(1), 8).unwrap();
        assert!(v.transfer(EntityId(1), EntityId(7), 9, true).is_err()); // stronger
        assert!(v.transfer(EntityId(1), EntityId(7), 3, false).is_err()); // far
        v.transfer(EntityId(1), EntityId(7), 3, true).unwrap();
        assert_eq!(v.controlled(EntityId(1)), Some(EntityId(7)));
    }

    #[test]
    fn files_capped_single_use() {
        let mut v = Veil::new();
        v.enter(EntityId(1), 2 + ENTER_COST).unwrap(); // capacity 7
        for i in 0..7 {
            v.carry(EntityId(1), &format!("f{i}")).unwrap();
        }
        assert!(v.carry(EntityId(1), "extra").is_err());
        assert_eq!(v.read(EntityId(1), 0).unwrap(), "f0");
        assert!(v.read(EntityId(1), 99).is_err());
        v.carry(EntityId(1), "extra").unwrap(); // room after consume
    }

    #[test]
    fn blackout_ejects_all() {
        let mut v = Veil::new();
        v.enter(EntityId(1), 6).unwrap();
        v.enter(EntityId(2), 6).unwrap();
        let out = v.blackout();
        assert_eq!(out.len(), 2);
        assert!(!v.present(EntityId(1)));
    }

    #[test]
    fn areas_move_along_links_and_find_exits() {
        let mut v = Veil::new();
        // 1 - 2 - 3(exit); 4 isolated plot (hub-reachable only).
        v.add_area(1, "host-a:lobby", vec![2], false);
        v.add_area(2, "host-a:ops", vec![1, 3], false);
        v.add_area(3, "host-a:roof", vec![2], true);
        v.add_area(4, "host-b:vault", vec![], false);
        v.enter_at(EntityId(1), 8, 1).unwrap();
        assert_eq!(v.area_of(EntityId(1)), Some(1));
        assert!(v.move_to(EntityId(1), 3).is_err()); // no direct link
        v.move_to(EntityId(1), 2).unwrap();
        assert_eq!(v.nearest_exit(EntityId(1)), Some(3));
        v.move_to(EntityId(1), 3).unwrap();
        assert!(v.move_to(EntityId(1), 9).is_err()); // unknown area
        assert!(v.enter_at(EntityId(2), 8, 9).is_err()); // unknown entry
        // Unlinked 4 still reaches an exit via the 0-lobby hub (two hops).
        v.enter_at(EntityId(3), 8, 4).unwrap();
        assert_eq!(v.nearest_exit(EntityId(3)), Some(3));
    }
}

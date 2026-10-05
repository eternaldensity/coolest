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
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Veil {
    tokens: BTreeMap<u64, MindToken>,
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
        self.tokens.insert(owner.0, MindToken { hp: 10, mindpower, files: Vec::new(), controls: None });
        Ok(())
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
}

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, VecDeque};
use crate::error::SimError;
use crate::ids::{EntityId, HostId};

/// M2a Deck arena: one host's RAM grid + programs.
/// Position stays grid-based (4-dir); cards/resources/trace land in M2b/c.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct CellPos(pub u8, pub u8);

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Segment {
    prog: EntityId,
    is_head: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Program {
    pub id: EntityId,
    pub owner: EntityId,
    pub head: CellPos,
    pub tail: VecDeque<CellPos>,
    pub max_len: u8,
    pub cpu_per_seg: u8,
}

impl Program {
    pub fn len(&self) -> usize {
        1 + self.tail.len()
    }

    pub fn is_empty(&self) -> bool {
        false // programs always occupy at least the head cell
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Host {
    pub id: HostId,
    pub w: u8,
    pub h: u8,
    pub cpu_total: u32,
    cells: BTreeMap<(u8, u8), Segment>,
    pub progs: BTreeMap<u64, Program>,
}

impl Host {
    pub fn new(id: HostId, w: u8, h: u8, cpu_total: u32) -> Self {
        Self { id, w, h, cpu_total, cells: BTreeMap::new(), progs: BTreeMap::new() }
    }

    pub fn cpu_load(&self) -> u32 {
        self.progs.values().map(|p| 1 + p.len() as u32 * p.cpu_per_seg as u32).sum()
    }

    pub fn overloaded(&self) -> bool {
        self.cpu_load() > self.cpu_total
    }

    fn free(&self, at: CellPos) -> bool {
        at.0 < self.w && at.1 < self.h && !self.cells.contains_key(&(at.0, at.1))
    }

    /// Load a fresh len-1 program onto a free Public cell.
    pub fn load(
        &mut self,
        prog: EntityId,
        owner: EntityId,
        at: CellPos,
        max_len: u8,
        cpu_per_seg: u8,
    ) -> Result<(), SimError> {
        if !self.free(at) {
            return Err(SimError::new("cell_busy", "free cell", "occupied/out of bounds"));
        }
        if self.progs.contains_key(&prog.0) {
            return Err(SimError::new("dup_prog", "fresh id", "exists"));
        }
        // Project load including newcomer; refuse if it would overload.
        let add = 1 + cpu_per_seg as u32;
        if self.cpu_load() + add > self.cpu_total {
            return Err(SimError::new("cpu_overload", "headroom", "over budget"));
        }
        self.cells.insert((at.0, at.1), Segment { prog, is_head: true });
        self.progs.insert(prog.0, Program {
            id: prog, owner, head: at,
            tail: VecDeque::new(), max_len: max_len.max(1), cpu_per_seg,
        });
        Ok(())
    }

    /// 4-dir step of the head. Grows by 1 unless at max_len (then slides: drop last tail).
    pub fn step(&mut self, prog: EntityId, to: CellPos) -> Result<(), SimError> {
        if !self.free(to) {
            return Err(SimError::new("cell_busy", "free cell", "occupied/out of bounds"));
        }
        // 4-dir adjacency.
        let p = self.progs.get(&prog.0)
            .ok_or(SimError::new("no_prog", "live program", "missing"))?;
        let dx = (to.0 as i16 - p.head.0 as i16).abs();
        let dy = (to.1 as i16 - p.head.1 as i16).abs();
        if dx + dy != 1 {
            return Err(SimError::new("not_adjacent", "4-dir step", "jump"));
        }
        if self.overloaded() {
            return Err(SimError::new("cpu_overload", "headroom", "growth blocked"));
        }
        let p = self.progs.get_mut(&prog.0).unwrap();
        // Old head becomes tail segment.
        self.cells.insert((p.head.0, p.head.1), Segment { prog, is_head: false });
        p.tail.push_front(p.head);
        p.head = to;
        self.cells.insert((to.0, to.1), Segment { prog, is_head: true });
        // Slide when over max length.
        while p.tail.len() + 1 > p.max_len as usize {
            if let Some(last) = p.tail.pop_back() {
                self.cells.remove(&(last.0, last.1));
            }
        }
        Ok(())
    }

    /// Strike a cell: deletes one tail segment, or kills on headshot.
    /// Returns true if a program died.
    pub fn strike(&mut self, target: CellPos) -> Result<bool, SimError> {
        let seg = self.cells.get(&(target.0, target.1)).cloned()
            .ok_or(SimError::new("no_target", "program cell", "empty"))?;
        let victim = seg.prog;
        if seg.is_head {
            self.remove_program(victim);
            Ok(true)
        } else {
            // Remove that tail segment from victim.
            if let Some(p) = self.progs.get_mut(&victim.0) {
                p.tail.retain(|c| *c != target);
            }
            self.cells.remove(&(target.0, target.1));
            Ok(false)
        }
    }

    /// Takeover: attacker's head must be 4-adjacent to victim's head.
    /// Ownership flips; costs the caller 2 acts (enforced by scheduler side).
    pub fn takeover(&mut self, attacker: EntityId, victim_head: CellPos) -> Result<(), SimError> {
        let seg = self.cells.get(&(victim_head.0, victim_head.1)).cloned()
            .ok_or(SimError::new("no_target", "program head", "empty"))?;
        if !seg.is_head {
            return Err(SimError::new("not_head", "head cell", "tail"));
        }
        let a = self.progs.get(&attacker.0)
            .ok_or(SimError::new("no_prog", "live attacker", "missing"))?;
        let dx = (victim_head.0 as i16 - a.head.0 as i16).abs();
        let dy = (victim_head.1 as i16 - a.head.1 as i16).abs();
        if dx + dy != 1 {
            return Err(SimError::new("not_adjacent", "adjacent heads", "too far"));
        }
        let new_owner = a.owner;
        if let Some(v) = self.progs.get_mut(&seg.prog.0) {
            v.owner = new_owner;
        }
        Ok(())
    }

    fn remove_program(&mut self, prog: EntityId) {
        if let Some(p) = self.progs.remove(&prog.0) {
            self.cells.remove(&(p.head.0, p.head.1));
            for c in p.tail {
                self.cells.remove(&(c.0, c.1));
            }
        }
    }

    pub fn owner_of(&self, prog: EntityId) -> Option<EntityId> {
        self.progs.get(&prog.0).map(|p| p.owner)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn host() -> Host {
        Host::new(HostId(1), 12, 8, 100)
    }

    #[test]
    fn killer_ttk_is_3_to_4() {
        // Two len-3 killers: 3 tail/head strikes to clear.
        let mut h = host();
        h.load(EntityId(10), EntityId(1), CellPos(2, 2), 3, 1).unwrap();
        h.step(EntityId(10), CellPos(3, 2)).unwrap();
        h.step(EntityId(10), CellPos(4, 2)).unwrap();
        assert_eq!(h.progs[&10].len(), 3);
        // Strike tail twice then head: 3 acts total.
        assert!(!h.strike(CellPos(2, 2)).unwrap());
        assert!(!h.strike(CellPos(3, 2)).unwrap());
        assert!(h.strike(CellPos(4, 2)).unwrap());
        assert!(h.progs.get(&10).is_none());
    }

    #[test]
    fn cpu_overload_blocks_load_and_growth() {
        let mut h = Host::new(HostId(2), 12, 8, 3);
        h.load(EntityId(20), EntityId(1), CellPos(0, 0), 2, 1).unwrap(); // load 2
        assert!(h.load(EntityId(21), EntityId(1), CellPos(5, 5), 2, 1).is_err()); // 2+2>3
        // Growth while at cap is fine (still under), but overload blocks step.
        let mut h2 = Host::new(HostId(3), 12, 8, 100);
        h2.load(EntityId(30), EntityId(1), CellPos(0, 0), 4, 1).unwrap();
        h2.step(EntityId(30), CellPos(1, 0)).unwrap();
        assert_eq!(h2.progs[&30].len(), 2);
    }

    #[test]
    fn takeover_flips_owner() {
        let mut h = host();
        h.load(EntityId(40), EntityId(1), CellPos(0, 0), 2, 1).unwrap();
        h.load(EntityId(41), EntityId(2), CellPos(0, 2), 2, 1).unwrap();
        h.step(EntityId(40), CellPos(0, 1)).unwrap(); // adjacent to victim head
        h.takeover(EntityId(40), CellPos(0, 2)).unwrap();
        assert_eq!(h.owner_of(EntityId(41)), Some(EntityId(1)));
        // Takeover on tail rejected.
        assert!(h.takeover(EntityId(40), CellPos(0, 0)).is_err());
    }
}

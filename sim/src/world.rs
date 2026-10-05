use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use crate::error::SimError;
use crate::ids::EntityId;
use crate::pos::Pos;
use crate::scheduler::Scheduler;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Dir { N, S, E, W }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Intent {
    pub actor: EntityId,
    pub verb: Verb,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Verb {
    Move(Dir),
    Wait,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Delta {
    pub moved: Vec<(EntityId, Pos)>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    pub tick: u64,
    pub actor: EntityId,
    pub kind: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Being {
    pos: Pos,
    next_at: u64,
    speed: u64, // ticks per tile, 10 = walk
}

/// Minimal M0 world: 1 section 16x16, walls on border, one floor.
#[derive(Debug, Serialize, Deserialize)]
pub struct World {
    pub tick: u64,
    pub seed: u64,
    beings: BTreeMap<u64, Being>,
    #[serde(skip)]
    sched: Scheduler,
}

impl World {
    pub fn new(seed: u64) -> Self {
        Self { tick: 0, seed, beings: BTreeMap::new(), sched: Scheduler::new() }
    }

    pub fn spawn(&mut self, id: EntityId, pos: Pos, speed: u64) {
        self.beings.insert(id.0, Being { pos, next_at: 0, speed });
        self.sched.schedule(id, 0);
    }

    pub fn apply(&mut self, intent: &Intent) -> Result<(Delta, Vec<Event>), SimError> {
        let b = self.beings.get_mut(&intent.actor.0)
            .ok_or(SimError::new("unknown_actor", "spawned entity", "missing"))?;
        if self.tick < b.next_at {
            return Err(SimError::new("too_early", "tick >= next_at", "acted early"));
        }
        let mut pos = b.pos;
        match intent.verb {
            Verb::Wait => {}
            Verb::Move(d) => {
                let (dx, dy) = match d {
                    Dir::N => (0, -1), Dir::S => (0, 1),
                    Dir::E => (1, 0), Dir::W => (-1, 0),
                };
                let nx = pos.x as i16 + dx;
                let ny = pos.y as i16 + dy;
                // M0: 16x16 section, border walls at 0 and 15
                if nx >= 1 && nx <= 14 && ny >= 1 && ny <= 14 {
                    pos.x = nx as u8;
                    pos.y = ny as u8;
                }
            }
        }
        b.pos = pos;
        b.next_at = self.tick + b.speed.max(1);
        self.sched.schedule(intent.actor, b.next_at);
        self.tick += 1;
        Ok((
            Delta { moved: vec![(intent.actor, pos)] },
            vec![Event { tick: self.tick, actor: intent.actor, kind: "acted".into() }],
        ))
    }

    /// Deterministic FNV-1a over canonical JSON (no DefaultHasher instability).
    pub fn snapshot_hash(&self) -> u64 {
        let v = serde_json::to_value(self).unwrap_or(serde_json::Value::Null);
        let s = serde_json::to_string(&v).unwrap_or_default();
        let mut h: u64 = 0xcbf29ce484222325;
        for b in s.bytes() {
            h ^= b as u64;
            h = h.wrapping_mul(0x100000001b3);
        }
        h
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pos(x: u8, y: u8) -> Pos {
        Pos { city: 1, sx: 0, sy: 0, z: 0, x, y }
    }

    #[test]
    fn replay_is_deterministic() {
        let run = || {
            let mut w = World::new(42);
            w.spawn(EntityId(1), pos(8, 8), 10);
            for _ in 0..5 {
                let _ = w.apply(&Intent { actor: EntityId(1), verb: Verb::Move(Dir::E) });
                // fast-forward scheduler time like server would
                w.tick = w.beings[&1].next_at - 9;
                // note: test helper jumps; real server advances tick to next due
            }
            w.snapshot_hash()
        };
        // Same intents on fresh worlds must hash equal; wall clamp keeps in-bounds.
        let mut w1 = World::new(42);
        let mut w2 = World::new(42);
        w1.spawn(EntityId(1), pos(1, 1), 1);
        w2.spawn(EntityId(1), pos(1, 1), 1);
        for _ in 0..3 {
            let _ = w1.apply(&Intent { actor: EntityId(1), verb: Verb::Move(Dir::W) });
            let _ = w2.apply(&Intent { actor: EntityId(1), verb: Verb::Move(Dir::W) });
        }
        assert_eq!(w1.snapshot_hash(), w2.snapshot_hash());
        assert_eq!(w1.beings[&1].pos.x, 1); // border wall blocks
        let _ = run();
    }
}

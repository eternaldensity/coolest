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
    Eat,
    Work(u8),
    Watch,
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
    food: u8,   // 0-100, player-only needs (2024, ambient NPCs exempt per 2006)
    mood: u8,   // 0-100
    money: i64, // cents
    rations: u8,
}

/// M0.5 jobs: (payout_cents, mood_cost, food_cost).
const JOBS: [(i64, u8, u8); 2] = [(60, 5, 2), (120, 9, 4)];
const WATCH_COST: i64 = 30;

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
        self.beings.insert(id.0, Being {
            pos, next_at: 0, speed,
            food: 80, mood: 70, money: 200, rations: 3,
        });
        self.sched.schedule(id, 0);
    }

    /// Test helper: set needs directly.
    #[cfg(test)]
    fn set_needs(&mut self, id: EntityId, food: u8, mood: u8, money: i64, rations: u8) {
        if let Some(b) = self.beings.get_mut(&id.0) {
            b.food = food; b.mood = mood; b.money = money; b.rations = rations;
        }
    }

    pub fn needs(&self, id: EntityId) -> Option<(u8, u8, i64, u8)> {
        self.beings.get(&id.0).map(|b| (b.food, b.mood, b.money, b.rations))
    }

    /// Next scheduled act tick (server scheduler jumps the clock here).
    pub fn next_at(&self, id: EntityId) -> u64 {
        self.beings.get(&id.0).map(|b| b.next_at).unwrap_or(self.tick)
    }

    pub fn apply(&mut self, intent: &Intent) -> Result<(Delta, Vec<Event>), SimError> {
        let b = self.beings.get_mut(&intent.actor.0)
            .ok_or(SimError::new("unknown_actor", "spawned entity", "missing"))?;
        if self.tick < b.next_at {
            return Err(SimError::new("too_early", "tick >= next_at", "acted early"));
        }
        let mut pos = b.pos;
        // Pressure clock: every act costs 1 food + 1 mood (player-only).
        b.food = b.food.saturating_sub(1);
        b.mood = b.mood.saturating_sub(1);
        let mut kind = "acted";
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
                if (1..=14).contains(&nx) && (1..=14).contains(&ny) {
                    pos.x = nx as u8;
                    pos.y = ny as u8;
                }
            }
            Verb::Eat => {
                if b.rations == 0 {
                    return Err(SimError::new("no_food", "rations > 0", "empty"));
                }
                b.rations -= 1;
                b.food = (b.food + 30).min(100);
                kind = "ate";
            }
            Verb::Work(j) => {
                let (pay, mood_c, food_c) = *JOBS.get(j as usize)
                    .ok_or(SimError::new("no_job", "job 0-1", "bad id"))?;
                if b.food == 0 || b.mood == 0 {
                    return Err(SimError::new("job_lockout", "food>0,mood>0", "depleted"));
                }
                b.mood = b.mood.saturating_sub(mood_c);
                b.food = b.food.saturating_sub(food_c);
                b.money += pay;
                kind = "worked";
            }
            Verb::Watch => {
                if b.money < WATCH_COST {
                    return Err(SimError::new("broke", "money >= cost", "poor"));
                }
                b.money -= WATCH_COST;
                b.mood = (b.mood + 15).min(100);
                kind = "watched";
            }
        }
        b.pos = pos;
        // Starving doubles time cost (soft fail, not hard game-over per synthesis).
        let eff = if b.food == 0 { b.speed.max(1) * 2 } else { b.speed.max(1) };
        b.next_at = self.tick + eff;
        self.sched.schedule(intent.actor, b.next_at);
        self.tick += 1;
        Ok((
            Delta { moved: vec![(intent.actor, pos)] },
            vec![Event { tick: self.tick, actor: intent.actor, kind: kind.into() }],
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

    pub fn save_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| "{}".into())
    }

    pub fn load_json(s: &str) -> Result<Self, SimError> {
        let mut w: Self = serde_json::from_str(s)
            .map_err(|_| SimError::new("load", "valid snapshot", "corrupt"))?;
        w.rebuild_sched();
        Ok(w)
    }

    fn rebuild_sched(&mut self) {
        let mut s = Scheduler::new();
        for (raw, b) in &self.beings {
            s.schedule(EntityId(*raw), b.next_at);
        }
        self.sched = s;
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

    #[test]
    fn save_load_roundtrip_preserves_hash() {
        let mut w = World::new(7);
        w.spawn(EntityId(1), pos(8, 8), 2);
        let _ = w.apply(&Intent { actor: EntityId(1), verb: Verb::Move(Dir::E) });
        let s = w.save_json();
        let w2 = World::load_json(&s).expect("load");
        assert_eq!(w.snapshot_hash(), w2.snapshot_hash());
        assert!(World::load_json("{bad").is_err());
    }

    #[test]
    fn survival_loop_pays_costs_and_restores() {
        let mut w = World::new(1);
        w.spawn(EntityId(1), pos(8, 8), 1);
        // Starter: 80/70, 200c, 3 rations. Decay applies per act.
        let (_, mood0, m0, _) = w.needs(EntityId(1)).unwrap();
        let _ = w.apply(&Intent { actor: EntityId(1), verb: Verb::Work(0) });
        let (food, mood, money, _) = w.needs(EntityId(1)).unwrap();
        assert_eq!(money, m0 + 60);
        assert!(mood < mood0 && food < 80);

        // Eat consumes ration, restores food.
        let (_, _, _, r0) = w.needs(EntityId(1)).unwrap();
        let _ = w.apply(&Intent { actor: EntityId(1), verb: Verb::Eat });
        let (food2, _, _, r1) = w.needs(EntityId(1)).unwrap();
        assert_eq!(r1, r0 - 1);
        assert!(food2 > food);

        // Watch costs money, restores mood.
        let (_, mood_b, money_b, _) = w.needs(EntityId(1)).unwrap();
        let _ = w.apply(&Intent { actor: EntityId(1), verb: Verb::Watch });
        let (_, mood2, money2, _) = w.needs(EntityId(1)).unwrap();
        assert_eq!(money2, money_b - 30);
        assert!(mood2 > mood_b - 1); // net of decay + restore

        // Lockout at zero mood/food.
        w.set_needs(EntityId(1), 10, 0, 100, 1);
        // need tick >= next_at: jump clock like server would
        w.tick = w.beings[&1].next_at;
        assert!(w.apply(&Intent { actor: EntityId(1), verb: Verb::Work(0) }).is_err());
        // Starving doubles step cost.
        w.set_needs(EntityId(1), 1, 50, 100, 1);
        w.tick = w.beings[&1].next_at;
        let t0 = w.tick;
        let _ = w.apply(&Intent { actor: EntityId(1), verb: Verb::Wait });
        assert_eq!(w.beings[&1].next_at, t0 + 2); // speed 1 x2 while starving
    }
}

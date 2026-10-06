use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use crate::error::SimError;
use crate::ids::{EntityId, HostId};
use crate::civic::{Role, cop_duty, role_table};
use crate::crime::Standing;
use crate::ledger::Ledger;
use crate::pos::Pos;
use crate::scheduler::Scheduler;
use crate::shop::Shop;
use crate::street::{Ports, Section, gen_section};

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
    /// Buy an item from the shop at the actor's tile (ledgered).
    Buy(String),
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
struct PlacedShop {
    x: u8,
    y: u8,
    shop: Shop,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Npc {
    pos: Pos,
    next_at: u64,
    speed: u64,
    role: Role,
    home: (u8, u8),
    work: (u8, u8),
    leisure: (u8, u8),
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

/// M0.5 jobs: (payout_cents, mood_cost, food_cost). Defaults match
/// data/jobs.yaml; `set_jobs` overrides from file at runtime.
const DEFAULT_JOBS: [(i64, u8, u8); 2] = [(60, 5, 2), (120, 9, 4)];
const WATCH_COST: i64 = 30;

/// Minimal M0 world: 1 section 16x16, walls on border, one floor.
#[derive(Debug, Serialize, Deserialize)]
pub struct World {
    pub tick: u64,
    pub seed: u64,
    beings: BTreeMap<u64, Being>,
    #[serde(default = "default_jobs")]
    jobs: Vec<(i64, u8, u8)>,
    #[serde(default)]
    pub ledger: Ledger,
    #[serde(default)]
    accts: BTreeMap<u64, u64>,
    #[serde(default)]
    npcs: BTreeMap<u64, Npc>,
    #[serde(default)]
    sections: Vec<Section>,
    #[serde(default)]
    shops: Vec<PlacedShop>,
    #[serde(skip)]
    sched: Scheduler,
}

fn default_jobs() -> Vec<(i64, u8, u8)> {
    DEFAULT_JOBS.to_vec()
}

impl World {
    pub fn new(seed: u64) -> Self {
        Self { tick: 0, seed, beings: BTreeMap::new(), jobs: DEFAULT_JOBS.to_vec(), ledger: Ledger::new(), accts: BTreeMap::new(), shops: Vec::new(), npcs: BTreeMap::new(), sections: Vec::new(), sched: Scheduler::new() }
    }

    /// Override the job table from data/jobs.yaml (validated by data::parse_jobs).
    pub fn set_jobs(&mut self, defs: &[crate::data::JobDef]) {
        if !defs.is_empty() {
            self.jobs = defs.iter().map(|d| (d.payout_cents, d.mood_cost, d.food_cost)).collect();
        }
    }

    /// Generate (or fetch) a section, constraining shared-side ports from
    /// existing neighbors so roads/rivers continue across the seam.
    pub fn ensure_section(&mut self, sx: i32, sy: i32) {
        if self.sections.iter().any(|s| s.sx == sx && s.sy == sy) {
            return;
        }
        let port = |x: i32, y: i32, f: fn(&Section) -> Option<u8>| {
            self.sections.iter().find(|s| s.sx == x && s.sy == y).and_then(f)
        };
        let mut want = Ports::default();
        if let Some(w) = port(sx - 1, sy, |s| s.ports.e) {
            want.w = Some(w);
        }
        if let Some(w) = port(sx + 1, sy, |s| s.ports.w) {
            want.e = Some(w);
        }
        if let Some(w) = port(sx, sy - 1, |s| s.ports.s) {
            want.n = Some(w);
        }
        if let Some(w) = port(sx, sy + 1, |s| s.ports.n) {
            want.s = Some(w);
        }
        self.sections.push(gen_section(self.seed, sx, sy, want));
    }

    pub fn section_at(&self, sx: i32, sy: i32) -> Option<&Section> {
        self.sections.iter().find(|s| s.sx == sx && s.sy == sy)
    }

    pub fn section_count(&self) -> usize {
        self.sections.len()
    }

    pub fn spawn(&mut self, id: EntityId, pos: Pos, speed: u64) {
        self.ensure_section(pos.sx, pos.sy);
        self.beings.insert(id.0, Being {
            pos, next_at: 0, speed,
            food: 80, mood: 70, money: 200, rations: 3,
        });
        self.sched.schedule(id, 0);
    }

    /// Open a bank account for a being (branch host). Purchases need one.
    pub fn open_account(&mut self, id: EntityId, branch: HostId) -> Result<u64, SimError> {
        if !self.beings.contains_key(&id.0) {
            return Err(SimError::new("unknown_actor", "spawned entity", "missing"));
        }
        let acct = self.ledger.open(id, branch);
        self.accts.insert(id.0, acct);
        Ok(acct)
    }

    /// Place a shop on a local tile (counter sale; design/05-06).
    pub fn place_shop(&mut self, x: u8, y: u8, shop: Shop) {
        if let Some(slot) = self.shops.iter_mut().find(|s| s.x == x && s.y == y) {
            slot.shop = shop;
        } else {
            self.shops.push(PlacedShop { x, y, shop });
        }
    }

    pub fn shop_at(&self, x: u8, y: u8) -> Option<&Shop> {
        self.shops.iter().find(|s| s.x == x && s.y == y).map(|s| &s.shop)
    }

    /// Spawn an ambient NPC (no hunger/mood — design 2006 scope cut).
    /// home/work/leisure are local tiles; timetable comes from role.
    pub fn spawn_npc(&mut self, id: EntityId, role: Role, home: (u8, u8), work: (u8, u8), leisure: (u8, u8), speed: u64) {
        self.npcs.insert(id.0, Npc { pos: Pos { city: 1, sx: 0, sy: 0, z: 0, x: home.0, y: home.1 }, next_at: 0, speed, role, home, work, leisure });
    }

    pub fn npc_positions(&self) -> Vec<(u64, u8, u8)> {
        self.npcs.iter().map(|(id, n)| (*id, n.pos.x, n.pos.y)).collect()
    }

    /// Advance NPCs whose turn is due. hour drives timetables; wanted maps
    /// being-id -> standing for cop pursuit (nearest Wanted target).
    /// NPC moves are free within the tick (M1 LOD, noted in design/05).
    pub fn step_npcs(&mut self, hour: u8, wanted: &BTreeMap<u64, Standing>) {
        let ids: Vec<u64> = self.npcs.keys().copied().collect();
        for id in ids {
            let due = self.npcs[&id].next_at;
            if due > self.tick {
                continue;
            }
            let table = role_table(self.npcs[&id].role);
            let target = self.npc_target(id, &table, hour, wanted);
            let n = self.npcs.get_mut(&id).unwrap();
            let (dx, dy) = (target.0 as i16 - n.pos.x as i16, target.1 as i16 - n.pos.y as i16);
            // Step along the farther axis (4-dir, border-clamped like PCs).
            let (nx, ny) = if dx.abs() >= dy.abs() {
                (n.pos.x as i16 + dx.signum(), n.pos.y as i16)
            } else {
                (n.pos.x as i16, n.pos.y as i16 + dy.signum())
            };
            if (1..=14).contains(&nx) && (1..=14).contains(&ny) {
                n.pos.x = nx as u8;
                n.pos.y = ny as u8;
            }
            n.next_at = self.tick + n.speed.max(1);
        }
    }

    fn npc_target(&self, id: u64, table: &crate::civic::Timetable, hour: u8, wanted: &BTreeMap<u64, Standing>) -> (u8, u8) {
        let n = &self.npcs[&id];
        if n.role == Role::Cop {
            // Nearest Wanted being, if any — pursuit overrides the timetable.
            let mut best: Option<((u8, u8), i32)> = None;
            for (bid, b) in &self.beings {
                if wanted.get(bid).copied().unwrap_or(Standing::Clean) != Standing::Wanted {
                    continue;
                }
                let d = (b.pos.x as i32 - n.pos.x as i32).abs() + (b.pos.y as i32 - n.pos.y as i32).abs();
                if best.map(|(_, bd)| d < bd).unwrap_or(true) {
                    best = Some(((b.pos.x, b.pos.y), d));
                }
            }
            if let Some((t, _)) = best {
                // Pursuit overrides the timetable (cops stay cops off shift).
                let _duty = cop_duty(table, hour, Standing::Wanted);
                debug_assert_eq!(_duty, crate::civic::Activity::Pursue);
                return t;
            }
        }
        match table.at(hour) {
            crate::civic::Activity::Work | crate::civic::Activity::Patrol => n.work,
            crate::civic::Activity::Leisure => n.leisure,
            _ => n.home,
        }
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

    pub fn pos_of(&self, id: EntityId) -> Option<Pos> {
        self.beings.get(&id.0).map(|b| b.pos)
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
                if (1..=14).contains(&nx) && (1..=14).contains(&ny) {
                    pos.x = nx as u8;
                    pos.y = ny as u8;
                } else {
                    // Section crossing: wrap to the neighbor, generating it
                    // with the crossing row/col as its entry port.
                    let (nsx, nsy, wx, wy, side) = match d {
                        Dir::W => (pos.sx - 1, pos.sy, 14u8, pos.y, ("e", pos.y)),
                        Dir::E => (pos.sx + 1, pos.sy, 1u8, pos.y, ("w", pos.y)),
                        Dir::N => (pos.sx, pos.sy - 1, pos.x, 14u8, ("s", pos.x)),
                        Dir::S => (pos.sx, pos.sy + 1, pos.x, 1u8, ("n", pos.x)),
                    };
                    let mut want = Ports::default();
                    match side.0 {
                        "e" => want.e = Some(side.1),
                        "w" => want.w = Some(side.1),
                        "s" => want.s = Some(side.1),
                        _ => want.n = Some(side.1),
                    }
                    if !self.sections.iter().any(|s| s.sx == nsx && s.sy == nsy) {
                        self.sections.push(gen_section(self.seed, nsx, nsy, want));
                    }
                    pos.sx = nsx;
                    pos.sy = nsy;
                    pos.x = wx;
                    pos.y = wy;
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
                let (pay, mood_c, food_c) = *self.jobs.get(j as usize)
                    .ok_or(SimError::new("no_job", "known job", "bad id"))?;
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
            Verb::Buy(ref item) => {
                let shop = self.shops.iter_mut().find(|s| s.x == pos.x && s.y == pos.y)
                    .ok_or(SimError::new("no_shop", "shop counter", "none here"))?;
                let shop = &mut shop.shop;
                let acct = self.accts.get(&intent.actor.0).copied()
                    .ok_or(SimError::new("no_account", "open account", "unbanked"))?;
                shop.buy(&mut self.ledger, acct, item)?;
                // M1 food shops sell rations that go straight to the pack.
                if item == "ration" {
                    b.rations = b.rations.saturating_add(1);
                }
                kind = "bought";
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
        // Borders now cross sections instead of blocking: three steps west
        // from (1,1) lands two tiles into the western neighbor.
        assert_eq!((w1.beings[&1].pos.sx, w1.beings[&1].pos.x), (-1, 12));
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

    #[test]
    fn shop_counter_buy_then_eat() {
        use crate::shop::Shop;
        let mut w = World::new(3);
        w.spawn(EntityId(1), pos(8, 8), 1);
        let acct = w.open_account(EntityId(1), crate::ids::HostId(2)).unwrap();
        w.ledger.mint(acct, 500).unwrap();
        let mut shop = Shop::new(w.ledger.open(EntityId(9), crate::ids::HostId(9)));
        shop.add_line("ration", 100, 2, 1, 5, 50);
        w.place_shop(8, 8, shop);
        // Walk away: no counter here.
        w.tick = w.next_at(EntityId(1));
        let _ = w.apply(&Intent { actor: EntityId(1), verb: Verb::Move(Dir::E) });
        w.tick = w.next_at(EntityId(1));
        assert!(w.apply(&Intent { actor: EntityId(1), verb: Verb::Buy("ration".into()) }).is_err());
        // Back on the tile: buy, pack gains a ration, then eat it.
        w.tick = w.next_at(EntityId(1));
        let _ = w.apply(&Intent { actor: EntityId(1), verb: Verb::Move(Dir::W) });
        w.tick = w.next_at(EntityId(1));
        w.apply(&Intent { actor: EntityId(1), verb: Verb::Buy("ration".into()) }).unwrap();
        assert_eq!(w.needs(EntityId(1)).unwrap().3, 4); // spawn 3 + 1 bought
        w.tick = w.next_at(EntityId(1));
        let (f0, _, _, _) = w.needs(EntityId(1)).unwrap();
        w.apply(&Intent { actor: EntityId(1), verb: Verb::Eat }).unwrap();
        assert!(w.needs(EntityId(1)).unwrap().0 > f0);
        assert_eq!(w.ledger.live_sum(), w.ledger.audit_sum());
        // Unbanked stranger cannot buy.
        w.spawn(EntityId(2), pos(8, 8), 1);
        w.tick = w.next_at(EntityId(2));
        assert!(w.apply(&Intent { actor: EntityId(2), verb: Verb::Buy("ration".into()) }).is_err());
    }

    #[test]
    fn npcs_walk_timetables() {
        use std::collections::BTreeMap;
        let mut w = World::new(1);
        w.spawn_npc(EntityId(10), Role::Shopkeeper, (2, 2), (12, 12), (8, 2), 1);
        // Morning: head to work.
        for _ in 0..30 {
            w.step_npcs(10, &BTreeMap::new());
            w.tick += 1;
        }
        let p = w.npc_positions();
        assert_eq!(p.len(), 1);
        assert_eq!((p[0].1, p[0].2), (12, 12));
        // Night: back home.
        for _ in 0..30 {
            w.step_npcs(23, &BTreeMap::new());
            w.tick += 1;
        }
        let p = w.npc_positions();
        assert_eq!((p[0].1, p[0].2), (2, 2));
    }

    #[test]
    fn cop_pursues_wanted_ignores_clean() {
        use std::collections::BTreeMap;
        let mut w = World::new(1);
        w.spawn(EntityId(1), pos(8, 8), 1);
        w.spawn_npc(EntityId(20), Role::Cop, (2, 2), (12, 12), (8, 2), 1);
        // Clean citizen: cop works its beat (off to work at 10).
        w.step_npcs(10, &BTreeMap::new());
        let home = w.npc_positions()[0];
        assert_ne!((home.1, home.2), (8, 8));
        // Wanted: cop converges on the player.
        let mut wanted = BTreeMap::new();
        wanted.insert(1u64, Standing::Wanted);
        for _ in 0..20 {
            w.step_npcs(16, &wanted.clone());
            w.tick += 1;
        }
        let p = w.npc_positions()[0];
        assert_eq!((p.1, p.2), (8, 8));
    }

    #[test]
    fn section_crossing_generates_matching_neighbors() {
        let mut w = World::new(11);
        w.spawn(EntityId(1), pos(8, 8), 1);
        assert_eq!(w.section_count(), 1);
        // Walk east across the seam: 7 steps from x=8 (9..14, cross, 1).
        for _ in 0..7 {
            w.tick = w.next_at(EntityId(1));
            w.apply(&Intent { actor: EntityId(1), verb: Verb::Move(Dir::E) }).unwrap();
        }
        let b = &w.beings[&1];
        assert_eq!((b.pos.sx, b.pos.x), (1, 1));
        assert_eq!(w.section_count(), 2);
        // Entry port on the shared side matches the crossing row (y=8).
        assert_eq!(w.section_at(1, 0).unwrap().ports.w, Some(8));
        // Walk back west: no new section, same sim both ways.
        for _ in 0..7 {
            w.tick = w.next_at(EntityId(1));
            w.apply(&Intent { actor: EntityId(1), verb: Verb::Move(Dir::W) }).unwrap();
        }
        let b = &w.beings[&1];
        // One step crosses (lands x=14), six more walk to x=8.
        assert_eq!((b.pos.sx, b.pos.x), (0, 8));
        assert_eq!(w.section_count(), 2);
        // Deterministic twin replays the whole trip and agrees.
        let mut v = World::new(11);
        v.spawn(EntityId(1), pos(8, 8), 1);
        for d in [Dir::E; 7].into_iter().chain([Dir::W; 7]) {
            v.tick = v.next_at(EntityId(1));
            v.apply(&Intent { actor: EntityId(1), verb: Verb::Move(d) }).unwrap();
        }
        assert_eq!(w.snapshot_hash(), v.snapshot_hash());
    }

    #[test]
    fn jobs_override_from_data_file() {
        use crate::data::parse_jobs;
        let defs = parse_jobs(include_str!("../../data/jobs.yaml")).unwrap();
        let mut w = World::new(1);
        w.set_jobs(&defs);
        w.spawn(EntityId(1), pos(8, 8), 1);
        let (_, _, m0, _) = w.needs(EntityId(1)).unwrap();
        let _ = w.apply(&Intent { actor: EntityId(1), verb: Verb::Work(1) });
        let (_, _, m1, _) = w.needs(EntityId(1)).unwrap();
        assert_eq!(m1, m0 + 120); // file value, not code constant
        // Old snapshots without a jobs table still load (serde default).
        let mut w2 = World::load_json(r#"{"tick":0,"seed":1,"beings":{}}"#).unwrap();
        w2.spawn(EntityId(9), pos(8, 8), 1);
        let (_, _, n0, _) = w2.needs(EntityId(9)).unwrap();
        let _ = w2.apply(&Intent { actor: EntityId(9), verb: Verb::Work(0) });
        assert_eq!(w2.needs(EntityId(9)).unwrap().2, n0 + 60);
    }
}

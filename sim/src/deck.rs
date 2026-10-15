use crate::error::SimError;
use crate::ids::{EntityId, HostId};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, VecDeque};

/// M2a Deck arena: one host's RAM grid + programs.
/// M2b adds cards/resources/paired-stats; M2c adds trace/vulns.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct CellPos(pub u8, pub u8);

/// M2b resource types (2024): programming components creatures produce.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Res {
    Loop,
    Data,
    Algo,
}

impl Res {
    /// All three types, in tier order.
    pub const ALL: [Res; 3] = [Res::Loop, Res::Data, Res::Algo];

    /// Stable index for compact storage / integer requirements.
    pub fn as_index(self) -> u8 {
        match self {
            Res::Loop => 0,
            Res::Data => 1,
            Res::Algo => 2,
        }
    }

    /// Inverse of [`Res::as_index`]; unknown indices fall back to Loop.
    pub fn from_index(i: u8) -> Res {
        match i {
            1 => Res::Data,
            2 => Res::Algo,
            _ => Res::Loop,
        }
    }

    /// Lower-case name as used in data files.
    pub fn name(self) -> &'static str {
        match self {
            Res::Loop => "loop",
            Res::Data => "data",
            Res::Algo => "algo",
        }
    }
}

/// M2b paired Deck stats (2024). Attacks target the negative pole, next-turn only.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeckStats {
    pub dedication: u8,
    pub focus: u8,
    pub stability: u8,
    pub courage: u8,
    pub clarity: u8,
}

impl Default for DeckStats {
    fn default() -> Self {
        Self {
            dedication: 10,
            focus: 10,
            stability: 10,
            courage: 10,
            clarity: 10,
        }
    }
}

impl DeckStats {
    pub fn lowest(&self) -> &'static str {
        let mut best = ("dedication", self.dedication);
        for (n, v) in [
            ("focus", self.focus),
            ("stability", self.stability),
            ("courage", self.courage),
            ("clarity", self.clarity),
        ] {
            if v < best.1 {
                best = (n, v);
            }
        }
        best.0
    }
}

/// M2b next-turn-only effects (design/03 closed table). Dice affect crits,
/// not baseline segment damage, so basic-strike TTK stays deterministic.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Fx {
    SkipNext,
    ShieldNext(u8),
    DotNext(u8),
    /// The victim's next attack roll is reduced by N (floor 1).
    RollMinus(u8),
    /// The victim's next attack cannot crit on this face (treated as one
    /// lower, so a 6 becomes 5).
    BanDie(u8),
    /// The next attack on this program is absorbed entirely (decoy zero).
    DecoyNext,
    /// This program's next attack deals half damage (floor 1).
    DowngradeNext,
    /// Return the next incoming damage to the attacker.
    ReflectNext,
    /// This program's next attack also deals 1 to orthogonally adjacent
    /// program cells.
    SplashNext,
    /// Roll twice on the next attack and keep the higher face.
    ExtraRollNext,
    /// The next attack retargets to a random occupied cell (design/03
    /// "random-target").
    RandomTargetNext,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Segment {
    prog: EntityId,
    is_head: bool,
}

/// JSON-safe cell slot (tuple map keys do not survive serde).
#[derive(Debug, Clone, Serialize, Deserialize)]
struct CellSlot {
    x: u8,
    y: u8,
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
struct VulnState {
    patched: bool,
    burned: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct LogEntry {
    who: EntityId,
    when: u64,
    via: HostId,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Trace {
    target: EntityId,
    progress: u32,
    threshold: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Host {
    pub id: HostId,
    pub w: u8,
    pub h: u8,
    pub cpu_total: u32,
    cells: Vec<CellSlot>,
    pub progs: BTreeMap<u64, Program>,
    // M2b: per-program makers (types it can produce + rate), per-owner stocks,
    // per-program paired stats + pending next-turn effects.
    makers: BTreeMap<u64, (Vec<Res>, u8)>,
    stocks: BTreeMap<u64, BTreeMap<Res, u32>>,
    stats: BTreeMap<u64, DeckStats>,
    effects: BTreeMap<u64, Vec<Fx>>,
    // M2c: vulns, breach flag, transit logs, inbound trace, monitor.
    vulns: BTreeMap<String, VulnState>,
    pub breached: bool,
    logs: Vec<LogEntry>,
    trace: Option<Trace>,
    monitor_armed: bool,
    pub flagged: bool,
    // Installed packet filter (design/04): inbound traces crawl at +1/tick.
    #[serde(default)]
    firewall: bool,
    // Nominal owner of this rig. A monitored, owned rig retaliates against
    // programs owned by anyone else; unowned rigs stay quiet (wild boxes).
    // Defaults to None so old saves and fresh arenas behave as before.
    #[serde(default)]
    owner: Option<EntityId>,
    /// Deterministic dice state (design/03: resolution uses dice). Advanced
    /// once per roll, serialized so save/replay reproduce the same faces.
    /// Default is a nonzero constant so old saves still roll.
    #[serde(default = "default_rng")]
    rng: u64,
}

/// Nonzero xorshift seed for hosts without a serialized rng state.
fn default_rng() -> u64 {
    0x9E37_79B9_7F4A_7C15
}

/// What one retaliation beat did. Clients use `traced` to mark funnel
/// milestones and `killed`/`burned` to narrate.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Retaliation {
    /// A warden stands guard (freshly spawned or already posted).
    pub warden: bool,
    /// A trace opened on an intruder this beat.
    pub traced: bool,
    /// The warden struck something this beat.
    pub struck: bool,
    /// The warden killed a program this beat.
    pub killed: bool,
    /// A completed trace burned a program this beat.
    pub burned: bool,
}

/// What one card attack did (design/03 resolution): for narration/tests.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct AttackOutcome {
    pub killed: bool,
    pub damage: u32,
    pub face: u8,
    pub crit: bool,
    pub reflected: bool,
    pub splashed: Vec<EntityId>,
    pub retargeted: bool,
    pub absorbed: bool,
}

impl Host {
    pub fn new(id: HostId, w: u8, h: u8, cpu_total: u32) -> Self {
        Self {
            id,
            w,
            h,
            cpu_total,
            cells: Vec::new(),
            progs: BTreeMap::new(),
            makers: BTreeMap::new(),
            stocks: BTreeMap::new(),
            stats: BTreeMap::new(),
            effects: BTreeMap::new(),
            vulns: BTreeMap::new(),
            breached: false,
            logs: Vec::new(),
            trace: None,
            monitor_armed: false,
            flagged: false,
            firewall: false,
            owner: None,
            // Seed from the host id so distinct rigs roll differently, and
            // never zero (xorshift would stick).
            rng: default_rng() ^ (id.0.wrapping_mul(0x2545_F491_4F6C_DD1D) | 1),
        }
    }

    /// Advance the dice state and return a face in `1..=sides`.
    pub fn roll(&mut self, sides: u8) -> u8 {
        // xorshift64
        let mut x = self.rng;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.rng = x;
        let sides = sides.max(1) as u64;
        ((x % sides) + 1) as u8
    }

    pub fn cpu_load(&self) -> u32 {
        self.progs
            .values()
            .map(|p| 1 + p.len() as u32 * p.cpu_per_seg as u32)
            .sum()
    }

    pub fn overloaded(&self) -> bool {
        self.cpu_load() > self.cpu_total
    }

    fn free(&self, at: CellPos) -> bool {
        at.0 < self.w && at.1 < self.h && !self.cells.iter().any(|c| c.x == at.0 && c.y == at.1)
    }

    fn cell_get(&self, x: u8, y: u8) -> Option<Segment> {
        self.cells
            .iter()
            .find(|c| c.x == x && c.y == y)
            .map(|c| Segment {
                prog: c.prog,
                is_head: c.is_head,
            })
    }

    fn cell_put(&mut self, x: u8, y: u8, prog: EntityId, is_head: bool) {
        if let Some(c) = self.cells.iter_mut().find(|c| c.x == x && c.y == y) {
            c.prog = prog;
            c.is_head = is_head;
        } else {
            self.cells.push(CellSlot {
                x,
                y,
                prog,
                is_head,
            });
        }
    }

    fn cell_drop(&mut self, x: u8, y: u8) {
        self.cells.retain(|c| !(c.x == x && c.y == y));
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
            return Err(SimError::new(
                "cell_busy",
                "free cell",
                "occupied/out of bounds",
            ));
        }
        if self.progs.contains_key(&prog.0) {
            return Err(SimError::new("dup_prog", "fresh id", "exists"));
        }
        // Project load including newcomer; refuse if it would overload.
        let add = 1 + cpu_per_seg as u32;
        if self.cpu_load() + add > self.cpu_total {
            return Err(SimError::new("cpu_overload", "headroom", "over budget"));
        }
        self.cell_put(at.0, at.1, prog, true);
        self.progs.insert(
            prog.0,
            Program {
                id: prog,
                owner,
                head: at,
                tail: VecDeque::new(),
                max_len: max_len.max(1),
                cpu_per_seg,
            },
        );
        Ok(())
    }

    /// 4-dir step of the head. Grows by 1 unless at max_len (then slides: drop last tail).
    pub fn step(&mut self, prog: EntityId, to: CellPos) -> Result<(), SimError> {
        if !self.free(to) {
            return Err(SimError::new(
                "cell_busy",
                "free cell",
                "occupied/out of bounds",
            ));
        }
        // 4-dir adjacency.
        let p =
            self.progs
                .get(&prog.0)
                .ok_or(SimError::new("no_prog", "live program", "missing"))?;
        let dx = (to.0 as i16 - p.head.0 as i16).abs();
        let dy = (to.1 as i16 - p.head.1 as i16).abs();
        if dx + dy != 1 {
            return Err(SimError::new("not_adjacent", "4-dir step", "jump"));
        }
        if self.overloaded() {
            return Err(SimError::new("cpu_overload", "headroom", "growth blocked"));
        }
        let old_head = self.progs.get(&prog.0).unwrap().head;
        // Old head becomes tail segment.
        self.cell_put(old_head.0, old_head.1, prog, false);
        self.cell_put(to.0, to.1, prog, true);
        let dropped: Vec<CellPos> = {
            let p = self.progs.get_mut(&prog.0).unwrap();
            p.tail.push_front(old_head);
            p.head = to;
            let mut dropped = Vec::new();
            while p.tail.len() + 1 > p.max_len as usize {
                if let Some(last) = p.tail.pop_back() {
                    dropped.push(last);
                }
            }
            dropped
        };
        for last in dropped {
            self.cell_drop(last.0, last.1);
        }
        Ok(())
    }

    /// Strike a cell: deletes one tail segment, or kills on headshot.
    /// Returns true if a program died.
    pub fn strike(&mut self, target: CellPos) -> Result<bool, SimError> {
        let seg = self.cell_get(target.0, target.1).ok_or(SimError::new(
            "no_target",
            "program cell",
            "empty",
        ))?;
        let victim = seg.prog;
        if seg.is_head {
            self.remove_program(victim);
            Ok(true)
        } else {
            // Remove that tail segment from victim.
            if let Some(p) = self.progs.get_mut(&victim.0) {
                p.tail.retain(|c| *c != target);
            }
            self.cell_drop(target.0, target.1);
            Ok(false)
        }
    }

    /// Takeover: attacker's head must be 4-adjacent to victim's head.
    /// Ownership flips; costs the caller 2 acts (enforced by scheduler side).
    pub fn takeover(&mut self, attacker: EntityId, victim_head: CellPos) -> Result<(), SimError> {
        let seg = self
            .cell_get(victim_head.0, victim_head.1)
            .ok_or(SimError::new("no_target", "program head", "empty"))?;
        if !seg.is_head {
            return Err(SimError::new("not_head", "head cell", "tail"));
        }
        let a = self.progs.get(&attacker.0).ok_or(SimError::new(
            "no_prog",
            "live attacker",
            "missing",
        ))?;
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
            self.cell_drop(p.head.0, p.head.1);
            for c in p.tail {
                self.cell_drop(c.0, c.1);
            }
        }
    }

    pub fn owner_of(&self, prog: EntityId) -> Option<EntityId> {
        self.progs.get(&prog.0).map(|p| p.owner)
    }

    /// Clone a segment to a free cell (the `copy` card). The child starts at
    /// len 1 with the same owner, build, and maker — never shares cells.
    pub fn clone_to(
        &mut self,
        prog: EntityId,
        new_id: EntityId,
        at: CellPos,
    ) -> Result<(), SimError> {
        let p = self.progs.get(&prog.0).cloned().ok_or(SimError::new(
            "no_prog",
            "live program",
            "missing",
        ))?;
        if !self.free(at) {
            return Err(SimError::new(
                "cell_busy",
                "free cell",
                "occupied/out of bounds",
            ));
        }
        if self.progs.contains_key(&new_id.0) {
            return Err(SimError::new("dup_prog", "fresh id", "exists"));
        }
        if self.cpu_load() + 1 + p.cpu_per_seg as u32 > self.cpu_total {
            return Err(SimError::new("cpu_overload", "headroom", "over budget"));
        }
        self.cell_put(at.0, at.1, new_id, true);
        self.progs.insert(
            new_id.0,
            Program {
                id: new_id,
                owner: p.owner,
                head: at,
                tail: VecDeque::new(),
                max_len: p.max_len,
                cpu_per_seg: p.cpu_per_seg,
            },
        );
        if let Some(maker) = self.makers.get(&prog.0).cloned() {
            self.makers.insert(new_id.0, maker);
        }
        Ok(())
    }

    /// Card ids in data/cards.yaml that this build implements. The yaml
    /// cross-check test fails on drift in either direction.
    pub fn implemented_cards() -> Vec<&'static str> {
        vec![
            // negative
            "distract",
            "freakout",
            "stun",
            "jam",
            "hex",
            "decoy",
            "downgrade",
            "shuffle",
            // positive
            "tank",
            "plating",
            "patch",
            "spike",
            "overload",
            "reflect",
            "splash",
            "extra",
            // manipulation
            "farm",
            "copy",
            "load",
            "upgrade",
        ]
    }

    // ---- M2c: vulns, trace/bounce/logs, monitor ----

    /// Register a vuln on this host (data-driven in vulns.yaml later; 3 for M2).
    pub fn add_vuln(&mut self, id: &str) {
        self.vulns.insert(
            id.to_string(),
            VulnState {
                patched: false,
                burned: false,
            },
        );
    }

    /// Exploit fires once per vuln: sets breached. Patched or burned → Err.
    pub fn exploit(&mut self, prog: EntityId, vuln: &str) -> Result<(), SimError> {
        if !self.progs.contains_key(&prog.0) {
            return Err(SimError::new("no_prog", "live program", "missing"));
        }
        let v =
            self.vulns
                .get_mut(vuln)
                .ok_or(SimError::new("no_vuln", "known vuln", "unknown"))?;
        if v.patched {
            return Err(SimError::new("patched", "unpatched vuln", "patched"));
        }
        if v.burned {
            return Err(SimError::new("burned", "fresh exploit", "already used"));
        }
        v.burned = true;
        self.breached = true;
        if self.monitor_armed {
            self.flagged = true;
        }
        Ok(())
    }

    pub fn patch(&mut self, vuln: &str) -> Result<(), SimError> {
        let v =
            self.vulns
                .get_mut(vuln)
                .ok_or(SimError::new("no_vuln", "known vuln", "unknown"))?;
        v.patched = true;
        Ok(())
    }

    /// Append a transit log entry (every hop records you).
    pub fn log(&mut self, who: EntityId, when: u64, via: HostId) {
        self.logs.push(LogEntry { who, when, via });
    }

    /// Scrub your entries. Requires one of your programs present on this host
    /// (presence = access; Private-cell gating comes with zones in M3).
    pub fn scrub_log(&mut self, requester: EntityId) -> Result<usize, SimError> {
        if !self.progs.values().any(|p| p.owner == requester) {
            return Err(SimError::new("no_access", "program on host", "absent"));
        }
        let before = self.logs.len();
        self.logs.retain(|e| e.who != requester);
        Ok(before - self.logs.len())
    }

    /// Start an inbound trace (needs Tracer running on the defender side).
    pub fn start_trace(&mut self, target: EntityId, threshold: u32) -> Result<(), SimError> {
        if !self.progs.contains_key(&target.0) {
            return Err(SimError::new("no_prog", "live target", "missing"));
        }
        self.trace = Some(Trace {
            target,
            progress: 0,
            threshold,
        });
        if self.monitor_armed {
            self.flagged = true;
        }
        Ok(())
    }

    /// Advance the trace one tick. Scrambler halves the rate (1 vs 2),
    /// unless an installed firewall filters: then +1/tick regardless.
    /// Returns true when the target is fully traced.
    pub fn tick_trace(&mut self, scrambled: bool) -> bool {
        match &mut self.trace {
            None => false,
            Some(t) => {
                t.progress += if self.firewall || scrambled { 1 } else { 2 };
                t.progress >= t.threshold
            }
        }
    }

    /// Install (or remove) the packet filter. Idempotent.
    pub fn set_firewall(&mut self, on: bool) {
        self.firewall = on;
    }

    pub fn firewall(&self) -> bool {
        self.firewall
    }

    pub fn set_monitor(&mut self, armed: bool) {
        self.monitor_armed = armed;
    }

    /// Claim this rig for a defender (or release it with None). Only a
    /// monitored, owned rig retaliates; see `retaliate`.
    pub fn set_owner(&mut self, owner: Option<EntityId>) {
        self.owner = owner;
    }

    pub fn owner(&self) -> Option<EntityId> {
        self.owner
    }

    /// Reserved system identity that owns defended rigs and their wardens.
    /// Never spawn it: ownership and warden programs address it directly.
    pub const WARDEN: EntityId = EntityId(u64::MAX - 1);

    /// Inbound-trace threshold retaliation opens on intruders.
    pub const TRACE_THRESHOLD: u32 = 6;

    /// One AI defense beat for a monitored, owned rig: tick an active trace
    /// (completion burns the program), open a trace on the first intruder,
    /// post a warden owned by the defender, and hunt one step (strike an
    /// adjacent intruder head, else close a BFS step). Unowned or
    /// unmonitored rigs stay quiet; full or CPU-starved arenas skip the
    /// warden gracefully. Deterministic: same state, same beat.
    pub fn retaliate(&mut self) -> Retaliation {
        let mut out = Retaliation::default();
        let defender = match self.owner {
            Some(o) => o,
            None => return out,
        };
        if !self.monitor_armed {
            return out;
        }
        // 1. Advance an active trace; completion burns the program.
        if self.trace.is_some() && self.tick_trace(false) {
            if let Some(t) = self.trace.take() {
                if self.progs.contains_key(&t.target.0) {
                    self.remove_program(t.target);
                    out.burned = true;
                }
            }
        }
        // Intruders: live programs owned by anyone but the defender,
        // lowest program id first for determinism. Passive producers
        // (makers) are not intruders — the warden hunts offensive
        // programs — so resource creatures survive on a defended rig.
        let mut targets: Vec<(EntityId, CellPos)> = self
            .progs
            .values()
            .filter(|p| p.owner != defender && !self.makers.contains_key(&p.id.0))
            .map(|p| (p.id, p.head))
            .collect();
        if targets.is_empty() {
            out.warden = self.progs.values().any(|p| p.owner == defender);
            return out;
        }
        targets.sort_by_key(|(id, _)| id.0);
        // 2. Open a trace on the first intruder if none running.
        if self.trace.is_none()
            && self
                .start_trace(targets[0].0, Self::TRACE_THRESHOLD)
                .is_ok()
        {
            out.traced = true;
        }
        // 3. Ensure the warden stands guard under defender colors.
        let ward = match self.progs.values().find(|p| p.owner == defender) {
            Some(p) => p.id,
            None => {
                let id = match (1u64..=1024)
                    .map(EntityId)
                    .find(|i| !self.progs.contains_key(&i.0))
                {
                    Some(id) => id,
                    None => return out,
                };
                let mut spot = None;
                for y in 0..self.h {
                    for x in 0..self.w {
                        if self.free(CellPos(x, y)) {
                            spot = Some(CellPos(x, y));
                            break;
                        }
                    }
                    if spot.is_some() {
                        break;
                    }
                }
                match spot {
                    Some(at) => {
                        if self.load(id, defender, at, 2, 1).is_err() {
                            return out;
                        }
                        id
                    }
                    None => return out,
                }
            }
        };
        out.warden = true;
        // 4. Hunt: strike an adjacent intruder head, else close one step.
        let Some(hhead) = self.progs.get(&ward.0).map(|p| p.head) else {
            return out;
        };
        let near = |a: CellPos, b: CellPos| {
            (a.0 as i16 - b.0 as i16).abs() + (a.1 as i16 - b.1 as i16).abs() == 1
        };
        if let Some((_, vhead)) = targets.iter().find(|(_, vh)| near(*vh, hhead)) {
            if let Ok(killed) = self.strike(*vhead) {
                out.struck = true;
                out.killed = killed;
            }
        } else {
            let goals: Vec<CellPos> = targets.iter().map(|(_, h)| *h).collect();
            if let Some(step) = self.hunt_step(hhead, &goals) {
                let _ = self.step(ward, step);
            }
        }
        out
    }

    /// One BFS step from `from` over free cells toward adjacency with any
    /// goal head. None when already adjacent (strike instead) or when no
    /// reachable adjacent cell exists.
    fn hunt_step(&self, from: CellPos, goals: &[CellPos]) -> Option<CellPos> {
        let near = |x: u8, y: u8| {
            goals
                .iter()
                .any(|g| (g.0 as i16 - x as i16).abs() + (g.1 as i16 - y as i16).abs() == 1)
        };
        let mut prev: BTreeMap<(u8, u8), (u8, u8)> = BTreeMap::new();
        let mut queue = VecDeque::new();
        queue.push_back(from);
        prev.insert((from.0, from.1), (from.0, from.1));
        while let Some(CellPos(x, y)) = queue.pop_front() {
            for (dx, dy) in [(0i16, -1), (0, 1), (1, 0), (-1, 0)] {
                let (nx, ny) = (x as i16 + dx, y as i16 + dy);
                if nx < 0 || ny < 0 || nx >= self.w as i16 || ny >= self.h as i16 {
                    continue;
                }
                let (nx, ny) = (nx as u8, ny as u8);
                if !self.free(CellPos(nx, ny)) || prev.contains_key(&(nx, ny)) {
                    continue;
                }
                prev.insert((nx, ny), (x, y));
                if near(nx, ny) {
                    let mut cur = (nx, ny);
                    while let Some(&p) = prev.get(&cur) {
                        if p == (from.0, from.1) {
                            return Some(CellPos(cur.0, cur.1));
                        }
                        cur = p;
                    }
                    return None;
                }
                queue.push_back(CellPos(nx, ny));
            }
        }
        None
    }

    // ---- M2b: resources, paired stats, next-turn effects ----

    /// Declare what a program can produce (1-2 types) and at what rate.
    pub fn set_maker(&mut self, prog: EntityId, makes: Vec<Res>, rate: u8) -> Result<(), SimError> {
        if !self.progs.contains_key(&prog.0) {
            return Err(SimError::new("no_prog", "live program", "missing"));
        }
        self.makers.insert(prog.0, (makes, rate.max(1)));
        Ok(())
    }

    /// Produce `base` units of `res` into the owner's stock.
    /// Multiplied by rate only if the program's type list includes it.
    pub fn produce(&mut self, prog: EntityId, res: Res, base: u32) -> Result<u32, SimError> {
        let p =
            self.progs
                .get(&prog.0)
                .ok_or(SimError::new("no_prog", "live program", "missing"))?;
        let mult = match self.makers.get(&prog.0) {
            Some((makes, rate)) if makes.contains(&res) => *rate as u32,
            _ => 1,
        };
        let amount = base * mult;
        *self
            .stocks
            .entry(p.owner.0)
            .or_default()
            .entry(res)
            .or_insert(0) += amount;
        Ok(amount)
    }

    pub fn stock(&self, owner: EntityId, res: Res) -> u32 {
        self.stocks
            .get(&owner.0)
            .and_then(|m| m.get(&res))
            .copied()
            .unwrap_or(0)
    }

    /// Public consume: spend `n` units of `res` from the owner's stock,
    /// refusing (and moving nothing) when short.
    pub fn consume(&mut self, owner: EntityId, res: Res, n: u32) -> Result<(), SimError> {
        self.spend(owner, res, n)
    }

    /// Add `n` units of `res` to the owner's stock (e.g. manual coding).
    pub fn add_stock(&mut self, owner: EntityId, res: Res, n: u32) -> u32 {
        let e = self
            .stocks
            .entry(owner.0)
            .or_default()
            .entry(res)
            .or_insert(0);
        *e += n;
        *e
    }

    /// Passive production (design/10: "creatures auto-produce faster + free
    /// player"): every live maker program the owner controls yields its
    /// declared rate into the owner's stock. Returns the per-resource
    /// totals produced this beat (sorted for determinism).
    pub fn tick_makers(&mut self, owner: EntityId) -> Vec<(Res, u32)> {
        // Snapshot the makers first so the stock mutation below is a
        // separate borrow (programs can be added/removed between acts).
        let makers: Vec<(Vec<Res>, u8)> = self
            .progs
            .values()
            .filter(|p| p.owner == owner)
            .filter_map(|p| self.makers.get(&p.id.0).cloned())
            .collect();
        let mut made: BTreeMap<Res, u32> = BTreeMap::new();
        for (makes, rate) in makers {
            for r in makes {
                *made.entry(r).or_insert(0) += rate as u32;
            }
        }
        for (r, n) in &made {
            self.add_stock(owner, *r, *n);
        }
        made.into_iter().collect()
    }

    /// Producer info for one program: the resources it makes and its rate,
    /// if it is a maker (client rendering: mark producer cells).
    pub fn maker_info(&self, prog: EntityId) -> Option<(Vec<Res>, u8)> {
        self.makers.get(&prog.0).cloned()
    }

    /// Program occupying a cell, if any (client/card targeting).
    pub fn prog_at(&self, x: u8, y: u8) -> Option<EntityId> {
        self.cell_get(x, y).map(|s| s.prog)
    }

    /// Farm card: one production tick for the owner's makers, immediately.
    /// Returns the per-resource totals (sorted).
    pub fn farm_once(&mut self, owner: EntityId) -> Vec<(Res, u32)> {
        let makers: Vec<(EntityId, Vec<Res>)> = self
            .progs
            .values()
            .filter(|p| p.owner == owner)
            .filter_map(|p| self.makers.get(&p.id.0).map(|(m, _)| (p.id, m.clone())))
            .collect();
        let mut out: std::collections::BTreeMap<Res, u32> = Default::default();
        for (id, makes) in makers {
            for r in makes {
                if let Ok(n) = self.produce(id, r, 1) {
                    *out.entry(r).or_insert(0) += n;
                }
            }
        }
        out.into_iter().collect()
    }

    /// First free arena cell in reading order (client convenience: deploy a
    /// producer without making the player pick a cell).
    pub fn free_cell(&self) -> Option<CellPos> {
        (0..self.h).find_map(|y| (0..self.w).map(|x| CellPos(x, y)).find(|c| self.free(*c)))
    }

    /// First free cell with no orthogonally adjacent program owned by
    /// someone else — a safer auto-pick for deploying a producer, since a
    /// hostile warden strikes an adjacent program on the same beat. Falls
    /// back to any free cell when the arena offers no breathing room.
    pub fn free_cell_for(&self, owner: EntityId) -> Option<CellPos> {
        let adjacent = |x: u8, y: u8| {
            let touches = |p: &Program| {
                let near =
                    |c: CellPos| (x as i16 - c.0 as i16).abs() + (y as i16 - c.1 as i16).abs() == 1;
                near(p.head) || p.tail.iter().any(|t| near(*t))
            };
            self.progs
                .values()
                .filter(|p| p.owner != owner)
                .any(touches)
        };
        let safe = (0..self.h).find_map(|y| {
            (0..self.w)
                .map(|x| CellPos(x, y))
                .find(|c| self.free(*c) && !adjacent(c.0, c.1))
        });
        safe.or_else(|| self.free_cell())
    }

    /// Per-resource yield the owner's producers will deliver each act,
    /// with a count of producers per type (client summary line).
    pub fn producer_summary(&self, owner: EntityId) -> Vec<(Res, u32, u32)> {
        let mut out: BTreeMap<Res, (u32, u32)> = BTreeMap::new();
        for p in self.progs.values().filter(|p| p.owner == owner) {
            if let Some((makes, rate)) = self.makers.get(&p.id.0) {
                for r in makes {
                    let e = out.entry(*r).or_insert((0, 0));
                    e.0 += *rate as u32; // total yield per act
                    e.1 += 1; // producer count
                }
            }
        }
        out.into_iter().map(|(r, (y, c))| (r, y, c)).collect()
    }

    fn spend(&mut self, owner: EntityId, res: Res, n: u32) -> Result<(), SimError> {
        let e = self
            .stocks
            .entry(owner.0)
            .or_default()
            .entry(res)
            .or_insert(0);
        if *e < n {
            return Err(SimError::new("no_resource", "stocked units", "short"));
        }
        *e -= n;
        Ok(())
    }

    /// Card strike: base 1 segment + `bonus` segments, funded by consuming stock.
    /// `consume` spends N of one type for +N damage; `consume_all` spends the whole
    /// pile of another type for +pile damage. Shield absorbs first.
    pub fn card_strike(
        &mut self,
        owner: EntityId,
        target: CellPos,
        consume: Option<(Res, u32)>,
        consume_all: Option<Res>,
    ) -> Result<bool, SimError> {
        let mut dmg: u32 = 1;
        if let Some((res, n)) = consume {
            self.spend(owner, res, n)?;
            dmg += n;
        }
        if let Some(res) = consume_all {
            let pile = self.stock(owner, res);
            self.spend(owner, res, pile)?;
            dmg += pile;
        }
        self.deal_damage(target, dmg)
    }

    /// Card attack (design/03: paired stats + dice + resources). Baseline
    /// segment damage matches `card_strike`; the d6 only decides a crit
    /// (+1) so plain-strike TTK stays deterministic. Both sides' next-turn
    /// effects modify the roll and the outcome.
    pub fn card_attack(
        &mut self,
        owner: EntityId,
        attacker: EntityId,
        target: CellPos,
        consume: Option<(Res, u32)>,
        consume_all: Option<Res>,
    ) -> Result<AttackOutcome, SimError> {
        let mut out = AttackOutcome::default();
        // 1. Base damage from resources (spent up front; a refused spend
        //    leaves everything untouched).
        let mut dmg: u32 = 1;
        if let Some((res, n)) = consume {
            self.spend(owner, res, n)?;
            dmg += n;
        }
        if let Some(res) = consume_all {
            let pile = self.stock(owner, res);
            self.spend(owner, res, pile)?;
            dmg += pile;
        }
        // 2. Random-target: retarget to a random occupied cell.
        let target = if self.take_fx(attacker, |f| matches!(f, Fx::RandomTargetNext)) {
            let cells: Vec<CellPos> = self.cells.iter().map(|c| CellPos(c.x, c.y)).collect();
            if cells.is_empty() {
                return Err(SimError::new("no_target", "occupied cell", "empty"));
            }
            let i = (self.roll(cells.len() as u8) - 1) as usize;
            out.retargeted = true;
            cells[i]
        } else {
            target
        };
        let seg = self.cell_get(target.0, target.1).ok_or(SimError::new(
            "no_target",
            "program cell",
            "empty",
        ))?;
        let victim = seg.prog;
        // 3. Roll the attack die, honouring both sides' modifiers.
        let mut face = self.roll(6);
        if self.take_fx(attacker, |f| matches!(f, Fx::ExtraRollNext)) {
            face = face.max(self.roll(6));
        }
        if let Some(Fx::BanDie(b)) = self.fx_of(victim, |f| matches!(f, Fx::BanDie(_))) {
            if face == b {
                face = face.saturating_sub(1).max(1);
                self.take_fx(victim, |f| matches!(f, Fx::BanDie(_)));
            }
        }
        if let Some(Fx::RollMinus(m)) = self.fx_of(victim, |f| matches!(f, Fx::RollMinus(_))) {
            face = face.saturating_sub(m).max(1);
            self.take_fx(victim, |f| matches!(f, Fx::RollMinus(_)));
        }
        out.face = face;
        if face >= 5 {
            out.crit = true;
            dmg += 1;
        }
        // Downgrade: this attacker's next hit lands at half strength.
        if self.take_fx(attacker, |f| matches!(f, Fx::DowngradeNext)) {
            dmg = (dmg / 2).max(1);
        }
        // Decoy: the victim absorbs the hit entirely.
        if self.take_fx(victim, |f| matches!(f, Fx::DecoyNext)) {
            out.absorbed = true;
            out.damage = 0;
            return Ok(out);
        }
        // Reflect: the damage lands on the attacker's body instead (a
        // headshot would make reflect unconditionally lethal).
        if self.take_fx(victim, |f| matches!(f, Fx::ReflectNext)) {
            out.reflected = true;
            out.damage = dmg;
            let body = self
                .progs
                .get(&attacker.0)
                .and_then(|p| p.tail.back().copied().or(Some(p.head)));
            if let Some(b) = body {
                if self.deal_damage(b, dmg)? {
                    out.killed = true;
                }
            }
            return Ok(out);
        }
        out.damage = dmg;
        out.killed = self.deal_damage(target, dmg)?;
        // Splash: 1 damage to orthogonally adjacent program cells.
        if self.take_fx(attacker, |f| matches!(f, Fx::SplashNext)) {
            let neighbours = [
                (target.0.wrapping_sub(1), target.1),
                (target.0.wrapping_add(1), target.1),
                (target.0, target.1.wrapping_sub(1)),
                (target.0, target.1.wrapping_add(1)),
            ];
            for (x, y) in neighbours {
                if let Some(seg) = self.cell_get(x, y) {
                    let v = seg.prog;
                    let _ = self.deal_damage(CellPos(x, y), 1)?;
                    out.splashed.push(v);
                }
            }
        }
        Ok(out)
    }

    fn deal_damage(&mut self, target: CellPos, mut dmg: u32) -> Result<bool, SimError> {
        let seg = self.cell_get(target.0, target.1).ok_or(SimError::new(
            "no_target",
            "program cell",
            "empty",
        ))?;
        let victim = seg.prog;
        // Shield absorbs segment damage first.
        if let Some(fx) = self.effects.get_mut(&victim.0) {
            for f in fx.iter_mut() {
                if let Fx::ShieldNext(s) = f {
                    let absorb = (*s as u32).min(dmg);
                    *s -= absorb as u8;
                    dmg -= absorb;
                    break;
                }
            }
        }
        if seg.is_head {
            if dmg >= 1 {
                self.remove_program(victim);
                return Ok(true);
            }
            return Ok(false);
        }
        // Remove up to `dmg` tail segments nearest the head.
        let mut gone = Vec::new();
        if let Some(p) = self.progs.get_mut(&victim.0) {
            let mut removed = 0;
            while removed < dmg {
                if let Some(c) = p.tail.pop_front() {
                    gone.push(c);
                    removed += 1;
                } else {
                    break;
                }
            }
        }
        for c in gone {
            self.cell_drop(c.0, c.1);
        }
        Ok(false)
    }

    /// Distract: victim skips its next act. Freakout: DoT ticks on `end_turn`.
    pub fn apply_fx(&mut self, prog: EntityId, fx: Fx) -> Result<(), SimError> {
        if !self.progs.contains_key(&prog.0) {
            return Err(SimError::new("no_prog", "live program", "missing"));
        }
        self.effects.entry(prog.0).or_default().push(fx);
        Ok(())
    }

    pub fn is_skipped(&self, prog: EntityId) -> bool {
        self.effects
            .get(&prog.0)
            .map(|v| v.contains(&Fx::SkipNext))
            .unwrap_or(false)
    }

    /// End of a program's turn: DoT eats one tail segment (Stability poison),
    /// then all next-turn effects expire.
    pub fn end_turn(&mut self, prog: EntityId) {
        if let Some(fx) = self.effects.remove(&prog.0) {
            let dots: u32 = fx
                .iter()
                .filter_map(|f| match f {
                    Fx::DotNext(n) => Some(*n as u32),
                    _ => None,
                })
                .sum();
            for _ in 0..dots {
                if let Some(p) = self.progs.get_mut(&prog.0) {
                    if let Some(c) = p.tail.pop_back() {
                        self.cell_drop(c.0, c.1);
                    } else {
                        self.remove_program(prog);
                        break;
                    }
                }
            }
            // SkipNext/ShieldNext expire unconsumed: next-turn-only per 2024.
        }
    }

    /// Damage a paired stat directly (Sway/Distract/... pole). Floor 0.
    pub fn damage_stat(&mut self, prog: EntityId, stat: &str, n: u8) -> Result<(), SimError> {
        let s = self.stats.entry(prog.0).or_default();
        let v = match stat {
            "dedication" => &mut s.dedication,
            "focus" => &mut s.focus,
            "stability" => &mut s.stability,
            "courage" => &mut s.courage,
            "clarity" => &mut s.clarity,
            _ => return Err(SimError::new("no_stat", "known pole", "bad name")),
        };
        *v = v.saturating_sub(n);
        Ok(())
    }

    /// Patch card: heal the lowest stat by `n` (cap 100... stats cap at 255 u8; soft cap 20).
    pub fn heal_lowest(&mut self, prog: EntityId, n: u8) -> Result<&'static str, SimError> {
        let s = self.stats.entry(prog.0).or_default();
        let name = s.lowest();
        let v = match name {
            "dedication" => &mut s.dedication,
            "focus" => &mut s.focus,
            "stability" => &mut s.stability,
            "courage" => &mut s.courage,
            _ => &mut s.clarity,
        };
        *v = (*v + n).min(20);
        Ok(name)
    }

    // ---- M2 card table (design/03 closed effect set) ----

    /// True when the program carries an effect matching `pred`.
    pub fn has_fx(&self, prog: EntityId, pred: impl Fn(&Fx) -> bool) -> bool {
        self.effects
            .get(&prog.0)
            .is_some_and(|v| v.iter().any(&pred))
    }

    /// Read the first matching effect (for its payload).
    pub fn fx_of(&self, prog: EntityId, pred: impl Fn(&Fx) -> bool) -> Option<Fx> {
        self.effects
            .get(&prog.0)
            .and_then(|v| v.iter().find(|f| pred(f)).copied())
    }

    /// Consume (remove) one matching effect; true when one was present.
    fn take_fx(&mut self, prog: EntityId, pred: impl Fn(&Fx) -> bool) -> bool {
        if let Some(v) = self.effects.get_mut(&prog.0) {
            if let Some(i) = v.iter().position(&pred) {
                v.remove(i);
                return true;
            }
        }
        false
    }

    /// Load card: append one tail segment adjacent to the program's tail
    /// end (or head when it has no tail). `blocked` when no free cell
    /// touches the body.
    pub fn grow(&mut self, prog: EntityId) -> Result<CellPos, SimError> {
        let p =
            self.progs
                .get(&prog.0)
                .ok_or(SimError::new("no_prog", "live program", "missing"))?;
        let anchor = p.tail.back().copied().unwrap_or(p.head);
        let cand = [
            CellPos(anchor.0.wrapping_sub(1), anchor.1),
            CellPos(anchor.0.wrapping_add(1), anchor.1),
            CellPos(anchor.0, anchor.1.wrapping_sub(1)),
            CellPos(anchor.0, anchor.1.wrapping_add(1)),
        ]
        .into_iter()
        .find(|c| self.free(*c));
        let at = cand.ok_or(SimError::new("blocked", "free adjacent cell", "cramped"))?;
        self.cell_put(at.0, at.1, prog, false);
        self.progs.get_mut(&prog.0).unwrap().tail.push_back(at);
        Ok(at)
    }

    /// Upgrade card: raise the program's growth cap by `n`.
    pub fn upgrade_max(&mut self, prog: EntityId, n: u8) -> Result<u8, SimError> {
        let p = self.progs.get_mut(&prog.0).ok_or(SimError::new(
            "no_prog",
            "live program",
            "missing",
        ))?;
        p.max_len = p.max_len.saturating_add(n);
        Ok(p.max_len)
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
        h.load(EntityId(10), EntityId(1), CellPos(2, 2), 3, 1)
            .unwrap();
        h.step(EntityId(10), CellPos(3, 2)).unwrap();
        h.step(EntityId(10), CellPos(4, 2)).unwrap();
        assert_eq!(h.progs[&10].len(), 3);
        // Strike tail twice then head: 3 acts total.
        assert!(!h.strike(CellPos(2, 2)).unwrap());
        assert!(!h.strike(CellPos(3, 2)).unwrap());
        assert!(h.strike(CellPos(4, 2)).unwrap());
        assert!(!h.progs.contains_key(&10));
    }

    #[test]
    fn cpu_overload_blocks_load_and_growth() {
        let mut h = Host::new(HostId(2), 12, 8, 3);
        h.load(EntityId(20), EntityId(1), CellPos(0, 0), 2, 1)
            .unwrap(); // load 2
        assert!(h
            .load(EntityId(21), EntityId(1), CellPos(5, 5), 2, 1)
            .is_err()); // 2+2>3
                        // Growth while at cap is fine (still under), but overload blocks step.
        let mut h2 = Host::new(HostId(3), 12, 8, 100);
        h2.load(EntityId(30), EntityId(1), CellPos(0, 0), 4, 1)
            .unwrap();
        h2.step(EntityId(30), CellPos(1, 0)).unwrap();
        assert_eq!(h2.progs[&30].len(), 2);
    }

    #[test]
    fn takeover_flips_owner() {
        let mut h = host();
        h.load(EntityId(40), EntityId(1), CellPos(0, 0), 2, 1)
            .unwrap();
        h.load(EntityId(41), EntityId(2), CellPos(0, 2), 2, 1)
            .unwrap();
        h.step(EntityId(40), CellPos(0, 1)).unwrap(); // adjacent to victim head
        h.takeover(EntityId(40), CellPos(0, 2)).unwrap();
        assert_eq!(h.owner_of(EntityId(41)), Some(EntityId(1)));
        // Takeover on tail rejected.
        assert!(h.takeover(EntityId(40), CellPos(0, 0)).is_err());
    }

    #[test]
    fn clone_inherits_owner_build_and_maker() {
        let mut h = host();
        h.load(EntityId(40), EntityId(1), CellPos(0, 0), 4, 2)
            .unwrap();
        h.set_maker(EntityId(40), vec![Res::Algo], 2).unwrap();
        h.clone_to(EntityId(40), EntityId(44), CellPos(5, 5))
            .unwrap();
        assert_eq!(h.owner_of(EntityId(44)), Some(EntityId(1)));
        assert_eq!(h.progs[&44].max_len, 4);
        assert_eq!(h.produce(EntityId(44), Res::Algo, 1).unwrap(), 2); // maker copied
        assert!(h
            .clone_to(EntityId(40), EntityId(44), CellPos(6, 6))
            .is_err()); // dup id
        assert!(h
            .clone_to(EntityId(99), EntityId(45), CellPos(6, 6))
            .is_err()); // missing
        assert!(h
            .clone_to(EntityId(40), EntityId(45), CellPos(5, 5))
            .is_err()); // busy
    }

    #[test]
    fn yaml_cards_all_implemented() {
        use crate::data::parse_cards;
        let t = parse_cards(include_str!("../../data/cards.yaml")).unwrap();
        let have: std::collections::BTreeSet<&str> =
            Host::implemented_cards().into_iter().collect();
        let mut want = std::collections::BTreeSet::new();
        for c in t.negative.iter().chain(&t.positive).chain(&t.manipulation) {
            want.insert(c.id.as_str());
        }
        assert_eq!(have, want);
        // design/09 M2: the closed table is 8 negative / 8 positive /
        // 4 manipulation.
        assert_eq!(t.negative.len(), 8, "negative cards");
        assert_eq!(t.positive.len(), 8, "positive cards");
        assert_eq!(t.manipulation.len(), 4, "manipulation cards");
        // Every negative card names an effect this build resolves.
        for c in &t.negative {
            assert!(!c.effect.is_empty(), "negative {} has no effect", c.id);
        }
    }

    /// A program grown to `len` cells starting at (x,y), moving east.
    fn grown(h: &mut Host, id: EntityId, owner: EntityId, x: u8, y: u8, len: u8) -> CellPos {
        h.load(id, owner, CellPos(x, y), len, 1).unwrap();
        for i in 1..len {
            h.step(id, CellPos(x + i, y)).unwrap();
        }
        CellPos(x + len - 1, y)
    }

    /// A body cell to strike (rear tail) so attacks measure damage instead
    /// of taking a headshot; falls back to the head for single-cell programs.
    fn body(h: &Host, id: u64) -> CellPos {
        let p = &h.progs[&id];
        p.tail.back().copied().unwrap_or(p.head)
    }

    /// Standard arena: attacker (10) row 0, victim (20) row 3 from x=4,
    /// neighbour (30) at (4,4) beside the victim's head.
    fn arena() -> Host {
        let mut h = host();
        grown(&mut h, EntityId(10), EntityId(1), 0, 0, 11);
        grown(&mut h, EntityId(20), EntityId(2), 1, 3, 11);
        grown(&mut h, EntityId(30), EntityId(2), 10, 4, 2);
        h
    }

    #[test]
    fn card_attack_crit_maps_to_face_and_keeps_baseline_ttk() {
        let mut h = arena();
        // Base damage 1; a face of 5-6 crits for +1. Relationship is exact
        // and seed-independent.
        for _ in 0..4 {
            let out = h
                .card_attack(EntityId(2), EntityId(10), body(&h, 20), None, None)
                .unwrap();
            assert_eq!(out.crit, out.face >= 5, "face {}", out.face);
            assert_eq!(out.damage, if out.crit { 2 } else { 1 });
            assert!((1..=6).contains(&out.face));
        }
    }

    #[test]
    fn roll_minus_and_ban_die_weaken_the_attacker() {
        let mut h = arena();
        // RollMinus(3): the attacker's face can never exceed 3, so no crit.
        h.apply_fx(EntityId(20), Fx::RollMinus(3)).unwrap();
        let out = h
            .card_attack(EntityId(2), EntityId(10), body(&h, 20), None, None)
            .unwrap();
        assert!(out.face <= 3, "face {} not reduced", out.face);
        assert!(!out.crit);
        assert!(!h.has_fx(EntityId(20), |f| matches!(f, Fx::RollMinus(_))));
        // BanDie(6): a six is demoted, so the face is never 6.
        h.apply_fx(EntityId(20), Fx::BanDie(6)).unwrap();
        let out = h
            .card_attack(EntityId(2), EntityId(10), body(&h, 20), None, None)
            .unwrap();
        assert_ne!(out.face, 6, "banned face still rolled");
    }

    #[test]
    fn extra_roll_keeps_the_higher_face() {
        let mut h = arena();
        // Over many extra-roll attacks at least one crit must appear (a max
        // of two d6 beats a single die); deterministic per seed.
        let mut crits = 0;
        for _ in 0..12 {
            if !h.progs.contains_key(&20) {
                grown(&mut h, EntityId(20), EntityId(2), 1, 3, 11);
            }
            h.apply_fx(EntityId(10), Fx::ExtraRollNext).unwrap();
            let out = h
                .card_attack(EntityId(2), EntityId(10), body(&h, 20), None, None)
                .unwrap();
            if out.crit {
                crits += 1;
            }
        }
        assert!(crits > 0, "extra roll never produced a crit");
    }

    #[test]
    fn decoy_absorbs_reflect_returns_splash_hits_neighbours() {
        let mut h = arena();
        // Decoy: the hit is absorbed, victim untouched, effect consumed.
        h.apply_fx(EntityId(20), Fx::DecoyNext).unwrap();
        let before = h.progs[&20].len();
        let out = h
            .card_attack(EntityId(2), EntityId(10), body(&h, 20), None, None)
            .unwrap();
        assert!(out.absorbed && out.damage == 0);
        assert_eq!(h.progs[&20].len(), before);
        // Reflect: damage lands on the attacker's body instead.
        h.apply_fx(EntityId(20), Fx::ReflectNext).unwrap();
        let atk_before = h.progs[&10].len();
        let out = h
            .card_attack(EntityId(2), EntityId(10), body(&h, 20), None, None)
            .unwrap();
        assert!(out.reflected);
        assert!(h.progs[&10].len() < atk_before, "attacker took the hit");
        // Splash: the neighbour beside the victim head takes 1.
        h.apply_fx(EntityId(10), Fx::SplashNext).unwrap();
        let nb_before = h.progs[&30].len();
        let out = h
            .card_attack(EntityId(2), EntityId(10), h.progs[&20].head, None, None)
            .unwrap();
        assert!(out.splashed.contains(&EntityId(30)), "{:?}", out.splashed);
        // Splash lands on a cell, so it can even headshot: 30 either shrank
        // or died, but it was definitely hit.
        let nb_after = h.progs.get(&30).map(|p| p.len()).unwrap_or(0);
        assert!(
            nb_after < nb_before,
            "neighbour splashed: {nb_after} vs {nb_before}"
        );
    }

    #[test]
    fn downgrade_halves_and_random_target_retargets() {
        let mut h = arena();
        // Downgrade with resources: 1 + 3 = 4 base (+crit), halved.
        h.add_stock(EntityId(2), Res::Loop, 3);
        h.apply_fx(EntityId(10), Fx::DowngradeNext).unwrap();
        let out = h
            .card_attack(
                EntityId(2),
                EntityId(10),
                body(&h, 20),
                Some((Res::Loop, 3)),
                None,
            )
            .unwrap();
        assert!(out.damage <= 2, "not halved: {}", out.damage);
        // Random target: the attack is retargeted to some occupied cell.
        h.apply_fx(EntityId(10), Fx::RandomTargetNext).unwrap();
        let out = h
            .card_attack(EntityId(2), EntityId(10), body(&h, 20), None, None)
            .unwrap();
        assert!(out.retargeted);
        assert!(!h.has_fx(EntityId(10), |f| matches!(f, Fx::RandomTargetNext)));
    }

    #[test]
    fn grow_and_upgrade_reshape_a_program() {
        let mut h = host();
        grown(&mut h, EntityId(10), EntityId(1), 0, 0, 2);
        assert_eq!(h.progs[&10].len(), 2);
        // Grow extends the body at the rear with a free neighbour.
        let at = h.grow(EntityId(10)).unwrap();
        assert_eq!(h.progs[&10].len(), 3);
        assert!(h.cell_get(at.0, at.1).is_some());
        // Upgrade raises the growth cap, so a further step keeps the length
        // instead of sliding back down.
        let cap = h.upgrade_max(EntityId(10), 1).unwrap();
        assert_eq!(cap, 3);
        h.step(EntityId(10), CellPos(2, 0)).unwrap();
        assert_eq!(h.progs[&10].len(), 3, "kept length under the new cap");
        // grow/upgrade on a missing program refuse.
        assert!(h.grow(EntityId(99)).is_err());
        assert!(h.upgrade_max(EntityId(99), 1).is_err());
    }

    #[test]
    fn card_attack_ttk_stays_bounded() {
        // design/03: the table is "small enough to balance TTK 3-4 hits".
        // A plain card attack (no resources) takes 2-3 hits on a len-3
        // body; a funded spike clears it in one. Deterministic per seed.
        let mut h = host();
        grown(&mut h, EntityId(10), EntityId(1), 0, 0, 3);
        grown(&mut h, EntityId(20), EntityId(2), 4, 3, 3);
        let mut hits = 0;
        while h.progs.contains_key(&20) {
            h.card_attack(EntityId(2), EntityId(10), body(&h, 20), None, None)
                .unwrap();
            hits += 1;
            assert!(hits <= 4, "plain card attack TTK {hits} exceeded 4");
        }
        assert!((2..=4).contains(&hits), "plain TTK {hits} outside 2-4");
        // Funded spike (consume 3) finishes a len-3 body in a single hit.
        grown(&mut h, EntityId(21), EntityId(2), 4, 6, 3);
        h.add_stock(EntityId(2), Res::Loop, 3);
        let out = h
            .card_attack(
                EntityId(2),
                EntityId(10),
                body(&h, 21),
                Some((Res::Loop, 3)),
                None,
            )
            .unwrap();
        assert!(out.damage >= 4, "funded hit only {} ", out.damage);
        // 4 damage strips both tail segments; the head alone remains, so
        // the body is reduced to a single cell rather than destroyed.
        assert_eq!(h.progs[&21].len(), 1, "funded spike stripped the body");
    }

    #[test]
    fn rolls_are_deterministic_in_range_and_seeded_per_host() {
        let mut a = host();
        let mut b = host();
        // Same seed, same sequence.
        let sa: Vec<u8> = (0..8).map(|_| a.roll(6)).collect();
        let sb: Vec<u8> = (0..8).map(|_| b.roll(6)).collect();
        assert_eq!(sa, sb);
        assert!(sa.iter().all(|f| (1..=6).contains(f)), "faces: {sa:?}");
        // Not constant (a stuck RNG would be a bug).
        assert!(sa.iter().collect::<std::collections::BTreeSet<_>>().len() > 1);
        // Distinct hosts roll differently.
        let mut c = Host::new(HostId(7), 12, 8, 100);
        let sc: Vec<u8> = (0..8).map(|_| c.roll(6)).collect();
        assert_ne!(sa, sc);
        // sides is clamped to at least 1.
        assert_eq!(a.roll(0), 1);
    }

    #[test]
    fn free_cell_for_avoids_hostile_neighbours() {
        let mut h = host();
        // Hostile program at (0,0): the safe pick skips its neighbours.
        h.load(EntityId(70), EntityId(9), CellPos(0, 0), 2, 1)
            .unwrap();
        let c = h.free_cell_for(EntityId(1)).unwrap();
        let adjacent = |c: CellPos| (c.0 as i16 + c.1 as i16) == 1;
        assert!(!adjacent(c), "picked a cell next to the hostile: {c:?}");
        // It is still a genuinely free cell.
        assert!(h.free(c));
        // With the arena crowded by one owner, the fallback still returns.
        let mut h2 = host();
        h2.load(EntityId(70), EntityId(9), CellPos(0, 0), 2, 1)
            .unwrap();
        assert!(h2.free_cell_for(EntityId(1)).is_some());
    }

    #[test]
    fn producer_summary_counts_yield_and_makers() {
        let mut h = host();
        h.load(EntityId(50), EntityId(1), CellPos(0, 0), 2, 1)
            .unwrap();
        h.set_maker(EntityId(50), vec![Res::Loop], 3).unwrap();
        h.load(EntityId(51), EntityId(1), CellPos(1, 0), 2, 1)
            .unwrap();
        h.set_maker(EntityId(51), vec![Res::Loop, Res::Data], 2)
            .unwrap();
        // A non-maker program contributes nothing.
        h.load(EntityId(52), EntityId(1), CellPos(2, 0), 2, 1)
            .unwrap();
        // Another owner's maker is not counted.
        h.load(EntityId(53), EntityId(9), CellPos(3, 0), 2, 1)
            .unwrap();
        h.set_maker(EntityId(53), vec![Res::Algo], 5).unwrap();

        let s = h.producer_summary(EntityId(1));
        assert_eq!(s, vec![(Res::Loop, 5, 2), (Res::Data, 2, 1)]);
        assert_eq!(h.producer_summary(EntityId(9)), vec![(Res::Algo, 5, 1)]);
        assert_eq!(h.maker_info(EntityId(50)), Some((vec![Res::Loop], 3)));
        assert_eq!(h.maker_info(EntityId(52)), None);
    }

    #[test]
    fn produce_pays_rate_only_on_match() {
        let mut h = host();
        h.load(EntityId(50), EntityId(1), CellPos(0, 0), 2, 1)
            .unwrap();
        h.set_maker(EntityId(50), vec![Res::Loop], 3).unwrap();
        assert_eq!(h.produce(EntityId(50), Res::Loop, 2).unwrap(), 6);
        assert_eq!(h.produce(EntityId(50), Res::Data, 2).unwrap(), 2); // off-type: x1
        assert_eq!(h.stock(EntityId(1), Res::Loop), 6);
    }

    #[test]
    fn consume_and_consume_all_scale_damage() {
        let mut h = host();
        // Victim len-3.
        h.load(EntityId(60), EntityId(2), CellPos(2, 2), 3, 1)
            .unwrap();
        h.step(EntityId(60), CellPos(3, 2)).unwrap();
        h.step(EntityId(60), CellPos(4, 2)).unwrap();
        // Attacker funds 2 Loop: base 1 + 2 = 3 segs = full clear via tail cell.
        h.load(EntityId(61), EntityId(1), CellPos(8, 7), 2, 1)
            .unwrap();
        h.set_maker(EntityId(61), vec![Res::Loop], 2).unwrap();
        h.produce(EntityId(61), Res::Loop, 1).unwrap(); // stock 2
        assert!(h
            .card_strike(EntityId(1), CellPos(4, 2), Some((Res::Loop, 2)), None)
            .unwrap());
        assert!(!h.progs.contains_key(&60)); // headshot kills regardless of dmg
                                             // Unfunded consume rejected.
        h.load(EntityId(62), EntityId(2), CellPos(6, 6), 2, 1)
            .unwrap();
        assert!(h
            .card_strike(EntityId(1), CellPos(6, 6), Some((Res::Loop, 9)), None)
            .is_err());
        // Funded tail strike scales: len-1 victim + 1 funded dmg clears tail, head survives.
        h.step(EntityId(62), CellPos(7, 6)).unwrap(); // len 2
        h.produce(EntityId(61), Res::Loop, 1).unwrap(); // stock 2 again
        assert!(!h
            .card_strike(EntityId(1), CellPos(6, 6), Some((Res::Loop, 1)), None)
            .unwrap());
        assert_eq!(h.progs[&62].len(), 1);
    }

    #[test]
    fn skip_shield_dot_expire_next_turn() {
        let mut h = host();
        h.load(EntityId(70), EntityId(1), CellPos(0, 0), 3, 1)
            .unwrap();
        h.step(EntityId(70), CellPos(1, 0)).unwrap();
        h.step(EntityId(70), CellPos(2, 0)).unwrap(); // len 3
        h.apply_fx(EntityId(70), Fx::SkipNext).unwrap();
        assert!(h.is_skipped(EntityId(70)));
        h.apply_fx(EntityId(70), Fx::DotNext(1)).unwrap();
        h.end_turn(EntityId(70)); // DoT eats 1 tail, skip expires
        assert_eq!(h.progs[&70].len(), 2);
        assert!(!h.is_skipped(EntityId(70)));
    }

    #[test]
    fn stats_damage_and_heal_lowest() {
        let mut h = host();
        h.load(EntityId(80), EntityId(1), CellPos(0, 0), 2, 1)
            .unwrap();
        h.damage_stat(EntityId(80), "focus", 4).unwrap();
        assert_eq!(h.stats[&80].focus, 6);
        let name = h.heal_lowest(EntityId(80), 2).unwrap();
        assert_eq!(name, "focus");
        assert_eq!(h.stats[&80].focus, 8);
        assert!(h.damage_stat(EntityId(80), "bogus", 1).is_err());
    }

    #[test]
    fn damage_soak_floors_at_zero_and_heal_caps() {
        // Full u8 sweep on every pole: saturating floor, no panic.
        for stat in ["dedication", "focus", "stability", "courage", "clarity"] {
            for n in [0u8, 1, 9, 10, 11, 100, 255] {
                let mut h = host();
                h.load(EntityId(80), EntityId(1), CellPos(0, 0), 2, 1)
                    .unwrap();
                h.damage_stat(EntityId(80), stat, n).unwrap();
                let got = match stat {
                    "dedication" => h.stats[&80].dedication,
                    "focus" => h.stats[&80].focus,
                    "stability" => h.stats[&80].stability,
                    "courage" => h.stats[&80].courage,
                    _ => h.stats[&80].clarity,
                };
                assert_eq!(got, 10u8.saturating_sub(n), "{stat} -{n}");
            }
        }
        // Heal from the floor caps at the 20 soft cap, never wraps.
        let mut h = host();
        h.load(EntityId(81), EntityId(1), CellPos(0, 0), 2, 1)
            .unwrap();
        h.damage_stat(EntityId(81), "focus", 255).unwrap();
        h.heal_lowest(EntityId(81), 255).unwrap();
        assert_eq!(h.stats[&81].focus, 20);
    }

    #[test]
    fn exploit_once_then_burned_patched_blocks() {
        let mut h = host();
        h.add_vuln("cve-2006-0001");
        h.add_vuln("cve-2006-0002");
        h.load(EntityId(90), EntityId(1), CellPos(0, 0), 2, 1)
            .unwrap();
        h.set_monitor(true);
        h.exploit(EntityId(90), "cve-2006-0001").unwrap();
        assert!(h.breached);
        assert!(h.flagged); // monitor armed
        assert!(h.exploit(EntityId(90), "cve-2006-0001").is_err()); // burned
        assert!(h.exploit(EntityId(90), "cve-2006-nope").is_err()); // unknown
        h.patch("cve-2006-0002").unwrap();
        assert!(h.exploit(EntityId(90), "cve-2006-0002").is_err()); // patched
    }

    #[test]
    fn trace_runs_slower_scrambled() {
        let mut plain = host();
        plain
            .load(EntityId(91), EntityId(2), CellPos(0, 0), 2, 1)
            .unwrap();
        plain.start_trace(EntityId(91), 6).unwrap();
        let mut ticks = 0;
        while !plain.tick_trace(false) {
            ticks += 1;
        }
        assert_eq!(ticks, 2); // 2+2+2: third tick completes

        let mut slow = host();
        slow.load(EntityId(92), EntityId(2), CellPos(0, 0), 2, 1)
            .unwrap();
        slow.start_trace(EntityId(92), 6).unwrap();
        let mut ticks = 0;
        while !slow.tick_trace(true) {
            ticks += 1;
        }
        assert_eq!(ticks, 5); // scrambler doubles the window
        assert!(slow.start_trace(EntityId(99), 6).is_err()); // no such target
    }

    #[test]
    fn installed_firewall_slows_traces() {
        let mut h = host();
        h.load(EntityId(91), EntityId(2), CellPos(0, 0), 2, 1)
            .unwrap();
        assert!(!h.firewall());
        h.set_firewall(true);
        assert!(h.firewall());
        h.start_trace(EntityId(91), 6).unwrap();
        // Filtered: +1/tick even unscrambled, like a scrambler.
        let mut ticks = 0;
        while !h.tick_trace(false) {
            ticks += 1;
        }
        assert_eq!(ticks, 5);
        h.set_firewall(false);
        assert!(!h.firewall());
    }

    #[test]
    fn quiet_boxes_dont_retaliate() {
        // No owner, monitor armed: intruders go unanswered.
        let mut h = host();
        h.set_monitor(true);
        h.load(EntityId(10), EntityId(1), CellPos(2, 2), 3, 1)
            .unwrap();
        assert_eq!(h.retaliate(), Retaliation::default());
        assert!(h.progs.values().all(|p| p.owner == EntityId(1)));
        // Owned but unmonitored: still quiet (stealth on cold boxes).
        h.set_owner(Some(EntityId(7)));
        h.set_monitor(false);
        assert_eq!(h.retaliate(), Retaliation::default());
    }

    #[test]
    fn defended_host_spawns_warden_and_traces() {
        let mut h = host();
        h.set_owner(Some(EntityId(7)));
        h.set_monitor(true);
        h.load(EntityId(10), EntityId(1), CellPos(5, 5), 3, 1)
            .unwrap();
        let r = h.retaliate();
        assert!(r.warden && r.traced);
        assert!(!r.killed && !r.burned);
        // Warden stands guard under defender colors; trace is running.
        assert!(h.progs.values().any(|p| p.owner == EntityId(7)));
        // Defender-owned programs are never targets.
        h.load(EntityId(11), EntityId(7), CellPos(8, 7), 2, 1)
            .unwrap();
        let r = h.retaliate();
        assert!(!r.traced);
    }

    #[test]
    fn warden_hunts_to_kill_and_trace_burns() {
        let mut h = host();
        h.set_owner(Some(EntityId(7)));
        h.set_monitor(true);
        // Near intruder (killed fast) plus far intruder (burned by trace).
        h.load(EntityId(10), EntityId(1), CellPos(0, 0), 1, 1)
            .unwrap();
        h.load(EntityId(11), EntityId(1), CellPos(11, 7), 1, 1)
            .unwrap();
        let (mut saw_trace, mut saw_kill, mut saw_burn, mut beats) = (false, false, false, 0);
        while h.progs.values().any(|p| p.owner == EntityId(1)) && beats < 60 {
            let r = h.retaliate();
            saw_trace = saw_trace || r.traced;
            saw_kill = saw_kill || r.killed;
            saw_burn = saw_burn || r.burned;
            beats += 1;
        }
        assert!(saw_trace && saw_kill && saw_burn, "beats={beats}");
        assert!(
            !h.progs.values().any(|p| p.owner == EntityId(1)),
            "intruders cleared in {beats} beats"
        );
        assert!(beats < 60);
        // Warden stands guard afterward.
        assert!(h.progs.values().any(|p| p.owner == EntityId(7)));
    }

    #[test]
    fn logs_append_and_scrub_needs_presence() {
        let mut h = host();
        h.log(EntityId(1), 100, HostId(9));
        h.log(EntityId(2), 101, HostId(9));
        // No program on host yet: scrub refused.
        assert!(h.scrub_log(EntityId(1)).is_err());
        h.load(EntityId(93), EntityId(1), CellPos(0, 0), 2, 1)
            .unwrap();
        assert_eq!(h.scrub_log(EntityId(1)).unwrap(), 1);
        assert_eq!(h.logs.len(), 1); // other's entry survives
    }
}

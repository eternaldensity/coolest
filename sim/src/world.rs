use crate::civic::{cop_duty, role_table, Role};
use crate::crime::Standing;
use crate::deck::{CellPos, Host};
use crate::error::SimError;
use crate::ids::{EntityId, HostId};
use crate::ledger::Ledger;
use crate::net::NetGraph;
use crate::pos::Pos;
use crate::power::PowerGrid;
use crate::scheduler::Scheduler;
use crate::shop::Shop;
use crate::street::{gen_section, gen_section_weighted, Ports, Section};
use crate::veil::Veil;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, VecDeque};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Dir {
    N,
    S,
    E,
    W,
}

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
    /// Move pocket cash into the bank ledger (logged mint).
    Deposit(i64),
    /// Move bank money into pocket cash (logged burn).
    Withdraw(i64),
    /// Sleep in the rented room until 06:00. No needs cost beyond this
    /// act itself; rent still accrues past midnight.
    Sleep,
    /// Sabotage a wired link in the world graph (design/04). Routed
    /// through the server like any act: logged for replay, refused
    /// with the link's reason when there is nothing to damage.
    DamageWire(HostId, HostId),
    /// Load a killer program at an arena cell of world host 1, then
    /// run warden retaliation in the same act (design/03).
    DeckLoad {
        x: u8,
        y: u8,
    },
    /// Strike an arena cell of world host 1, then run warden
    /// retaliation in the same act (design/03).
    DeckStrike {
        x: u8,
        y: u8,
    },
    /// Jack into the veil lobby with the given mindpower (design/05).
    VeilEnter {
        mp: u8,
    },
    /// Move between veil areas along exit links.
    VeilMove {
        area: u64,
    },
    /// Seize the demo host through an active veil token.
    VeilSeize,
    /// Clean exit from the veil (light drain).
    VeilEject,
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

/// Play-session funnel (design/12 section 3): first-tick timestamps for
/// started → first job → first hack → first trace → survived. Hack/trace
/// are client-reported via `mark_hack`/`mark_traced` (a Deck program kill,
/// takeover, or exploit success; an inbound trace on the player's programs).
/// Survived = still present 100 ticks after the first trace.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Funnel {
    pub protagonist: Option<EntityId>,
    pub started: Option<u64>,
    pub job: Option<u64>,
    pub hack: Option<u64>,
    pub traced: Option<u64>,
    pub survived: Option<u64>,
}

impl Funnel {
    fn claim(&mut self, actor: EntityId) {
        if self.protagonist.is_none() {
            self.protagonist = Some(actor);
        }
    }

    fn for_protagonist(&self, actor: EntityId) -> bool {
        self.protagonist.is_none_or(|p| p == actor)
    }
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
    /// Last game day rent was charged. Default 0 for old saves.
    #[serde(default)]
    last_rent_day: u64,
}

/// M1 jobs: (payout_cents, mood_cost, food_cost), tiered ladder in
/// data/jobs.yaml. Defaults match that file; `set_jobs` overrides from
/// the file at runtime (design/11 rule 4).
const DEFAULT_JOBS: [(i64, u8, u8); 5] = [
    (60, 5, 2),
    (120, 9, 4),
    (200, 13, 6),
    (320, 18, 9),
    (500, 24, 13),
];
const WATCH_COST: i64 = 30;
/// Daily rent (design/04: rent is a burn sink). Provisional tuning: one day
/// is 1440 ticks; medians from human sessions should set the real number.
const DAY_TICKS: u64 = 1440;
const RENT_CENTS: i64 = 200;

/// Save format version. v1 = legacy version-less saves (identical schema);
/// v2 = stamped. Loaders migrate v1 forward and refuse anything newer.
pub const SAVE_VERSION: u32 = 2;

fn save_v1() -> u32 {
    1
}

/// Minimal M0 world: 1 section 16x16, walls on border, one floor.
#[derive(Debug, Serialize, Deserialize)]
pub struct World {
    pub tick: u64,
    pub seed: u64,
    #[serde(default = "save_v1")]
    version: u32,
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
    /// World-hosted cyberspace: Deck arenas keyed by host id, one Net graph,
    /// one power grid. Clients render these; the sim owns them (design/08).
    #[serde(default)]
    hosts: BTreeMap<u64, Host>,
    #[serde(default)]
    pub veil: Veil,
    #[serde(default)]
    pub net: NetGraph,
    #[serde(default)]
    pub power: PowerGrid,
    #[serde(default)]
    shops: Vec<PlacedShop>,
    #[serde(skip)]
    sched: Scheduler,
    /// Play-session funnel milestones. Default (all empty) for old saves.
    #[serde(default)]
    funnel: Funnel,
    /// Rented room tile (design/01: a room above the noodle shop). Sleep
    /// only works here. Default None for old saves (no room on record).
    #[serde(default)]
    home: Option<Pos>,
    /// Wholesale supplier account for shop restocking. Lazily opened on
    /// first restock (reserved system id); default None for old saves.
    #[serde(default)]
    supplier: Option<u64>,
}

fn default_jobs() -> Vec<(i64, u8, u8)> {
    DEFAULT_JOBS.to_vec()
}

impl World {
    pub fn new(seed: u64) -> Self {
        Self {
            tick: 0,
            seed,
            version: SAVE_VERSION,
            beings: BTreeMap::new(),
            jobs: DEFAULT_JOBS.to_vec(),
            ledger: Ledger::new(),
            accts: BTreeMap::new(),
            shops: Vec::new(),
            npcs: BTreeMap::new(),
            sections: Vec::new(),
            hosts: BTreeMap::new(),
            net: NetGraph::new(),
            power: PowerGrid::new(),
            veil: Veil::new(),
            sched: Scheduler::new(),
            funnel: Funnel::default(),
            supplier: None,
            home: None,
        }
    }

    /// Override the job table from data/jobs.yaml (validated by data::parse_jobs).
    pub fn set_jobs(&mut self, defs: &[crate::data::JobDef]) {
        if !defs.is_empty() {
            self.jobs = defs
                .iter()
                .map(|d| (d.payout_cents, d.mood_cost, d.food_cost))
                .collect();
        }
    }

    /// Generate (or fetch) a section, constraining shared-side ports from
    /// existing neighbors so roads/rivers continue across the seam.
    /// Presence and absence both propagate: a neighbor with no facing port
    /// shuts this side instead of rolling a road onto its wall.
    pub fn ensure_section(&mut self, sx: i32, sy: i32) {
        if self.sections.iter().any(|s| s.sx == sx && s.sy == sy) {
            return;
        }
        let face = |x: i32, y: i32, f: fn(&Section) -> Option<u8>| -> (bool, Option<u8>) {
            match self.sections.iter().find(|s| s.sx == x && s.sy == y) {
                None => (false, None),
                Some(s) => (true, f(s)),
            }
        };
        let mut want = Ports::default();
        let mut shut = [false; 4]; // N,S,E,W
        match face(sx - 1, sy, |s| s.ports.e) {
            (_, Some(v)) => want.w = Some(v),
            (true, None) => shut[3] = true,
            _ => {}
        }
        match face(sx + 1, sy, |s| s.ports.w) {
            (_, Some(v)) => want.e = Some(v),
            (true, None) => shut[2] = true,
            _ => {}
        }
        match face(sx, sy - 1, |s| s.ports.s) {
            (_, Some(v)) => want.n = Some(v),
            (true, None) => shut[0] = true,
            _ => {}
        }
        match face(sx, sy + 1, |s| s.ports.n) {
            (_, Some(v)) => want.s = Some(v),
            (true, None) => shut[1] = true,
            _ => {}
        }
        self.sections
            .push(gen_section_weighted(self.seed, sx, sy, want, &[], shut));
    }

    pub fn section_at(&self, sx: i32, sy: i32) -> Option<&Section> {
        self.sections.iter().find(|s| s.sx == sx && s.sy == sy)
    }

    pub fn section_count(&self) -> usize {
        self.sections.len()
    }

    /// Fetch or create a Deck arena for a host (12x8, 100 CPU default).
    pub fn ensure_host(&mut self, id: HostId) -> &mut Host {
        self.hosts
            .entry(id.0)
            .or_insert_with(|| Host::new(id, 12, 8, 100))
    }

    pub fn host(&self, id: HostId) -> Option<&Host> {
        self.hosts.get(&id.0)
    }

    pub fn spawn(&mut self, id: EntityId, pos: Pos, speed: u64) {
        self.ensure_section(pos.sx, pos.sy);
        self.beings.insert(
            id.0,
            Being {
                pos,
                next_at: 0,
                speed,
                food: 80,
                mood: 70,
                money: 200,
                rations: 3,
                // Move-in costs nothing: rent starts accruing tomorrow.
                last_rent_day: self.tick / DAY_TICKS,
            },
        );
        self.sched.schedule(id, 0);
    }

    /// Open a bank account for a being or NPC (branch host). Purchases need one.
    pub fn open_account(&mut self, id: EntityId, branch: HostId) -> Result<u64, SimError> {
        if !self.beings.contains_key(&id.0) && !self.npcs.contains_key(&id.0) {
            return Err(SimError::new("unknown_actor", "spawned entity", "missing"));
        }
        let acct = self.ledger.open(id, branch);
        self.accts.insert(id.0, acct);
        Ok(acct)
    }

    /// Ledger account id for a being or NPC, once opened.
    pub fn account(&self, id: EntityId) -> Option<u64> {
        self.accts.get(&id.0).copied()
    }

    /// Bank balance for a being or NPC, once an account exists. Clients
    /// show this beside pocket cash: deposits move money out of sight
    /// otherwise (design/04 bank + pocket split).
    pub fn bank_balance(&self, id: EntityId) -> Option<i64> {
        self.account(id).and_then(|a| self.ledger.balance(a).ok())
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
        self.shops
            .iter()
            .find(|s| s.x == x && s.y == y)
            .map(|s| &s.shop)
    }

    /// Nearest shop within counter reach (Chebyshev ≤ 1), mirroring the Buy
    /// radius: clients show the counter panel for these tiles.
    pub fn shop_near_at(&self, x: u8, y: u8) -> Option<&Shop> {
        self.shops
            .iter()
            .filter(|s| (s.x as i16 - x as i16).abs() <= 1 && (s.y as i16 - y as i16).abs() <= 1)
            .min_by_key(|s| (s.x as i16 - x as i16).abs() + (s.y as i16 - y as i16).abs())
            .map(|s| &s.shop)
    }

    /// Tile position of the nearest shop within counter reach, if any.
    pub fn shop_pos_near(&self, x: u8, y: u8) -> Option<(u8, u8)> {
        self.shops
            .iter()
            .filter(|s| (s.x as i16 - x as i16).abs() <= 1 && (s.y as i16 - y as i16).abs() <= 1)
            .min_by_key(|s| (s.x as i16 - x as i16).abs() + (s.y as i16 - y as i16).abs())
            .map(|s| (s.x, s.y))
    }

    /// Tile positions of all shops, for map marking (reach-independent:
    /// counters show wherever their tile is on screen).
    pub fn shop_positions(&self) -> Vec<(u8, u8)> {
        self.shops.iter().map(|s| (s.x, s.y)).collect()
    }

    /// Spawn an ambient NPC (no hunger/mood — design 2006 scope cut).
    /// home/work/leisure are local tiles; timetable comes from role.
    pub fn spawn_npc(
        &mut self,
        id: EntityId,
        role: Role,
        home: (u8, u8),
        work: (u8, u8),
        leisure: (u8, u8),
        speed: u64,
    ) {
        // NPCs need ground like PCs: ensure their section (spawn does the
        // same for beings; without it navigation has nothing to route on).
        self.ensure_section(0, 0);
        // Anchor tiles (home/work/leisure) stay walkable like the generator's
        // kept cells: timetables address exact tiles, and collision would
        // otherwise strand NPCs outside unreachable homes.
        if let Some(s) = self.sections.iter_mut().find(|s| s.sx == 0 && s.sy == 0) {
            for (ax, ay) in [home, work, leisure] {
                if ax < 16 && ay < 16 {
                    s.walk[ay as usize * 16 + ax as usize] = true;
                }
            }
        }
        self.npcs.insert(
            id.0,
            Npc {
                pos: Pos {
                    city: 1,
                    sx: 0,
                    sy: 0,
                    z: 0,
                    x: home.0,
                    y: home.1,
                },
                next_at: 0,
                speed,
                role,
                home,
                work,
                leisure,
            },
        );
    }

    pub fn npc_positions(&self) -> Vec<(u64, u8, u8)> {
        self.npcs
            .iter()
            .map(|(id, n)| (*id, n.pos.x, n.pos.y))
            .collect()
    }

    pub fn npc_role(&self, id: u64) -> Option<Role> {
        self.npcs.get(&id).map(|n| n.role)
    }

    pub fn npc_full(&self) -> Vec<(u64, Pos, Role)> {
        self.npcs
            .iter()
            .map(|(id, n)| (*id, n.pos, n.role))
            .collect()
    }

    /// Advance NPCs whose turn is due. hour drives timetables; wanted maps
    /// being-id -> standing for cop pursuit (nearest Wanted target).
    /// NPC moves are free within the tick (M1 LOD, noted in design/05).
    /// One BFS step from `from` toward `to` inside a section (None when
    /// arrived or unreachable). NPC navigation shares the PC collision
    /// rules: borders clamp to 1..=14 and walls route around, never through.
    fn bfs_step(s: &Section, from: (u8, u8), to: (u8, u8)) -> Option<(u8, u8)> {
        if from == to || !s.walkable(to.0, to.1) {
            return None;
        }
        let idx = |x: u8, y: u8| y as usize * 16 + x as usize;
        let mut prev: Vec<Option<(u8, u8)>> = vec![None; 256];
        let mut queue = VecDeque::new();
        queue.push_back(from);
        prev[idx(from.0, from.1)] = Some(from);
        while let Some((x, y)) = queue.pop_front() {
            if (x, y) == to {
                break;
            }
            for (dx, dy) in [(0i16, -1), (0, 1), (1, 0), (-1, 0)] {
                let (nx, ny) = (x as i16 + dx, y as i16 + dy);
                if !(1..=14).contains(&nx) || !(1..=14).contains(&ny) {
                    continue;
                }
                let (nx, ny) = (nx as u8, ny as u8);
                if !s.walkable(nx, ny) || prev[idx(nx, ny)].is_some() {
                    continue;
                }
                prev[idx(nx, ny)] = Some((x, y));
                queue.push_back((nx, ny));
            }
        }
        prev[idx(to.0, to.1)]?;
        let mut cur = to;
        while let Some(p) = prev[idx(cur.0, cur.1)] {
            if p == from {
                return Some(cur);
            }
            cur = p;
        }
        None
    }

    pub fn step_npcs(&mut self, hour: u8, wanted: &BTreeMap<u64, Standing>) {
        let ids: Vec<u64> = self.npcs.keys().copied().collect();
        for id in ids {
            let due = self.npcs[&id].next_at;
            if due > self.tick {
                continue;
            }
            let table = role_table(self.npcs[&id].role);
            let target = self.npc_target(id, &table, hour, wanted);
            // BFS one step toward the timetable spot so NPCs route around
            // the same walls that block PCs; unreachable targets wait out
            // the tick instead of phasing through.
            let cur = self.npcs[&id].pos;
            let step = self
                .sections
                .iter()
                .find(|s| s.sx == cur.sx && s.sy == cur.sy)
                .and_then(|s| Self::bfs_step(s, (cur.x, cur.y), target));
            let n = self.npcs.get_mut(&id).unwrap();
            if let Some((nx, ny)) = step {
                n.pos.x = nx;
                n.pos.y = ny;
            }
            n.next_at = self.tick + n.speed.max(1);
        }
    }

    fn npc_target(
        &self,
        id: u64,
        table: &crate::civic::Timetable,
        hour: u8,
        wanted: &BTreeMap<u64, Standing>,
    ) -> (u8, u8) {
        let n = &self.npcs[&id];
        if n.role == Role::Cop {
            // Nearest Wanted being, if any — pursuit overrides the timetable.
            let mut best: Option<((u8, u8), i32)> = None;
            for (bid, b) in &self.beings {
                if wanted.get(bid).copied().unwrap_or(Standing::Clean) != Standing::Wanted {
                    continue;
                }
                let d = (b.pos.x as i32 - n.pos.x as i32).abs()
                    + (b.pos.y as i32 - n.pos.y as i32).abs();
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
            b.food = food;
            b.mood = mood;
            b.money = money;
            b.rations = rations;
        }
    }

    pub fn needs(&self, id: EntityId) -> Option<(u8, u8, i64, u8)> {
        self.beings
            .get(&id.0)
            .map(|b| (b.food, b.mood, b.money, b.rations))
    }

    pub fn pos_of(&self, id: EntityId) -> Option<Pos> {
        self.beings.get(&id.0).map(|b| b.pos)
    }

    /// Next-step hint for new players. Pure function of needs state —
    /// the shell shows it as the objective line; tests pin the priority.
    pub fn advise(&self, id: EntityId) -> String {
        let Some(b) = self.beings.get(&id.0) else {
            return "spawn in".to_string();
        };
        if b.food == 0 || b.mood == 0 {
            return "Locked out of work: Eat (ration) and Watch (mood) to recover".to_string();
        }
        if b.food < 30 {
            return if b.rations > 0 {
                "Food low: Eat a ration".to_string()
            } else {
                "Food low, pack empty: buy a ration at a gold counter (walk on, Buy)".to_string()
            };
        }
        if b.mood < 30 {
            return "Mood low: Watch a show (-$0.30)".to_string();
        }
        if b.money < 0 {
            return "In debt: work to climb out".to_string();
        }
        let pay0 = self.jobs.first().map(|j| j.0).unwrap_or(0);
        if b.money < pay0.max(100) {
            return "Cash low: Work a job (Work 0/1)".to_string();
        }
        // Firewall objective (M0.5): needs met, so point at the big purchase
        // and state its benefit. Funds mirror what Buy can spend (pocket +
        // bank); skipped once host 1 is filtered.
        if !self.hosts.get(&1).is_some_and(|h| h.firewall()) {
            if let Some(price) = self
                .shops
                .iter()
                .flat_map(|s| s.shop.stock.get("firewall"))
                .map(|l| l.price_cents)
                .min()
            {
                let bank = self
                    .accts
                    .get(&id.0)
                    .and_then(|a| self.ledger.balance(*a).ok())
                    .unwrap_or(0);
                let funds = b.money + bank;
                let tag = format!("${:.2}", price as f64 / 100.0);
                if funds >= price {
                    return format!("Buy the firewall ({tag}): inbound traces crawl");
                }
                if funds * 2 >= price {
                    return format!("Save up for the firewall ({tag}): inbound traces crawl");
                }
            }
        }
        "Settled: explore (arrows/hjkl), Talk to n, hack via Deck tab".to_string()
    }

    /// Next scheduled act tick (server scheduler jumps the clock here).
    pub fn next_at(&self, id: EntityId) -> u64 {
        self.beings
            .get(&id.0)
            .map(|b| b.next_at)
            .unwrap_or(self.tick)
    }

    /// Register the rented room. Sleep only works on this tile.
    pub fn set_home(&mut self, pos: Pos) {
        self.home = Some(pos);
    }

    /// Report a successful offensive hack (Deck kill/takeover/exploit) for
    /// funnel tracking. The first reporter claims protagonism.
    pub fn mark_hack(&mut self, actor: EntityId) {
        self.funnel.claim(actor);
        if self.funnel.for_protagonist(actor) && self.funnel.hack.is_none() {
            self.funnel.hack = Some(self.tick);
        }
    }

    /// Report an inbound trace on the protagonist's programs.
    pub fn mark_traced(&mut self, actor: EntityId) {
        self.funnel.claim(actor);
        if self.funnel.for_protagonist(actor) && self.funnel.traced.is_none() {
            self.funnel.traced = Some(self.tick);
        }
    }

    /// Warden retaliation beat after a deck intrusion act (design/03):
    /// runs the host's retaliation, records funnel marks, and returns
    /// narrative event kinds for the caller to relay. Takes disjoint
    /// pieces (not &mut self) so `apply` can call it beside its live
    /// `beings` borrow.
    fn deck_beat(
        host: &mut Host,
        funnel: &mut Funnel,
        tick: u64,
        actor: EntityId,
        hacked: bool,
    ) -> Vec<String> {
        if hacked {
            funnel.claim(actor);
            if funnel.for_protagonist(actor) && funnel.hack.is_none() {
                funnel.hack = Some(tick);
            }
        }
        let r = host.retaliate();
        let mut kinds = Vec::new();
        if r.traced {
            funnel.claim(actor);
            if funnel.for_protagonist(actor) && funnel.traced.is_none() {
                funnel.traced = Some(tick);
            }
            kinds.push("traced".to_string());
        }
        if r.killed {
            kinds.push("warden_kill".to_string());
        }
        if r.burned {
            kinds.push("trace_burned".to_string());
        }
        kinds
    }

    /// One-line funnel report for demos and CLIs. `-` = milestone unreached.
    pub fn funnel_line(&self) -> String {
        let f = &self.funnel;
        let show = |v: Option<u64>| v.map(|t| t.to_string()).unwrap_or_else(|| "-".to_string());
        format!(
            "funnel started={} job={} hack={} traced={} survived={}",
            show(f.started),
            show(f.job),
            show(f.hack),
            show(f.traced),
            show(f.survived)
        )
    }

    pub fn apply(&mut self, intent: &Intent) -> Result<(Delta, Vec<Event>), SimError> {
        let b = self.beings.get_mut(&intent.actor.0).ok_or(SimError::new(
            "unknown_actor",
            "spawned entity",
            "missing",
        ))?;
        if self.tick < b.next_at {
            return Err(SimError::new("too_early", "tick >= next_at", "acted early"));
        }
        let mut pos = b.pos;
        // Pressure clock: every act costs 1 food + 1 mood (player-only).
        b.food = b.food.saturating_sub(1);
        b.mood = b.mood.saturating_sub(1);
        // Frozen-chunk catch-up (design/02): while this being's chunk was
        // unloaded, needs accrued at ~1/min instead of the active 1/act.
        // Missed == 0 on the normal path, so steady play is untouched.
        let missed = self.tick.saturating_sub(b.next_at);
        if missed > 0 {
            let catch = (missed / 60).min(100) as u8;
            b.food = b.food.saturating_sub(catch);
            b.mood = b.mood.saturating_sub(catch);
        }
        // Daily rent (design/04 economy sink): at most one day's charge per
        // act, never back-billed. May drive pocket money negative (debt).
        let day = self.tick / DAY_TICKS;
        if day > b.last_rent_day {
            b.last_rent_day = day;
            b.money -= RENT_CENTS;
        }
        let mut kind = "acted";
        let mut extra_kinds: Vec<String> = Vec::new();
        match intent.verb {
            Verb::Wait => {}
            Verb::Move(d) => {
                let (dx, dy) = match d {
                    Dir::N => (0, -1),
                    Dir::S => (0, 1),
                    Dir::E => (1, 0),
                    Dir::W => (-1, 0),
                };
                let nx = pos.x as i16 + dx;
                let ny = pos.y as i16 + dy;
                if (1..=14).contains(&nx) && (1..=14).contains(&ny) {
                    let walled = self
                        .sections
                        .iter()
                        .find(|s| s.sx == pos.sx && s.sy == pos.sy)
                        .is_some_and(|s| !s.walkable(nx as u8, ny as u8));
                    if walled {
                        return Err(SimError::new("blocked", "walkable tile", "wall"));
                    }
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
                let (pay, mood_c, food_c) = *self.jobs.get(j as usize).ok_or(SimError::new(
                    "no_job",
                    "known job",
                    "bad id",
                ))?;
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
            Verb::Sleep => {
                let home = self
                    .home
                    .ok_or(SimError::new("no_home", "rented room", "homeless"))?;
                if pos != home {
                    return Err(SimError::new("not_home", "your room", "elsewhere"));
                }
                // Rest to next 06:00. The slept hours cost nothing beyond
                // this act; crossing midnight still owes rent next apply.
                let day_min = self.tick % 1440;
                self.tick += if day_min < 360 {
                    360 - day_min
                } else {
                    1440 - day_min + 360
                };
                kind = "slept";
            }
            Verb::DamageWire(from, to) => {
                self.net.damage_wire(from, to)?;
                kind = "damaged";
            }
            Verb::DeckLoad { x, y } => {
                // Field-disjoint with the live `beings` borrow below,
                // like the Buy arm: no &mut self calls in this match.
                let id = {
                    let host = self
                        .hosts
                        .entry(1)
                        .or_insert_with(|| Host::new(HostId(1), 12, 8, 100));
                    EntityId(100 + host.progs.len() as u64)
                };
                self.hosts
                    .get_mut(&1)
                    .unwrap()
                    .load(id, intent.actor, CellPos(x, y), 3, 1)?;
                kind = "loaded";
                extra_kinds = Self::deck_beat(
                    self.hosts.get_mut(&1).unwrap(),
                    &mut self.funnel,
                    self.tick,
                    intent.actor,
                    false,
                );
            }
            Verb::DeckStrike { x, y } => {
                let killed = self
                    .hosts
                    .entry(1)
                    .or_insert_with(|| Host::new(HostId(1), 12, 8, 100))
                    .strike(CellPos(x, y))?;
                if killed {
                    extra_kinds.push("kill".to_string());
                }
                kind = "struck";
                extra_kinds.extend(Self::deck_beat(
                    self.hosts.get_mut(&1).unwrap(),
                    &mut self.funnel,
                    self.tick,
                    intent.actor,
                    killed,
                ));
            }
            Verb::VeilEnter { mp } => {
                self.veil.enter_at(intent.actor, mp, 1)?;
                kind = "entered";
            }
            Verb::VeilMove { area } => {
                self.veil.move_to(intent.actor, area)?;
                kind = "moved";
            }
            Verb::VeilSeize => {
                self.veil.transfer(intent.actor, EntityId(70), 3, true)?;
                kind = "seized";
            }
            Verb::VeilEject => {
                self.veil.eject(intent.actor)?;
                kind = "ejected";
            }
            Verb::Deposit(amt) => {
                if amt <= 0 {
                    return Err(SimError::new("neg_amt", "amt > 0", "non-positive"));
                }
                let acct = self
                    .accts
                    .get(&intent.actor.0)
                    .copied()
                    .ok_or(SimError::new("no_account", "open account", "unbanked"))?;
                if b.money < amt {
                    return Err(SimError::new("overdraw", "pocket funds", "short"));
                }
                // Verify the account before debiting pocket: mint itself
                // cannot fail afterward, so either everything moves or nothing.
                self.ledger.balance(acct)?;
                b.money -= amt;
                self.ledger.mint(acct, amt)?;
                kind = "deposited";
            }
            Verb::Withdraw(amt) => {
                if amt <= 0 {
                    return Err(SimError::new("neg_amt", "amt > 0", "non-positive"));
                }
                let acct = self
                    .accts
                    .get(&intent.actor.0)
                    .copied()
                    .ok_or(SimError::new("no_account", "open account", "unbanked"))?;
                let bal = self.ledger.balance(acct)?;
                if bal < amt {
                    return Err(SimError::new("overdraw", "bank funds", "short"));
                }
                self.ledger.burn(acct, amt)?;
                b.money += amt;
                kind = "withdrew";
            }
            Verb::Buy(ref item) => {
                // Firewall installs on world host 1 (the starter rig's arena);
                // check before charging so a duplicate costs nothing.
                if item == "firewall" && self.hosts.get(&1).is_some_and(|h| h.firewall()) {
                    return Err(SimError::new(
                        "already_installed",
                        "no firewall",
                        "installed",
                    ));
                }
                // Counters serve adjacent tiles: no pixel-exact standing.
                // Nearest shop within Chebyshev distance 1 (shops are sparse).
                let shop = self
                    .shops
                    .iter_mut()
                    .filter(|s| {
                        (s.x as i16 - pos.x as i16).abs() <= 1
                            && (s.y as i16 - pos.y as i16).abs() <= 1
                    })
                    .min_by_key(|s| {
                        (s.x as i16 - pos.x as i16).abs() + (s.y as i16 - pos.y as i16).abs()
                    })
                    .ok_or(SimError::new("no_shop", "shop counter", "none here"))?;
                let shop = &mut shop.shop;
                // Shuttered counters don't sell (game hour matches NPC time).
                let hour = ((self.tick / 60) % 24) as u8;
                if !shop.is_open(hour) {
                    return Err(SimError::new("shuttered", "open hours", "closed"));
                }
                let acct = self
                    .accts
                    .get(&intent.actor.0)
                    .copied()
                    .ok_or(SimError::new("no_account", "open account", "unbanked"))?;
                // Paychecks land in pocket cash while shops draw the bank
                // ledger: bridge them pocket-first so the displayed balance
                // always moves on a purchase. Check the combined total
                // before moving anything.
                let price = shop
                    .stock
                    .get(item)
                    .map(|l| l.price_cents)
                    .ok_or(SimError::new("no_line", "stocked item", "unknown"))?;
                let bank = self.ledger.balance(acct)?;
                let cash = b.money.min(price);
                if bank < price - cash {
                    return Err(SimError::new("overdraw", "funds", "short"));
                }
                shop.buy_with_cash(&mut self.ledger, acct, cash, item)?;
                b.money -= cash;
                // Restock from sales: the wholesale supplier account fills
                // lines the till can cover, so shelves refill while you earn.
                let supplier = match self.supplier {
                    Some(id) => id,
                    None => {
                        let id = self.ledger.open(EntityId(u64::MAX), HostId(1));
                        self.supplier = Some(id);
                        id
                    }
                };
                shop.auto_order(&mut self.ledger, supplier);
                // M1 food shops sell rations that go straight to the pack.
                if item == "ration" {
                    b.rations = b.rations.saturating_add(1);
                }
                // Meds are fast, pricey triage: big mood restore plus a
                // little food (design/10: order food/meds, shows prop mood).
                if item == "meds" {
                    b.mood = (b.mood + 25).min(100);
                    b.food = (b.food + 5).min(100);
                }
                // Packet filter for the starter rig's arena (design/04).
                // Same defaults as ensure_host; field access keeps the
                // beings borrow alive where a method call would not.
                if item == "firewall" {
                    self.hosts
                        .entry(1)
                        .or_insert_with(|| Host::new(HostId(1), 12, 8, 100))
                        .set_firewall(true);
                }
                kind = "bought";
            }
        }
        b.pos = pos;
        // Starving doubles time cost (soft fail, not hard game-over per synthesis).
        let eff = if b.food == 0 {
            b.speed.max(1) * 2
        } else {
            b.speed.max(1)
        };
        b.next_at = self.tick + eff;
        self.sched.schedule(intent.actor, b.next_at);
        // Funnel: protagonist claimed on first applied act; job on first Work.
        self.funnel.claim(intent.actor);
        if self.funnel.started.is_none() {
            self.funnel.started = Some(self.tick);
        }
        if kind == "worked" && self.funnel.job.is_none() {
            self.funnel.job = Some(self.tick);
        }
        // Survived: protagonist still present 100 ticks after first trace.
        if self.funnel.traced.is_some() && self.funnel.survived.is_none() {
            if let Some(t0) = self.funnel.traced {
                if self.tick >= t0 + 100 && self.pos_of(intent.actor).is_some() {
                    self.funnel.survived = Some(self.tick);
                }
            }
        }
        self.tick += 1;
        // NPC timetables advance on game time, in the act: per-frame calls
        // at an unchanged tick are no-ops, and this keeps intent replay
        // (which has no frames) on the same NPC tracks as live play.
        let hour = ((self.tick / 60) % 24) as u8;
        self.step_npcs(hour, &BTreeMap::new());
        let mut events = vec![Event {
            tick: self.tick,
            actor: intent.actor,
            kind: kind.into(),
        }];
        for k in extra_kinds {
            events.push(Event {
                tick: self.tick,
                actor: intent.actor,
                kind: k,
            });
        }
        Ok((
            Delta {
                moved: vec![(intent.actor, pos)],
            },
            events,
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
        match w.version {
            1 => w.version = SAVE_VERSION, // v1 schema is identical; stamp forward
            SAVE_VERSION => {}
            _ => return Err(SimError::new("load", "snapshot v1-v2", "newer version")),
        }
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
        Pos {
            city: 1,
            sx: 0,
            sy: 0,
            z: 0,
            x,
            y,
        }
    }

    #[test]
    fn replay_is_deterministic() {
        let run = || {
            let mut w = World::new(42);
            w.spawn(EntityId(1), pos(8, 8), 10);
            for _ in 0..5 {
                let _ = w.apply(&Intent {
                    actor: EntityId(1),
                    verb: Verb::Move(Dir::E),
                });
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
            let _ = w1.apply(&Intent {
                actor: EntityId(1),
                verb: Verb::Move(Dir::W),
            });
            let _ = w2.apply(&Intent {
                actor: EntityId(1),
                verb: Verb::Move(Dir::W),
            });
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
        let _ = w.apply(&Intent {
            actor: EntityId(1),
            verb: Verb::Move(Dir::E),
        });
        let s = w.save_json();
        let w2 = World::load_json(&s).expect("load");
        assert_eq!(w.snapshot_hash(), w2.snapshot_hash());
        assert!(World::load_json("{bad").is_err());
    }

    #[test]
    fn save_version_stamps_and_migrates_v1() {
        let mut w = World::new(7);
        w.spawn(EntityId(1), pos(8, 8), 2);
        let _ = w.apply(&Intent {
            actor: EntityId(1),
            verb: Verb::Move(Dir::E),
        });
        let s = w.save_json();
        let v: serde_json::Value = serde_json::from_str(&s).unwrap();
        assert_eq!(v["version"], serde_json::json!(SAVE_VERSION));
        // Legacy v1 save (no version key) migrates on load, hash intact.
        let mut legacy = v.clone();
        legacy.as_object_mut().unwrap().remove("version");
        let w2 = World::load_json(&serde_json::to_string(&legacy).unwrap()).unwrap();
        assert_eq!(w2.snapshot_hash(), w.snapshot_hash());
        // Future versions refuse with a clear error, not silent corruption.
        let mut future = v;
        future
            .as_object_mut()
            .unwrap()
            .insert("version".into(), 99.into());
        let err = World::load_json(&serde_json::to_string(&future).unwrap()).unwrap_err();
        assert_eq!(err.kind, "load");
        assert!(World::load_json("{bad").is_err());
    }

    #[test]
    fn survival_loop_pays_costs_and_restores() {
        let mut w = World::new(1);
        w.spawn(EntityId(1), pos(8, 8), 1);
        // Starter: 80/70, 200c, 3 rations. Decay applies per act.
        let (_, mood0, m0, _) = w.needs(EntityId(1)).unwrap();
        let _ = w.apply(&Intent {
            actor: EntityId(1),
            verb: Verb::Work(0),
        });
        let (food, mood, money, _) = w.needs(EntityId(1)).unwrap();
        assert_eq!(money, m0 + 60);
        assert!(mood < mood0 && food < 80);

        // Eat consumes ration, restores food.
        let (_, _, _, r0) = w.needs(EntityId(1)).unwrap();
        let _ = w.apply(&Intent {
            actor: EntityId(1),
            verb: Verb::Eat,
        });
        let (food2, _, _, r1) = w.needs(EntityId(1)).unwrap();
        assert_eq!(r1, r0 - 1);
        assert!(food2 > food);

        // Watch costs money, restores mood.
        let (_, mood_b, money_b, _) = w.needs(EntityId(1)).unwrap();
        let _ = w.apply(&Intent {
            actor: EntityId(1),
            verb: Verb::Watch,
        });
        let (_, mood2, money2, _) = w.needs(EntityId(1)).unwrap();
        assert_eq!(money2, money_b - 30);
        assert!(mood2 > mood_b - 1); // net of decay + restore

        // Lockout at zero mood/food.
        w.set_needs(EntityId(1), 10, 0, 100, 1);
        // need tick >= next_at: jump clock like server would
        w.tick = w.beings[&1].next_at;
        assert!(w
            .apply(&Intent {
                actor: EntityId(1),
                verb: Verb::Work(0)
            })
            .is_err());
        // Starving doubles step cost.
        w.set_needs(EntityId(1), 1, 50, 100, 1);
        w.tick = w.beings[&1].next_at;
        let t0 = w.tick;
        let _ = w.apply(&Intent {
            actor: EntityId(1),
            verb: Verb::Wait,
        });
        assert_eq!(w.beings[&1].next_at, t0 + 2); // speed 1 x2 while starving
    }

    #[test]
    fn frozen_chunk_applies_catchup_decay() {
        let mut w = World::new(1);
        w.spawn(EntityId(1), pos(8, 8), 1);
        w.tick = w.next_at(EntityId(1));
        w.apply(&Intent {
            actor: EntityId(1),
            verb: Verb::Wait,
        })
        .unwrap();
        let (f0, m0, _, _) = w.needs(EntityId(1)).unwrap();
        // 10 frozen minutes pass with the chunk unloaded.
        w.tick += 600;
        w.apply(&Intent {
            actor: EntityId(1),
            verb: Verb::Wait,
        })
        .unwrap();
        let (f1, m1, _, _) = w.needs(EntityId(1)).unwrap();
        assert_eq!((f0 - f1, m0 - m1), (11, 11)); // 10 catch-up + 1 act
                                                  // No absence, no catch-up: steady play costs exactly the act.
        w.tick = w.next_at(EntityId(1));
        w.apply(&Intent {
            actor: EntityId(1),
            verb: Verb::Wait,
        })
        .unwrap();
        let (f2, m2, _, _) = w.needs(EntityId(1)).unwrap();
        assert_eq!((f1 - f2, m1 - m2), (1, 1));
        // Huge absence floors at zero without panic.
        w.tick += 1_000_000;
        w.apply(&Intent {
            actor: EntityId(1),
            verb: Verb::Wait,
        })
        .unwrap();
        let (f3, m3, _, _) = w.needs(EntityId(1)).unwrap();
        assert_eq!((f3, m3), (0, 0));
    }

    #[test]
    fn firewall_purchase_arms_world_host() {
        use crate::shop::Shop;
        let mut w = World::new(3);
        w.spawn(EntityId(1), pos(8, 8), 1);
        let acct = w.open_account(EntityId(1), crate::ids::HostId(2)).unwrap();
        w.ledger.mint(acct, 5000).unwrap();
        let mut shop = Shop::new(w.ledger.open(EntityId(9), crate::ids::HostId(9)));
        shop.add_line("firewall", 1500, 1, 1, 1, 1200);
        w.place_shop(8, 8, shop);
        // No arena yet: purchase creates and arms world host 1.
        assert!(w.host(crate::ids::HostId(1)).is_none());
        w.tick = w.next_at(EntityId(1));
        w.apply(&Intent {
            actor: EntityId(1),
            verb: Verb::Buy("firewall".into()),
        })
        .unwrap();
        assert!(w.host(crate::ids::HostId(1)).unwrap().firewall());
        assert_eq!(w.ledger.live_sum(), w.ledger.audit_sum());
        // Duplicate install is refused before charging.
        let before = w.ledger.live_sum();
        w.tick = w.next_at(EntityId(1));
        assert!(w
            .apply(&Intent {
                actor: EntityId(1),
                verb: Verb::Buy("firewall".into()),
            })
            .is_err());
        assert_eq!(w.ledger.live_sum(), before);
    }

    #[test]
    fn pocket_cash_covers_bank_shortfall() {
        use crate::shop::Shop;
        // The reported bug: paychecks land in pocket cash while shops draw
        // the bank ledger, so a $15 firewall rejected a $20 pocket.
        let mut w = World::new(3);
        w.spawn(EntityId(1), pos(8, 8), 1);
        let acct = w.open_account(EntityId(1), crate::ids::HostId(2)).unwrap();
        w.ledger.mint(acct, 1400).unwrap();
        let mut shop = Shop::new(w.ledger.open(EntityId(9), crate::ids::HostId(9)));
        shop.add_line("firewall", 1500, 1, 1, 1, 1200);
        w.place_shop(8, 8, shop);
        // Pocket 200 + bank 1400 covers 1500 pocket-first: pocket drains,
        // bank pays the remaining 1300.
        assert_eq!(w.needs(EntityId(1)).unwrap().2, 200);
        w.tick = w.next_at(EntityId(1));
        w.apply(&Intent {
            actor: EntityId(1),
            verb: Verb::Buy("firewall".into()),
        })
        .unwrap();
        assert!(w.host(crate::ids::HostId(1)).unwrap().firewall());
        assert_eq!(w.needs(EntityId(1)).unwrap().2, 0);
        assert_eq!(w.ledger.balance(acct).unwrap(), 100);
        assert_eq!(w.ledger.live_sum(), w.ledger.audit_sum());
    }

    #[test]
    fn combined_shortfall_is_refused_atomically() {
        use crate::shop::Shop;
        let mut v = World::new(3);
        v.spawn(EntityId(1), pos(8, 8), 1);
        let acct = v.open_account(EntityId(1), crate::ids::HostId(2)).unwrap();
        v.ledger.mint(acct, 50).unwrap();
        let mut shop = Shop::new(v.ledger.open(EntityId(9), crate::ids::HostId(9)));
        shop.add_line("meds", 400, 2, 1, 3, 900);
        v.place_shop(8, 8, shop);
        v.set_needs(EntityId(1), 80, 70, 20, 3);
        // Bank 50 + pocket 20 cannot cover 400: refused, nothing moves.
        v.tick = v.next_at(EntityId(1));
        assert!(v
            .apply(&Intent {
                actor: EntityId(1),
                verb: Verb::Buy("meds".into()),
            })
            .is_err());
        assert_eq!(v.needs(EntityId(1)).unwrap().2, 20);
        assert_eq!(v.ledger.balance(acct).unwrap(), 50);
        assert_eq!(v.shop_at(8, 8).unwrap().stock["meds"].qty, 2);
        assert_eq!(v.ledger.live_sum(), v.ledger.audit_sum());
    }

    #[test]
    fn meds_restore_mood_and_a_little_food() {
        use crate::shop::Shop;
        let mut w = World::new(3);
        w.spawn(EntityId(1), pos(8, 8), 1);
        let acct = w.open_account(EntityId(1), crate::ids::HostId(2)).unwrap();
        w.ledger.mint(acct, 5000).unwrap();
        let mut shop = Shop::new(w.ledger.open(EntityId(9), crate::ids::HostId(9)));
        shop.add_line("meds", 400, 2, 1, 3, 900);
        w.place_shop(8, 8, shop);
        w.set_needs(EntityId(1), 40, 50, 5000, 3);
        w.tick = w.next_at(EntityId(1));
        w.apply(&Intent {
            actor: EntityId(1),
            verb: Verb::Buy("meds".into()),
        })
        .unwrap();
        // Pricey triage: +25 mood, +5 food, ledger stays closed.
        let (food, mood, _, _) = w.needs(EntityId(1)).unwrap();
        assert_eq!((food, mood), (44, 74));
        assert_eq!(w.ledger.live_sum(), w.ledger.audit_sum());
    }

    #[test]
    fn pocket_bank_bridge_moves_both_ways() {
        let mut w = World::new(3);
        w.spawn(EntityId(1), pos(8, 8), 1);
        let acct = w.open_account(EntityId(1), crate::ids::HostId(2)).unwrap();
        w.ledger.mint(acct, 1000).unwrap();
        // Pocket 200 → bank: audit grows by the deposit.
        w.tick = w.next_at(EntityId(1));
        w.apply(&Intent {
            actor: EntityId(1),
            verb: Verb::Deposit(150),
        })
        .unwrap();
        assert_eq!(w.needs(EntityId(1)).unwrap().2, 50);
        assert_eq!(w.ledger.balance(acct).unwrap(), 1150);
        assert_eq!(w.ledger.live_sum(), w.ledger.audit_sum());
        // Bank → pocket: burn logged, pocket credited.
        w.tick = w.next_at(EntityId(1));
        w.apply(&Intent {
            actor: EntityId(1),
            verb: Verb::Withdraw(200),
        })
        .unwrap();
        assert_eq!(w.needs(EntityId(1)).unwrap().2, 250);
        assert_eq!(w.ledger.balance(acct).unwrap(), 950);
        assert_eq!(w.ledger.live_sum(), w.ledger.audit_sum());
        // Overdraw, non-positive, and unbanked: refused, nothing moves.
        for verb in [
            Verb::Deposit(99999),
            Verb::Withdraw(99999),
            Verb::Deposit(0),
            Verb::Deposit(-5),
            Verb::Withdraw(0),
            Verb::Withdraw(-5),
        ] {
            w.tick = w.next_at(EntityId(1));
            assert!(w
                .apply(&Intent {
                    actor: EntityId(1),
                    verb
                })
                .is_err());
        }
        assert_eq!(w.needs(EntityId(1)).unwrap().2, 250);
        assert_eq!(w.ledger.balance(acct).unwrap(), 950);
        w.spawn(EntityId(2), pos(8, 8), 1);
        w.tick = w.next_at(EntityId(2));
        assert!(w
            .apply(&Intent {
                actor: EntityId(2),
                verb: Verb::Deposit(10),
            })
            .is_err());
        assert!(w
            .apply(&Intent {
                actor: EntityId(2),
                verb: Verb::Withdraw(10),
            })
            .is_err());
        assert_eq!(w.ledger.live_sum(), w.ledger.audit_sum());
    }

    #[test]
    fn funnel_tracks_started_job_hack_trace_survived() {
        let mut w = World::new(1);
        w.spawn(EntityId(1), pos(8, 8), 10);
        assert!(w.funnel_line().contains("started=-"));
        w.tick = w.next_at(EntityId(1));
        w.apply(&Intent {
            actor: EntityId(1),
            verb: Verb::Work(0),
        })
        .unwrap();
        // First applied act claims started + job.
        let line = w.funnel_line();
        assert!(line.contains("started=0"), "{line}");
        assert!(line.contains("job=0"), "{line}");
        assert!(line.contains("hack=-"), "{line}");
        // Another actor's milestones don't move the protagonist's funnel.
        w.spawn(EntityId(2), pos(8, 8), 10);
        w.mark_hack(EntityId(2));
        assert!(w.funnel_line().contains("hack=-"));
        w.mark_hack(EntityId(1));
        w.mark_traced(EntityId(1));
        let line = w.funnel_line();
        assert!(!line.contains("hack=-"), "{line}");
        assert!(!line.contains("traced=-"), "{line}");
        assert!(line.contains("survived=-"), "{line}");
        // Still present 100 ticks after the trace: survived.
        w.tick += 200;
        w.tick = w.next_at(EntityId(1)).max(w.tick);
        w.apply(&Intent {
            actor: EntityId(1),
            verb: Verb::Wait,
        })
        .unwrap();
        assert!(
            !w.funnel_line().contains("survived=-"),
            "{}",
            w.funnel_line()
        );
    }

    #[test]
    fn walls_block_movement() {
        let mut w = World::new(4);
        w.spawn(EntityId(1), pos(8, 8), 10);
        // Find a walkable tile with a walled orthogonal neighbor.
        let s = w.section_at(0, 0).unwrap();
        let mut probe = None;
        for y in 1..15u8 {
            for x in 1..15u8 {
                if !s.walkable(x, y) {
                    continue;
                }
                for (d, dx, dy) in [
                    (Dir::N, 0, -1),
                    (Dir::S, 0, 1),
                    (Dir::E, 1, 0),
                    (Dir::W, -1, 0),
                ] {
                    let (nx, ny) = (x as i16 + dx, y as i16 + dy);
                    if (1..=14).contains(&nx)
                        && (1..=14).contains(&ny)
                        && !s.walkable(nx as u8, ny as u8)
                    {
                        probe = Some(((x, y), d));
                    }
                }
            }
        }
        let ((fx, fy), dir) = probe.expect("seed has no wall-adjacent floor");
        w.beings.get_mut(&1).unwrap().pos = Pos {
            city: 1,
            sx: 0,
            sy: 0,
            z: 0,
            x: fx,
            y: fy,
        };
        w.tick = w.next_at(EntityId(1));
        assert!(w
            .apply(&Intent {
                actor: EntityId(1),
                verb: Verb::Move(dir),
            })
            .is_err());
        assert_eq!((w.beings[&1].pos.x, w.beings[&1].pos.y), (fx, fy));
    }

    #[test]
    fn counter_serves_adjacent_tiles() {
        use crate::shop::Shop;
        let mut w = World::new(3);
        w.spawn(EntityId(1), pos(8, 8), 1);
        let acct = w.open_account(EntityId(1), crate::ids::HostId(2)).unwrap();
        w.ledger.mint(acct, 5000).unwrap();
        let mut shop = Shop::new(w.ledger.open(EntityId(9), crate::ids::HostId(9)));
        shop.add_line("ration", 100, 5, 2, 5, 50);
        w.place_shop(9, 8, shop);
        // One tile east of the counter: purchase works.
        w.tick = w.next_at(EntityId(1));
        w.apply(&Intent {
            actor: EntityId(1),
            verb: Verb::Buy("ration".into()),
        })
        .unwrap();
        assert_eq!(w.needs(EntityId(1)).unwrap().3, 4);
        assert_eq!(w.shop_pos_near(8, 8), Some((9, 8)));
        assert_eq!(w.shop_pos_near(0, 0), None);
        assert_eq!(w.shop_positions(), vec![(9, 8)]);
        // Four tiles off: no counter in reach.
        w.beings.get_mut(&1).unwrap().pos = Pos {
            city: 1,
            sx: 0,
            sy: 0,
            z: 0,
            x: 5,
            y: 8,
        };
        w.tick = w.next_at(EntityId(1));
        assert!(w
            .apply(&Intent {
                actor: EntityId(1),
                verb: Verb::Buy("ration".into()),
            })
            .is_err());
    }

    #[test]
    fn sales_restock_shelves_from_till() {
        use crate::shop::Shop;
        let mut w = World::new(3);
        w.spawn(EntityId(1), pos(8, 8), 1);
        let acct = w.open_account(EntityId(1), crate::ids::HostId(2)).unwrap();
        w.ledger.mint(acct, 5000).unwrap();
        let till = w.ledger.open(EntityId(9), crate::ids::HostId(9));
        w.ledger.mint(till, 1000).unwrap();
        let mut shop = Shop::new(till);
        shop.add_line("ration", 100, 1, 2, 5, 50);
        w.place_shop(8, 8, shop);
        // Last ration on the shelf: sale funds an immediate restock.
        w.tick = w.next_at(EntityId(1));
        w.apply(&Intent {
            actor: EntityId(1),
            verb: Verb::Buy("ration".into()),
        })
        .unwrap();
        let stocked = w.shop_at(8, 8).unwrap().stock["ration"].qty;
        assert_eq!(stocked, 5);
        let supplier = w.supplier.expect("supplier account opened");
        assert_eq!(w.ledger.balance(supplier).unwrap(), 50);
        assert_eq!(w.ledger.live_sum(), w.ledger.audit_sum());
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
        // Step away: no counter in reach. (Positioned directly: movement
        // itself is covered by section_crossing and walls_block_movement.)
        w.beings.get_mut(&1).unwrap().pos = Pos {
            city: 1,
            sx: 0,
            sy: 0,
            z: 0,
            x: 5,
            y: 8,
        };
        w.tick = w.next_at(EntityId(1));
        assert!(w
            .apply(&Intent {
                actor: EntityId(1),
                verb: Verb::Buy("ration".into())
            })
            .is_err());
        // Back on the tile: buy, pack gains a ration, then eat it.
        w.beings.get_mut(&1).unwrap().pos = Pos {
            city: 1,
            sx: 0,
            sy: 0,
            z: 0,
            x: 8,
            y: 8,
        };
        w.tick = w.next_at(EntityId(1));
        w.apply(&Intent {
            actor: EntityId(1),
            verb: Verb::Buy("ration".into()),
        })
        .unwrap();
        assert_eq!(w.needs(EntityId(1)).unwrap().3, 4); // spawn 3 + 1 bought
        w.tick = w.next_at(EntityId(1));
        let (f0, _, _, _) = w.needs(EntityId(1)).unwrap();
        w.apply(&Intent {
            actor: EntityId(1),
            verb: Verb::Eat,
        })
        .unwrap();
        assert!(w.needs(EntityId(1)).unwrap().0 > f0);
        assert_eq!(w.ledger.live_sum(), w.ledger.audit_sum());
        // Unbanked stranger cannot buy.
        w.spawn(EntityId(2), pos(8, 8), 1);
        w.tick = w.next_at(EntityId(2));
        assert!(w
            .apply(&Intent {
                actor: EntityId(2),
                verb: Verb::Buy("ration".into())
            })
            .is_err());
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
            w.apply(&Intent {
                actor: EntityId(1),
                verb: Verb::Move(Dir::E),
            })
            .unwrap();
        }
        let b = &w.beings[&1];
        assert_eq!((b.pos.sx, b.pos.x), (1, 1));
        assert_eq!(w.section_count(), 2);
        // Entry port on the shared side matches the crossing row (y=8).
        assert_eq!(w.section_at(1, 0).unwrap().ports.w, Some(8));
        // Walk back west: no new section, same sim both ways.
        for _ in 0..7 {
            w.tick = w.next_at(EntityId(1));
            w.apply(&Intent {
                actor: EntityId(1),
                verb: Verb::Move(Dir::W),
            })
            .unwrap();
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
            v.apply(&Intent {
                actor: EntityId(1),
                verb: Verb::Move(d),
            })
            .unwrap();
        }
        assert_eq!(w.snapshot_hash(), v.snapshot_hash());
    }

    #[test]
    fn hosted_deck_persists_through_save() {
        use crate::deck::CellPos;
        let mut w = World::new(1);
        w.spawn(EntityId(1), pos(8, 8), 1);
        w.ensure_host(crate::ids::HostId(1));
        w.hosts
            .get_mut(&1)
            .unwrap()
            .load(EntityId(10), EntityId(1), CellPos(2, 2), 3, 1)
            .unwrap();
        let s = w.save_json();
        let w2 = World::load_json(&s).unwrap();
        assert_eq!(w.snapshot_hash(), w2.snapshot_hash());
        assert!(w2.host(crate::ids::HostId(1)).is_some());
        assert!(w2.host(crate::ids::HostId(9)).is_none());
    }

    #[test]
    fn advise_prioritizes_lockout_then_food_mood_cash() {
        let mut w = World::new(1);
        w.spawn(EntityId(1), pos(8, 8), 1);
        // Settled starter.
        assert!(w.advise(EntityId(1)).starts_with("Settled"));
        // Cash low beats settled.
        w.set_needs(EntityId(1), 80, 70, 10, 3);
        assert!(w.advise(EntityId(1)).contains("Work"));
        // Mood low beats cash.
        w.set_needs(EntityId(1), 80, 10, 10, 3);
        assert!(w.advise(EntityId(1)).contains("Mood"));
        // Food low beats mood; pack vs counter branches.
        w.set_needs(EntityId(1), 10, 10, 10, 2);
        assert!(w.advise(EntityId(1)).contains("Eat a ration"));
        w.set_needs(EntityId(1), 10, 10, 10, 0);
        assert!(w.advise(EntityId(1)).contains("gold counter"));
        // Lockout beats everything.
        w.set_needs(EntityId(1), 10, 0, 10, 0);
        assert!(w.advise(EntityId(1)).contains("Locked out"));
        // Unknown actor.
        assert_eq!(w.advise(EntityId(99)), "spawn in");
    }

    #[test]
    fn advise_names_firewall_goal_and_benefit() {
        use crate::shop::Shop;
        let mut w = World::new(3);
        w.spawn(EntityId(1), pos(8, 8), 1);
        let mut shop = Shop::new(w.ledger.open(EntityId(9), crate::ids::HostId(9)));
        shop.add_line("firewall", 1500, 1, 1, 1, 1200);
        w.place_shop(8, 8, shop);
        // Needs met, broke: still settled, no goal named.
        assert!(w.advise(EntityId(1)).starts_with("Settled"));
        // Halfway there: save-up objective with price and benefit.
        w.set_needs(EntityId(1), 80, 70, 800, 3);
        let line = w.advise(EntityId(1));
        assert!(
            line.contains("Save up")
                && line.contains("$15.00")
                && line.contains("inbound traces crawl"),
            "{line}"
        );
        // Affordable: buy objective.
        w.set_needs(EntityId(1), 80, 70, 2000, 3);
        let line = w.advise(EntityId(1));
        assert!(line.contains("Buy the firewall"), "{line}");
        // Installed: goal retires back to settled.
        w.ensure_host(crate::ids::HostId(1)).set_firewall(true);
        assert!(w.advise(EntityId(1)).starts_with("Settled"));
    }

    #[test]
    fn rent_charges_once_per_day_and_allows_debt() {
        let mut w = World::new(1);
        w.spawn(EntityId(1), pos(8, 8), 10);
        // Day 0: acts cost no rent.
        w.tick = w.next_at(EntityId(1));
        w.apply(&Intent {
            actor: EntityId(1),
            verb: Verb::Wait,
        })
        .unwrap();
        assert_eq!(w.needs(EntityId(1)).unwrap().2, 200);
        // Day 1 boundary: exactly one charge across several acts.
        w.tick = DAY_TICKS;
        w.tick = w.next_at(EntityId(1)).max(w.tick);
        w.apply(&Intent {
            actor: EntityId(1),
            verb: Verb::Wait,
        })
        .unwrap();
        assert_eq!(w.needs(EntityId(1)).unwrap().2, 0);
        w.tick = w.next_at(EntityId(1));
        w.apply(&Intent {
            actor: EntityId(1),
            verb: Verb::Wait,
        })
        .unwrap();
        assert_eq!(w.needs(EntityId(1)).unwrap().2, 0);
        // Day 2 with thin pocket: debt allowed, advise names it.
        w.tick = DAY_TICKS * 2;
        w.tick = w.next_at(EntityId(1)).max(w.tick);
        w.apply(&Intent {
            actor: EntityId(1),
            verb: Verb::Wait,
        })
        .unwrap();
        assert_eq!(w.needs(EntityId(1)).unwrap().2, -200);
        w.set_needs(EntityId(1), 80, 70, -200, 3);
        assert!(w.advise(EntityId(1)).contains("In debt"));
        assert_eq!(w.ledger.live_sum(), w.ledger.audit_sum());
    }

    #[test]
    fn shuttered_counters_refuse_but_morning_serves() {
        use crate::shop::Shop;
        let mut w = World::new(3);
        w.spawn(EntityId(1), pos(8, 8), 1);
        let acct = w.open_account(EntityId(1), crate::ids::HostId(2)).unwrap();
        w.ledger.mint(acct, 5000).unwrap();
        let mut shop = Shop::new(w.ledger.open(EntityId(9), crate::ids::HostId(9)));
        shop.add_line("ration", 100, 5, 2, 5, 50);
        shop.set_hours(8, 20);
        w.place_shop(8, 8, shop);
        // 23:00: shuttered, money untouched.
        w.tick = 23 * 60;
        w.tick = w.next_at(EntityId(1)).max(w.tick);
        assert!(w
            .apply(&Intent {
                actor: EntityId(1),
                verb: Verb::Buy("ration".into()),
            })
            .is_err());
        assert_eq!(w.needs(EntityId(1)).unwrap().2, 200);
        // 09:00: open for business.
        w.tick = 9 * 60;
        w.tick = w.next_at(EntityId(1)).max(w.tick);
        w.apply(&Intent {
            actor: EntityId(1),
            verb: Verb::Buy("ration".into()),
        })
        .unwrap();
        assert_eq!(w.needs(EntityId(1)).unwrap().3, 4);
    }

    #[test]
    fn sleep_rests_to_morning_without_needs_cost() {
        let mut w = World::new(1);
        w.spawn(EntityId(1), pos(8, 8), 10);
        // No room on record: homeless.
        w.tick = w.next_at(EntityId(1));
        assert!(w
            .apply(&Intent {
                actor: EntityId(1),
                verb: Verb::Sleep,
            })
            .is_err());
        w.set_home(pos(8, 8));
        // Away from home: refused.
        w.beings.get_mut(&1).unwrap().pos = pos(9, 8);
        w.tick = w.next_at(EntityId(1));
        assert!(w
            .apply(&Intent {
                actor: EntityId(1),
                verb: Verb::Sleep,
            })
            .is_err());
        // Home at 16:40: rest to next 06:00. Align the clock first so
        // frozen-chunk catch-up doesn't confound the slept hours.
        w.beings.get_mut(&1).unwrap().pos = pos(8, 8);
        w.set_needs(EntityId(1), 80, 70, 200, 3);
        w.tick = 16 * 60 + 40;
        w.beings.get_mut(&1).unwrap().next_at = w.tick;
        w.apply(&Intent {
            actor: EntityId(1),
            verb: Verb::Sleep,
        })
        .unwrap();
        assert_eq!((w.tick - 1) % 1440, 360);
        let (food, mood, _, _) = w.needs(EntityId(1)).unwrap();
        assert_eq!((food, mood), (79, 69));
    }

    #[test]
    fn damage_wire_cuts_route_and_refuses_laser() {
        use crate::net::{LinkKind, NetKind};
        let mut w = World::new(3);
        w.spawn(EntityId(1), pos(8, 8), 1);
        w.net.add_node(HostId(1), NetKind::Oldnet, 10, false);
        w.net.add_node(HostId(2), NetKind::Oldnet, 100, true);
        w.net.add_node(HostId(3), NetKind::Betanet, 50, false);
        w.net
            .add_link(
                HostId(1),
                HostId(2),
                LinkKind::Wired { damaged: false },
                5,
                0,
            )
            .unwrap();
        w.net
            .add_link(HostId(2), HostId(3), LinkKind::Laser, 40, 10)
            .unwrap();
        assert!(w.net.route(HostId(1), HostId(3)).is_ok());
        w.tick = w.next_at(EntityId(1));
        let (_, events) = w
            .apply(&Intent {
                actor: EntityId(1),
                verb: Verb::DamageWire(HostId(1), HostId(2)),
            })
            .unwrap();
        assert_eq!(events.last().map(|e| e.kind.as_str()), Some("damaged"));
        // Host 1 is isolated now; laser links stay untouchable.
        assert!(w.net.route(HostId(1), HostId(3)).is_err());
        w.tick = w.next_at(EntityId(1));
        let err = w
            .apply(&Intent {
                actor: EntityId(1),
                verb: Verb::DamageWire(HostId(2), HostId(3)),
            })
            .unwrap_err();
        assert_eq!(err.kind, "untouchable");
        assert_eq!(w.ledger.live_sum(), w.ledger.audit_sum());
    }

    #[test]
    fn deck_load_strike_run_retaliation_in_act() {
        use crate::ids::HostId;
        let mut w = World::new(3);
        w.spawn(EntityId(1), pos(8, 8), 1);
        // Defended rig like the demo world: warden owner + monitor.
        let range = w.ensure_host(HostId(1));
        range.set_owner(Some(crate::deck::Host::WARDEN));
        range.set_monitor(true);
        // Empty cell refuses.
        w.tick = w.next_at(EntityId(1));
        let err = w
            .apply(&Intent {
                actor: EntityId(1),
                verb: Verb::DeckStrike { x: 5, y: 5 },
            })
            .unwrap_err();
        assert_eq!(err.kind, "no_target");
        // Load a killer: act kind loaded, trace opens on the intruder.
        w.tick = w.next_at(EntityId(1));
        let (_, events) = w
            .apply(&Intent {
                actor: EntityId(1),
                verb: Verb::DeckLoad { x: 5, y: 5 },
            })
            .unwrap();
        let kinds: Vec<&str> = events.iter().map(|e| e.kind.as_str()).collect();
        assert_eq!(kinds.first(), Some(&"loaded"));
        assert!(kinds.contains(&"traced"), "kinds: {kinds:?}");
        assert!(!w.funnel_line().contains("traced=-"));
        // Strike the head: kill reported, hack marked, ledger untouched.
        w.tick = w.next_at(EntityId(1));
        let (_, events) = w
            .apply(&Intent {
                actor: EntityId(1),
                verb: Verb::DeckStrike { x: 5, y: 5 },
            })
            .unwrap();
        let kinds: Vec<&str> = events.iter().map(|e| e.kind.as_str()).collect();
        assert_eq!(kinds.first(), Some(&"struck"));
        assert!(kinds.contains(&"kill"), "kinds: {kinds:?}");
        assert!(!w.funnel_line().contains("hack=-"));
        assert_eq!(w.ledger.live_sum(), w.ledger.audit_sum());

        // Retaliation runs inside the load act: a program dropped next to
        // the warden's spawn (first free cell) is killed the same beat.
        let mut w2 = World::new(3);
        w2.spawn(EntityId(1), pos(8, 8), 1);
        let r2 = w2.ensure_host(HostId(1));
        r2.set_owner(Some(crate::deck::Host::WARDEN));
        r2.set_monitor(true);
        w2.tick = w2.next_at(EntityId(1));
        let (_, ev2) = w2
            .apply(&Intent {
                actor: EntityId(1),
                verb: Verb::DeckLoad { x: 1, y: 0 },
            })
            .unwrap();
        let k2: Vec<&str> = ev2.iter().map(|e| e.kind.as_str()).collect();
        assert!(
            k2.contains(&"warden_kill"),
            "adjacent load should die in-act; kinds: {k2:?}"
        );
    }

    #[test]
    fn veil_enter_move_seize_eject_flow() {
        let mut w = World::new(1);
        w.spawn(EntityId(1), pos(8, 8), 1);
        w.veil.add_area(1, "lobby", vec![2], false);
        w.veil.add_area(2, "ops", vec![1, 3], false);
        w.veil.add_area(3, "roof", vec![2], true);
        // Weak mind refused.
        w.tick = w.next_at(EntityId(1));
        let err = w
            .apply(&Intent {
                actor: EntityId(1),
                verb: Verb::VeilEnter { mp: 2 },
            })
            .unwrap_err();
        assert_eq!(err.kind, "weak_mind");
        // Jack in, walk an exit link, seize, eject.
        for (verb, kind) in [
            (Verb::VeilEnter { mp: 8 }, "entered"),
            (Verb::VeilMove { area: 2 }, "moved"),
            (Verb::VeilSeize, "seized"),
            (Verb::VeilEject, "ejected"),
        ] {
            w.tick = w.next_at(EntityId(1));
            let (_, events) = w
                .apply(&Intent {
                    actor: EntityId(1),
                    verb,
                })
                .unwrap();
            assert_eq!(events.first().map(|e| e.kind.as_str()), Some(kind));
        }
        assert_eq!(w.veil.area_of(EntityId(1)), None);
        // Unlinked jump refused (fresh token first).
        w.tick = w.next_at(EntityId(1));
        w.apply(&Intent {
            actor: EntityId(1),
            verb: Verb::VeilEnter { mp: 8 },
        })
        .unwrap();
        w.tick = w.next_at(EntityId(1));
        let err = w
            .apply(&Intent {
                actor: EntityId(1),
                verb: Verb::VeilMove { area: 3 },
            })
            .unwrap_err();
        assert_eq!(err.kind, "no_exit");
    }

    #[test]
    fn bank_balance_visible_and_moves_with_deposit_withdraw() {
        let mut w = World::new(1);
        w.spawn(EntityId(1), pos(8, 8), 1);
        assert_eq!(w.bank_balance(EntityId(1)), None); // no account yet
        let acct = w.open_account(EntityId(1), HostId(2)).unwrap();
        assert_eq!(w.bank_balance(EntityId(1)), Some(0));
        let (_, _, pocket0, _) = w.needs(EntityId(1)).unwrap();
        w.tick = w.next_at(EntityId(1));
        w.apply(&Intent {
            actor: EntityId(1),
            verb: Verb::Deposit(150),
        })
        .unwrap();
        let (_, _, pocket1, _) = w.needs(EntityId(1)).unwrap();
        assert_eq!(pocket0 - pocket1, 150);
        assert_eq!(w.bank_balance(EntityId(1)), Some(150));
        assert_eq!(w.account(EntityId(1)), Some(acct));
        w.tick = w.next_at(EntityId(1));
        w.apply(&Intent {
            actor: EntityId(1),
            verb: Verb::Withdraw(50),
        })
        .unwrap();
        assert_eq!(w.bank_balance(EntityId(1)), Some(100));
        assert_eq!(w.ledger.live_sum(), w.ledger.audit_sum());
    }

    #[test]
    fn accounts_open_for_beings_and_npcs() {
        let mut w = World::new(1);
        w.spawn(EntityId(1), pos(8, 8), 1);
        w.spawn_npc(EntityId(10), Role::Shopkeeper, (3, 3), (12, 4), (8, 2), 1);
        assert!(w.open_account(EntityId(1), crate::ids::HostId(1)).is_ok());
        assert!(w.open_account(EntityId(10), crate::ids::HostId(1)).is_ok());
        assert!(w.open_account(EntityId(99), crate::ids::HostId(1)).is_err());
    }

    #[test]
    fn jobs_override_from_data_file() {
        use crate::data::parse_jobs;
        let defs = parse_jobs(include_str!("../../data/jobs.yaml")).unwrap();
        let mut w = World::new(1);
        w.set_jobs(&defs);
        w.spawn(EntityId(1), pos(8, 8), 1);
        let (_, _, m0, _) = w.needs(EntityId(1)).unwrap();
        let _ = w.apply(&Intent {
            actor: EntityId(1),
            verb: Verb::Work(1),
        });
        let (_, _, m1, _) = w.needs(EntityId(1)).unwrap();
        assert_eq!(m1, m0 + 120); // file value, not code constant
                                  // Old snapshots without a jobs table still load (serde default).
        let mut w2 = World::load_json(r#"{"tick":0,"seed":1,"beings":{}}"#).unwrap();
        w2.spawn(EntityId(9), pos(8, 8), 1);
        let (_, _, n0, _) = w2.needs(EntityId(9)).unwrap();
        let _ = w2.apply(&Intent {
            actor: EntityId(9),
            verb: Verb::Work(0),
        });
        assert_eq!(w2.needs(EntityId(9)).unwrap().2, n0 + 60);
    }
}

use crate::civic::{cop_duty, role_table, Role};
use crate::crime::{trace_evidence, Act, Alignment, Clans, Crime, License, Record, Standing};
use crate::deck::{CellPos, Host};
use crate::error::SimError;
use crate::ids::{EntityId, HostId};
use crate::ledger::{Ledger, Mail, Mailbox};
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
    /// Cut or restore a street section's power (design/02). Killing a
    /// section drops its anchored hosts from Net routing; restoring
    /// heals it. Logged like any act so replays reproduce the reroute.
    SetPower {
        sx: i32,
        sy: i32,
        up: bool,
    },
    /// Manual coding (design/10): write one unit of a resource into your
    /// rig's stock. Slow — one unit per act, plus a mood cost — so jobs
    /// push you onto producer programs.
    Code {
        res: crate::deck::Res,
    },
    /// Deploy a producer program on your rig (design/10: creatures
    /// auto-produce). Loads at the cell, marks it a maker for `res`, and
    /// then it yields its rate every successful act, free.
    DeckFarm {
        x: u8,
        y: u8,
        res: crate::deck::Res,
    },
    /// Play a card from the closed table (design/03) at a cell. Logged like
    /// any act, so replays reproduce dice and effects.
    DeckCard {
        card: String,
        x: u8,
        y: u8,
    },
    /// Scrub your transit trail on the rig (design/03 "get out clean").
    /// Needs a program on the host, or a privilege escalation there.
    ScrubLog,
    /// Toggle a transit firewall filter: `host` blocks traffic from
    /// `source` (design/04 filters). Logged like any act.
    SetFirewall {
        host: HostId,
        source: HostId,
        on: bool,
    },
    /// Exploit a vuln on the rig (design/03). Needs one of your programs
    /// there as the delivery vehicle; the vuln must be unpatched/unburned.
    Exploit {
        vuln: String,
    },
    /// Send an email trojan to a being (design/03): it is an attachment
    /// that runs when the victim opens it, so no breach is needed — but
    /// crafting one requires a Mailer+.
    SendTrojan {
        to: EntityId,
    },
    /// Send plain mail to a being (design/04).
    SendMail {
        to: EntityId,
        body: String,
    },
    /// Craft a hack-enabled Mailer+ (design/03), needed to attach trojans.
    /// Costs algo components from your rig.
    CraftMailerPlus,
    /// Open the mail at `index` in your inbox. A trojan attachment runs on
    /// open, enslaving the reader's hosts for the planter.
    OpenMail {
        index: usize,
    },
    /// Settle with the city (design/05): pay a fine proportional to the
    /// record's evidence mass and clear it, ending any pursuit.
    PayFine,
    /// Build a persistent ability into the program at a cell (design/03
    /// `Program.mods`), paying the card's resource cost.
    DeckMod {
        x: u8,
        y: u8,
        card: String,
    },
    /// Apply for a license to hack (design/05): 3 threat kills + a clean
    /// record. Once held, entering honest machines to fight intruders is
    /// legal — but hitting a defender revokes it with a fresh charge.
    ApplyLicense,
    /// Join a clan (design/05 M4b): pays the fee + initiation. Hitting a
    /// clanmate afterwards makes you its enemy for life.
    JoinClan {
        clan: u64,
    },
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
/// Chebyshev range at which an NPC witnesses an illicit act (design/05).
const WITNESS_RANGE: i32 = 5;
/// Cents per severity-evidence point when settling a fine (design/05).
const FINE_PER_MASS: i64 = 100;
/// Clan dues + initiation (design/05 M4b).
const CLAN_FEE: i64 = 500;
/// Algo units to craft a Mailer+ (design/03: trojans need one to craft).
const MAILER_PLUS_COST_ALGO: u32 = 3;
/// Manual coding cost (design/10): one unit per act at a small mood price,
/// so deck producers (free per act) are the real answer for higher tiers.
const CODE_MOOD_COST: u8 = 2;

/// Producer tier (design/10: "higher tier = faster but hungrier for
/// CPU/RAM"): (max_len, cpu_per_seg, units per act). A fresh producer is a
/// single cell, so it costs `1 + cpu_per_seg` CPU: 2 / 3 / 5 respectively.
fn farm_params(res: crate::deck::Res) -> (u8, u8, u8) {
    match res {
        crate::deck::Res::Loop => (2, 1, 3),
        crate::deck::Res::Data => (3, 2, 2),
        crate::deck::Res::Algo => (4, 4, 2),
    }
}
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
    /// Per-job resource requirement, parallel to `jobs`: (resource index
    /// 0 loop / 1 data / 2 algo, units). Default for old saves (design/10
    /// economy link); a missing entry means the job needs no units.
    #[serde(default = "default_job_reqs")]
    job_reqs: Vec<(u8, u32)>,
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
    /// Email-trojan zombie net (design/03): hosts enslaved by the player,
    /// each spamming for capped income per game day. Default empty.
    #[serde(default)]
    zombies: crate::mailer::ZombieNet,
    /// Last game day the zombie net paid out. Default 0 for old saves.
    #[serde(default)]
    spam_day: u64,
    /// Conduct record (design/05 M4): evidence-weighted crimes. Alignment
    /// and standing are pure functions of this, so editing it changes who
    /// the city thinks you are. Default empty for old saves.
    #[serde(default)]
    record: Record,
    /// Clan membership + enemies (design/05 M4b). Default empty.
    #[serde(default)]
    clans: Clans,
    /// License to hack (design/05 M4b). Default unearned.
    #[serde(default)]
    license: License,
    /// City/authority account fines are paid into (lazily opened, reserved
    /// system id); default None for old saves.
    #[serde(default)]
    city: Option<u64>,
    /// Card play costs: card id -> (resource index, units). Defaults mirror
    /// data/cards.yaml; `set_cards` overrides from the file. Default empty
    /// table would make cards free, so old saves get the code defaults.
    #[serde(default = "default_card_costs")]
    card_costs: BTreeMap<String, (u8, u32)>,
    /// Per-entity mailboxes (design/04: inbox/outbox capped at 20). Default
    /// empty for old saves.
    #[serde(default)]
    mailboxes: BTreeMap<u64, Mailbox>,
    /// The player holds a hack-enabled Mailer+ (design/03), required to
    /// attach a trojan to mail. Default false.
    #[serde(default)]
    mailer_plus: bool,
}

/// Card costs matching data/cards.yaml (design/03: cards consume to fire).
/// Attacks (spike/overload) fund their own damage and farm produces, so
/// those are free; everything else costs 1-2 units.
fn default_card_costs() -> BTreeMap<String, (u8, u32)> {
    // resource index: 0 loop, 1 data, 2 algo
    let rows: [(&str, u8, u32); 16] = [
        ("distract", 0, 1),
        ("freakout", 0, 1),
        ("stun", 0, 2),
        ("jam", 0, 1),
        ("hex", 0, 1),
        ("decoy", 0, 1),
        ("downgrade", 0, 1),
        ("shuffle", 1, 1),
        ("tank", 0, 1),
        ("plating", 0, 2),
        ("patch", 1, 1),
        ("reflect", 0, 2),
        ("splash", 0, 2),
        ("extra", 1, 1),
        ("load", 1, 1),
        ("upgrade", 1, 2),
    ];
    rows.iter()
        .map(|(id, r, n)| (id.to_string(), (*r, *n)))
        .collect()
}

fn default_jobs() -> Vec<(i64, u8, u8)> {
    DEFAULT_JOBS.to_vec()
}

/// Default unit requirements, parallel to DEFAULT_JOBS: 4/6 loop,
/// 5/6 data, 6 algo.
fn default_job_reqs() -> Vec<(u8, u32)> {
    vec![(0, 4), (0, 6), (1, 5), (1, 6), (2, 6)]
}

impl World {
    pub fn new(seed: u64) -> Self {
        Self {
            tick: 0,
            seed,
            version: SAVE_VERSION,
            beings: BTreeMap::new(),
            jobs: DEFAULT_JOBS.to_vec(),
            job_reqs: default_job_reqs(),
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
            zombies: crate::mailer::ZombieNet::new(),
            spam_day: 0,
            record: Record::new(),
            clans: Clans::new(),
            license: License::new(),
            city: None,
            card_costs: default_card_costs(),
            mailboxes: BTreeMap::new(),
            mailer_plus: false,
        }
    }

    /// Override the job table from data/jobs.yaml (validated by data::parse_jobs).
    pub fn set_jobs(&mut self, defs: &[crate::data::JobDef]) {
        if !defs.is_empty() {
            self.jobs = defs
                .iter()
                .map(|d| (d.payout_cents, d.mood_cost, d.food_cost))
                .collect();
            self.job_reqs = defs
                .iter()
                .map(|d| (d.resource_index().unwrap_or(0), d.units))
                .collect();
        }
    }

    /// What job `j` consumes, if anything: (resource, units).
    pub fn job_requirement(&self, j: usize) -> Option<(crate::deck::Res, u32)> {
        let (idx, units) = *self.job_reqs.get(j)?;
        if units == 0 {
            return None;
        }
        Some((crate::deck::Res::from_index(idx), units))
    }

    /// Override the card cost table from data/cards.yaml (design/11 rule 4).
    /// Cards with no cost (attacks, farm) stay free.
    pub fn set_cards(&mut self, table: &crate::data::CardTable) {
        let mut m = BTreeMap::new();
        for c in table
            .negative
            .iter()
            .chain(&table.positive)
            .chain(&table.manipulation)
        {
            if c.cost_units > 0 {
                if let Some(idx) = c.cost_index() {
                    m.insert(c.id.clone(), (idx, c.cost_units));
                }
            }
        }
        self.card_costs = m;
    }

    /// What playing card `id` costs, if anything: (resource, units).
    pub fn card_cost(&self, id: &str) -> Option<(crate::deck::Res, u32)> {
        self.card_costs
            .get(id)
            .map(|(idx, n)| (crate::deck::Res::from_index(*idx), *n))
    }

    /// Units of `res` the owner holds in `host`'s stock (their rig).
    pub fn units_at(&self, host: HostId, owner: EntityId, res: crate::deck::Res) -> u32 {
        self.hosts
            .get(&host.0)
            .map(|h| h.stock(owner, res))
            .unwrap_or(0)
    }

    /// Units of `res` the owner holds on their rig (world host 1).
    pub fn units(&self, owner: EntityId, res: crate::deck::Res) -> u32 {
        self.units_at(HostId(1), owner, res)
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

    // ---- M4 conduct: record, alignment, standing (design/05) ----

    /// The protagonist's conduct record (evidence-weighted crimes).
    pub fn record(&self) -> &Record {
        &self.record
    }

    pub fn standing(&self) -> Standing {
        self.record.standing()
    }

    pub fn alignment(&self) -> Alignment {
        self.record.alignment()
    }

    pub fn record_mass(&self) -> f32 {
        self.record.mass()
    }

    /// Cents to settle the current record (fine quote). 0 when clean.
    pub fn fine_cost(&self) -> i64 {
        let m = self.record.mass();
        if m <= 0.0 {
            0
        } else {
            (m.ceil() as i64) * FINE_PER_MASS
        }
    }

    pub fn license(&self) -> &License {
        &self.license
    }

    pub fn clans(&self) -> &Clans {
        &self.clans
    }

    /// Most authoritative witness to `actor`'s act: Authority when a cop is
    /// in earshot, HonestPassive for any other bystander, else None (no
    /// honest eyes — the act leaves only a rumor).
    fn witness_for(&self, actor: EntityId) -> Option<Alignment> {
        let ap = self.pos_of(actor)?;
        let mut civilian = false;
        for (_, p, role) in self.npc_full() {
            if p.sx != ap.sx || p.sy != ap.sy {
                continue;
            }
            let d = (p.x as i32 - ap.x as i32)
                .abs()
                .max((p.y as i32 - ap.y as i32).abs());
            if d > WITNESS_RANGE {
                continue;
            }
            if role == Role::Cop {
                return Some(Alignment::Authority);
            }
            civilian = true;
        }
        if civilian {
            Some(Alignment::HonestPassive)
        } else {
            None
        }
    }

    /// File an illicit act. Evidence is full only when an honest/authority
    /// witness saw it and the perp is still traceable (live chain); an
    /// unwitnessed act leaves a rumor that barely moves alignment
    /// (design/05).
    fn report_crime(
        &mut self,
        actor: EntityId,
        act: Act,
        host: Option<HostId>,
        victim: Option<EntityId>,
        live_chain: bool,
    ) {
        let evidence = match self.witness_for(actor) {
            Some(witness) => trace_evidence(1.0, witness, live_chain),
            // No honest eyes: only a rumor survives.
            None => 0.1,
        };
        self.record.report(Crime {
            act,
            at_host: host,
            tick: self.tick,
            victim,
            evidence,
        });
    }

    /// A host is a sanctioned practice range when the warden sentinel owns
    /// it: intrusions there are training, not trespass.
    fn is_range(&self, host: HostId) -> bool {
        self.hosts
            .get(&host.0)
            .map(|h| h.owner() == Some(Host::WARDEN))
            .unwrap_or(false)
    }

    /// A host is foreign to `actor` when someone else owns it and it is not
    /// a range (unowned wild boxes stay quiet).
    fn is_foreign(&self, actor: EntityId, host: HostId) -> bool {
        match self.hosts.get(&host.0).and_then(|h| h.owner()) {
            Some(o) => o != actor && o != Host::WARDEN,
            None => false,
        }
    }

    // ---- M3c mail (design/03 trojans, design/04 mailboxes) ----

    /// The mailbox for a being or NPC (empty when none exists yet).
    pub fn mailbox(&self, id: EntityId) -> Option<&Mailbox> {
        self.mailboxes.get(&id.0)
    }

    /// The inbox for a being, cloned for clients.
    pub fn inbox(&self, id: EntityId) -> Vec<Mail> {
        self.mailboxes
            .get(&id.0)
            .map(|b| b.inbox.clone())
            .unwrap_or_default()
    }

    /// True when the player has crafted a Mailer+ (trojan crafting).
    pub fn has_mailer_plus(&self) -> bool {
        self.mailer_plus
    }

    /// True when `host` has been enslaved into some zombie net (design/03).
    pub fn is_zombie_host(&self, host: HostId) -> bool {
        self.zombies.is_zombie(host)
    }

    /// Deliver mail from `from` to `to`: a copy in the sender's outbox and
    /// the recipient's inbox (both capped at 20). `trojan` marks an
    /// attachment that runs on open.
    fn send_mail(
        &mut self,
        from: EntityId,
        to: EntityId,
        body: String,
        trojan: bool,
    ) -> Result<(), SimError> {
        if !self.beings.contains_key(&from.0) && !self.npcs.contains_key(&from.0) {
            return Err(SimError::new("unknown_actor", "sender", "missing"));
        }
        let m = Mail {
            from,
            to,
            body,
            tick: self.tick,
            trojan,
        };
        let outbox = self.mailboxes.entry(from.0).or_default();
        if outbox.outbox.len() >= crate::ledger::MAIL_CAP {
            return Err(SimError::new("outbox_full", "cap 20", "full"));
        }
        let inbox = self.mailboxes.entry(to.0).or_default();
        if inbox.inbox.len() >= crate::ledger::MAIL_CAP {
            return Err(SimError::new("mailbox_full", "cap 20", "full"));
        }
        self.mailboxes
            .entry(from.0)
            .or_default()
            .record_sent(m.clone())?;
        self.mailboxes.entry(to.0).or_default().deliver(m)?;
        Ok(())
    }

    /// NPCs read their mail when their turn comes up (design/04). A trojan
    /// attachment runs then, enslaving the reader's hosts for the planter.
    fn npc_open_mail(&mut self, npc: u64) {
        let Some(m) = self.mailboxes.get_mut(&npc).and_then(|b| b.open(0)) else {
            return;
        };
        if !m.trojan {
            return;
        }
        let reader = EntityId(npc);
        let hosts: Vec<u64> = self
            .hosts
            .iter()
            .filter(|(_, h)| h.owner() == Some(reader))
            .map(|(id, _)| *id)
            .collect();
        for h in hosts {
            self.zombies.enslave(HostId(h), m.from);
        }
    }

    /// Being-id -> standing for cop pursuit: only the protagonist is
    /// tracked, and only once Wanted.
    fn wanted_map(&self) -> BTreeMap<u64, Standing> {
        let mut m = BTreeMap::new();
        if self.record.standing() == Standing::Wanted {
            // Every being shares the single record in single-player.
            for id in self.beings.keys() {
                m.insert(*id, Standing::Wanted);
            }
        }
        m
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
            // Reading mail is part of an NPC's turn (design/04): a trojan
            // attachment runs here, enslaving the reader's hosts.
            self.npc_open_mail(id);
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
        // Wanted: the city is hunting you; settling ends the chase.
        if self.standing() == Standing::Wanted {
            let cost = self.fine_cost();
            return format!(
                "Wanted: pay the fine (${:.2}) or lie low — cops pursue on sight",
                cost as f64 / 100.0
            );
        }
        let pay0 = self.jobs.first().map(|j| j.0).unwrap_or(0);
        if b.money < pay0.max(100) {
            // Unit gate (design/10): the work advice is useless when the
            // first job's components are short, so name the real blocker
            // while still pointing at Work.
            if let Some((res, units)) = self.job_requirement(0) {
                let have = self.units(id, res);
                if have < units {
                    return format!(
                        "Cash low: Work needs {} {} components ({have}/{units}) — code {} first",
                        units,
                        res.name(),
                        res.name()
                    );
                }
            }
            return "Cash low: Work a job (Work 0/1)".to_string();
        }
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
        // Deck acts land on world host 1; computed before the being borrow
        // because it reads host ownership.
        let deck_foreign = self.is_foreign(intent.actor, HostId(1));
        let deck_range = self.is_range(HostId(1));
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
        // Email-trojan income (design/03): once per game day the zombie net
        // spams for capped credit into its owners' accounts.
        if day > self.spam_day {
            self.spam_day = day;
            self.zombies.payout_day(&mut self.ledger, day, &self.accts);
        }
        let mut kind = "acted";
        let mut extra_kinds: Vec<String> = Vec::new();
        // Crimes committed this act, filed after the match: `report_crime`
        // borrows the whole world, which the live `beings` borrow forbids.
        let mut crimes: Vec<(Act, Option<HostId>, Option<EntityId>, bool)> = Vec::new();
        // Mail sent this act, delivered after the match for the same
        // borrow reason as crimes.
        let mut mails: Vec<(EntityId, String, bool)> = Vec::new();
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
                // Units gate (design/10 economy link): the job consumes
                // components from your rig's stock before it pays. Check
                // before charging so a shortfall costs nothing but the act.
                // Field access (not self.job_requirement) keeps the `beings`
                // borrow alive, like the Buy arm.
                if let Some(&(idx, units)) = self.job_reqs.get(j as usize) {
                    if units > 0 {
                        let res = crate::deck::Res::from_index(idx);
                        // Same lazy-rig rule as `Code`: a missing rig is
                        // just an empty stock, not a separate failure.
                        let rig = self
                            .hosts
                            .entry(1)
                            .or_insert_with(|| Host::new(HostId(1), 12, 8, 100));
                        rig.consume(intent.actor, res, units)?;
                    }
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
                // Wire sabotage is a crime; you are on the net, so traceable.
                crimes.push((Act::Attack, None, None, true));
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
                // Loading onto a foreign, non-range rig is trespass.
                // A licensed hacker may enter honest machines to fight
                // intruders, so that entry is not trespass (design/05).
                if deck_foreign && !self.license.granted {
                    crimes.push((Act::Trespass, Some(HostId(1)), None, true));
                }
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
                // License bookkeeping (design/05): killing a range warden
                // is a sanctioned threat kill; killing a defender on a
                // foreign rig while licensed is abuse (which supersedes the
                // plain attack charge); anything else on a foreign rig is
                // an attack.
                if killed {
                    if deck_range {
                        self.license.add_kill();
                    } else if deck_foreign {
                        if self.license.granted {
                            let tick = self.tick;
                            self.license.abuse(&mut self.record, tick);
                        } else {
                            crimes.push((Act::Attack, Some(HostId(1)), None, true));
                        }
                    }
                }
                // Hitting a clanmate makes you that clan's enemy for life
                // (design/05 M4b): the rig's owner is the clan member.
                if killed {
                    if let Some(owner) = self.hosts.get(&1).and_then(|h| h.owner()) {
                        if let Some(clan) = self.clans.shared_clan(intent.actor, owner) {
                            self.clans.mark_enemy(intent.actor, clan);
                        }
                    }
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
                // Taking over a host reads as theft (design/05).
                crimes.push((Act::Theft, None, None, true));
            }
            Verb::VeilEject => {
                self.veil.eject(intent.actor)?;
                kind = "ejected";
            }
            Verb::SetPower { sx, sy, up } => {
                if up {
                    self.power.restore((sx, sy));
                    kind = "power_up";
                } else {
                    self.power.kill((sx, sy));
                    kind = "power_down";
                    // Sabotage is a crime: you are on the net, so traceable.
                    crimes.push((Act::Attack, None, None, true));
                }
            }
            Verb::Code { res } => {
                // Manual coding: one unit onto your rig, at a mood cost.
                let host = self
                    .hosts
                    .entry(1)
                    .or_insert_with(|| Host::new(HostId(1), 12, 8, 100));
                host.add_stock(intent.actor, res, 1);
                b.mood = b.mood.saturating_sub(CODE_MOOD_COST);
                kind = "coded";
            }
            Verb::SetFirewall { host, source, on } => {
                self.net.set_firewall_entry(host, source, on)?;
                kind = if on { "firewall_on" } else { "firewall_off" };
            }
            Verb::Exploit { ref vuln } => {
                // Delivery vehicle: one of the actor's programs on the rig.
                let host = self
                    .hosts
                    .get_mut(&1)
                    .ok_or(SimError::new("no_host", "rig", "missing"))?;
                let prog = host
                    .progs
                    .values()
                    .find(|p| p.owner == intent.actor)
                    .map(|p| p.id)
                    .ok_or(SimError::new("no_prog", "your program on host", "absent"))?;
                host.exploit(prog, vuln)?;
                kind = "exploited";
            }
            Verb::SendMail { ref to, ref body } => {
                // Peek capacity now (field access keeps the `beings` borrow);
                // the actual delivery happens after the match.
                let cap_ok = self
                    .mailboxes
                    .get(&intent.actor.0)
                    .map(|b| b.outbox.len() < crate::ledger::MAIL_CAP)
                    .unwrap_or(true)
                    && self
                        .mailboxes
                        .get(&to.0)
                        .map(|b| b.inbox.len() < crate::ledger::MAIL_CAP)
                        .unwrap_or(true);
                if !cap_ok {
                    return Err(SimError::new("mailbox_full", "cap 20", "full"));
                }
                mails.push((*to, body.clone(), false));
                kind = "mailed";
            }
            Verb::SendTrojan { to } => {
                // Trojans are email attachments and need a Mailer+ to
                // craft (design/03). No breach required: the victim runs it
                // when they open the mail.
                if !self.mailer_plus {
                    return Err(SimError::new("no_mailer_plus", "Mailer+", "stock mailer"));
                }
                let cap_ok = self
                    .mailboxes
                    .get(&intent.actor.0)
                    .map(|b| b.outbox.len() < crate::ledger::MAIL_CAP)
                    .unwrap_or(true)
                    && self
                        .mailboxes
                        .get(&to.0)
                        .map(|b| b.inbox.len() < crate::ledger::MAIL_CAP)
                        .unwrap_or(true);
                if !cap_ok {
                    return Err(SimError::new("mailbox_full", "cap 20", "full"));
                }
                mails.push((to, "invoice attached — open to review".to_string(), true));
                kind = "trojan_sent";
            }
            Verb::CraftMailerPlus => {
                // Crafting the hack-enabled mailer costs algo components.
                let host = self
                    .hosts
                    .entry(1)
                    .or_insert_with(|| Host::new(HostId(1), 12, 8, 100));
                host.consume(intent.actor, crate::deck::Res::Algo, MAILER_PLUS_COST_ALGO)?;
                self.mailer_plus = true;
                kind = "mailer_plus";
            }
            Verb::OpenMail { index } => {
                let mail = self
                    .mailboxes
                    .entry(intent.actor.0)
                    .or_default()
                    .open(index)
                    .ok_or(SimError::new("no_mail", "inbox entry", "empty"))?;
                if mail.trojan {
                    // The attachment runs: the reader's hosts join the
                    // planter's zombie net (design/03).
                    let reader = intent.actor.0;
                    let hosts: Vec<u64> = self
                        .hosts
                        .iter()
                        .filter(|(_, h)| h.owner() == Some(EntityId(reader)))
                        .map(|(id, _)| *id)
                        .collect();
                    for h in hosts {
                        self.zombies.enslave(HostId(h), mail.from);
                    }
                    kind = "trojan_opened";
                } else {
                    kind = "opened_mail";
                }
            }
            Verb::PayFine => {
                // Settle with the city: cost scales with evidence mass.
                let mass = self.record.mass();
                if mass <= 0.0 {
                    return Err(SimError::new("no_fine", "outstanding fine", "clean"));
                }
                let cost = (mass.ceil() as i64) * FINE_PER_MASS;
                let acct = self
                    .accts
                    .get(&intent.actor.0)
                    .copied()
                    .ok_or(SimError::new("no_account", "open account", "unbanked"))?;
                // Pocket-first, bank covers the rest (same bridge as Buy).
                let bank = self.ledger.balance(acct)?;
                let cash = b.money.min(cost);
                if bank < cost - cash {
                    return Err(SimError::new("overdraw", "funds", "short"));
                }
                if cash > 0 {
                    b.money -= cash;
                    self.ledger.mint(acct, cash)?;
                }
                let city = match self.city {
                    Some(id) => id,
                    None => {
                        let id = self.ledger.open(EntityId(u64::MAX - 2), HostId(1));
                        self.city = Some(id);
                        id
                    }
                };
                self.ledger.transfer(acct, city, cost)?;
                self.record.pardon_all();
                kind = "paid_fine";
            }
            Verb::ScrubLog => {
                // Get out clean (design/03): erase your trail on the rig.
                // A program on the host scrubs normally; a privilege
                // escalation there lets you scrub with nothing left behind.
                let host = self
                    .hosts
                    .get_mut(&1)
                    .ok_or(SimError::new("no_host", "rig", "missing"))?;
                let privileged = host.privileged();
                match host.scrub_log(intent.actor) {
                    Ok(_) => kind = "scrubbed",
                    Err(_) if privileged => {
                        host.clear_logs_for(intent.actor);
                        kind = "scrubbed";
                    }
                    Err(e) => return Err(e),
                }
            }
            Verb::DeckCard { ref card, x, y } => {
                use crate::deck::Fx;
                let actor = intent.actor;
                let hostile;
                // Card cost (design/03: cards consume to fire). Peek first,
                // apply the effect, then spend — so a refused effect never
                // charges the player. Field access (not self.card_cost)
                // keeps the `beings` borrow alive, like the Buy arm.
                let cost = self
                    .card_costs
                    .get(card.as_str())
                    .map(|(idx, n)| (crate::deck::Res::from_index(*idx), *n));
                {
                    let host = self
                        .hosts
                        .entry(1)
                        .or_insert_with(|| Host::new(HostId(1), 12, 8, 100));
                    let target = CellPos(x, y);
                    let victim = host.prog_at(x, y);
                    let need = || SimError::new("no_target", "program cell", "empty");
                    // Affordability check before any mutation.
                    if let Some((res, units)) = cost {
                        if host.stock(actor, res) < units {
                            return Err(SimError::new("no_resource", "card cost", "short"));
                        }
                    }
                    hostile = victim.is_some_and(|v| v != actor)
                        || matches!(card.as_str(), "spike" | "overload");
                    match card.as_str() {
                        // Attacks (roll-aware, resource-funded).
                        "spike" => {
                            host.card_attack(
                                actor,
                                actor,
                                target,
                                Some((crate::deck::Res::Loop, 2)),
                                None,
                            )?;
                        }
                        "overload" => {
                            host.card_attack(
                                actor,
                                actor,
                                target,
                                None,
                                Some(crate::deck::Res::Loop),
                            )?;
                        }
                        // Self buffs.
                        "tank" => host.apply_fx(victim.ok_or_else(need)?, Fx::ShieldNext(1))?,
                        "plating" => host.apply_fx(victim.ok_or_else(need)?, Fx::ShieldNext(2))?,
                        "reflect" => host.apply_fx(victim.ok_or_else(need)?, Fx::ReflectNext)?,
                        "splash" => host.apply_fx(victim.ok_or_else(need)?, Fx::SplashNext)?,
                        "extra" => host.apply_fx(victim.ok_or_else(need)?, Fx::ExtraRollNext)?,
                        // Debuffs on another program.
                        "distract" => host.apply_fx(victim.ok_or_else(need)?, Fx::SkipNext)?,
                        "freakout" => host.apply_fx(victim.ok_or_else(need)?, Fx::DotNext(1))?,
                        "stun" => host.apply_fx(victim.ok_or_else(need)?, Fx::DotNext(2))?,
                        "jam" => host.apply_fx(victim.ok_or_else(need)?, Fx::RollMinus(2))?,
                        "hex" => host.apply_fx(victim.ok_or_else(need)?, Fx::BanDie(6))?,
                        "decoy" => host.apply_fx(victim.ok_or_else(need)?, Fx::DecoyNext)?,
                        "downgrade" => {
                            host.apply_fx(victim.ok_or_else(need)?, Fx::DowngradeNext)?
                        }
                        "shuffle" => {
                            host.apply_fx(victim.ok_or_else(need)?, Fx::RandomTargetNext)?
                        }
                        // Manipulation / immediate.
                        "patch" => {
                            host.heal_lowest(victim.ok_or_else(need)?, 2)?;
                        }
                        "load" => {
                            host.grow(victim.ok_or_else(need)?)?;
                        }
                        "upgrade" => {
                            host.upgrade_max(victim.ok_or_else(need)?, 1)?;
                        }
                        "copy" => {
                            let src = victim.ok_or_else(need)?;
                            let id = EntityId(100 + host.progs.len() as u64);
                            let at = host.free_cell_for(actor).ok_or(SimError::new(
                                "blocked",
                                "free cell",
                                "full",
                            ))?;
                            host.clone_to(src, id, at)?;
                        }
                        "farm" => {
                            host.farm_once(actor);
                        }
                        _ => return Err(SimError::new("no_card", "known card", "unknown")),
                    }
                    // Effect landed: now charge the cost (the peek above
                    // guarantees this succeeds).
                    if let Some((res, units)) = cost {
                        host.consume(actor, res, units)?;
                    }
                }
                kind = "card";
                if hostile {
                    extra_kinds = Self::deck_beat(
                        self.hosts.get_mut(&1).unwrap(),
                        &mut self.funnel,
                        self.tick,
                        actor,
                        true,
                    );
                }
            }
            Verb::ApplyLicense => {
                self.license.apply(&self.record)?;
                kind = "licensed";
            }
            Verb::JoinClan { clan } => {
                // Dues + initiation (design/05 M4b): pay the fee into the
                // city pool (pocket-first), then join.
                let acct = self
                    .accts
                    .get(&intent.actor.0)
                    .copied()
                    .ok_or(SimError::new("no_account", "open account", "unbanked"))?;
                let bank = self.ledger.balance(acct)?;
                let cash = b.money.min(CLAN_FEE);
                if bank < CLAN_FEE - cash {
                    return Err(SimError::new("overdraw", "clan dues", "short"));
                }
                if cash > 0 {
                    b.money -= cash;
                    self.ledger.mint(acct, cash)?;
                }
                let city = match self.city {
                    Some(id) => id,
                    None => {
                        let id = self.ledger.open(EntityId(u64::MAX - 2), HostId(1));
                        self.city = Some(id);
                        id
                    }
                };
                self.ledger.transfer(acct, city, CLAN_FEE)?;
                self.clans.join(clan, intent.actor, true, true)?;
                kind = "joined_clan";
            }
            Verb::DeckMod { x, y, ref card } => {
                // Install a persistent mod on your program (design/03):
                // check the target and affordability, install, then pay.
                let m = crate::deck::Mod::from_card(card).ok_or(SimError::new(
                    "no_mod",
                    "moddable card",
                    "not a mod",
                ))?;
                let cost = self
                    .card_costs
                    .get(card.as_str())
                    .map(|(idx, n)| (crate::deck::Res::from_index(*idx), *n));
                let host = self
                    .hosts
                    .entry(1)
                    .or_insert_with(|| Host::new(HostId(1), 12, 8, 100));
                let prog = host.prog_at(x, y).ok_or(SimError::new(
                    "no_target",
                    "program cell",
                    "empty",
                ))?;
                if host.owner_of(prog) != Some(intent.actor) {
                    return Err(SimError::new("not_yours", "your program", "foreign"));
                }
                if let Some((res, units)) = cost {
                    if host.stock(intent.actor, res) < units {
                        return Err(SimError::new("no_resource", "mod cost", "short"));
                    }
                }
                host.add_mod(prog, m)?;
                if let Some((res, units)) = cost {
                    host.consume(intent.actor, res, units)?;
                }
                kind = "modded";
            }
            Verb::DeckFarm { x, y, res } => {
                let (max_len, cpu, rate) = farm_params(res);
                let id = {
                    let host = self
                        .hosts
                        .entry(1)
                        .or_insert_with(|| Host::new(HostId(1), 12, 8, 100));
                    EntityId(100 + host.progs.len() as u64)
                };
                let host = self.hosts.get_mut(&1).unwrap();
                host.load(id, intent.actor, CellPos(x, y), max_len, cpu)?;
                host.set_maker(id, vec![res], rate)?;
                // Farming is a build action on your own rig, not an
                // intrusion, so it draws no retaliation beat. Producers
                // are also skipped by the warden (see retaliate), so they
                // survive later DeckLoad/DeckStrike beats too.
                kind = "farmed";
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
        // File any crimes this act committed, now that the `beings` borrow
        // is released (the witness lookup reads NPC positions).
        for (act, host, victim, live_chain) in crimes {
            self.report_crime(intent.actor, act, host, victim, live_chain);
        }
        // Deliver mail queued above (capacity was checked before mutating).
        for (to, body, trojan) in mails {
            let _ = self.send_mail(intent.actor, to, body, trojan);
        }
        // Passive production (design/10): your rig's makers yield on every
        // successful act. In-act, so replay reproduces it exactly.
        if let Some(host) = self.hosts.get_mut(&1) {
            if !host.tick_makers(intent.actor).is_empty() {
                extra_kinds.push("produced".to_string());
            }
        }
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
        // Wanted beings draw cop pursuit (design/05): the record's standing
        // is the only source, so a paid-off record ends the chase.
        let wanted = self.wanted_map();
        self.step_npcs(hour, &wanted);
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
        // Stock the rig so job 0's unit gate is satisfied.
        w.ensure_host(HostId(1))
            .add_stock(EntityId(1), crate::deck::Res::Loop, 4);
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
        // Job 0 needs loop units; stock the rig first (design/10 gate).
        w.ensure_host(HostId(1))
            .add_stock(EntityId(1), crate::deck::Res::Loop, 4);
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
    fn advise_names_unit_shortage_when_cash_is_low() {
        let mut w = World::new(1);
        w.spawn(EntityId(1), pos(8, 8), 1);
        // Cash low + no components: the work advice names the real blocker.
        w.set_needs(EntityId(1), 80, 70, 10, 3);
        let line = w.advise(EntityId(1));
        assert!(
            line.contains("Work needs 4 loop components (0/4)") && line.contains("code loop"),
            "{line}"
        );
        // Stock the rig: the plain work advice returns.
        w.ensure_host(HostId(1))
            .add_stock(EntityId(1), crate::deck::Res::Loop, 4);
        assert!(w.advise(EntityId(1)).contains("Cash low: Work a job"));
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
    fn set_power_reroutes_and_restores_through_apply() {
        use crate::net::{LinkKind, NetKind};
        let mut w = World::new(3);
        w.spawn(EntityId(1), pos(8, 8), 1);
        // Triangle: 1-2-3 direct via 2, plus a longer 1-3 fallback.
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
        w.net
            .add_link(
                HostId(1),
                HostId(3),
                LinkKind::Wired { damaged: false },
                100,
                0,
            )
            .unwrap();
        w.power.anchor(HostId(2), (1, 0));
        // Direct path uses transit host 2.
        assert_eq!(
            w.power
                .powered_route(&w.net, HostId(1), HostId(3))
                .unwrap()
                .path,
            vec![HostId(1), HostId(2), HostId(3)]
        );
        w.tick = w.next_at(EntityId(1));
        let (_, events) = w
            .apply(&Intent {
                actor: EntityId(1),
                verb: Verb::SetPower {
                    sx: 1,
                    sy: 0,
                    up: false,
                },
            })
            .unwrap();
        assert_eq!(events.last().map(|e| e.kind.as_str()), Some("power_down"));
        assert_eq!(w.power.dark_sections(), vec![(1, 0)]);
        // Host 2 is dark, so the powered route takes the long way round.
        assert_eq!(
            w.power
                .powered_route(&w.net, HostId(1), HostId(3))
                .unwrap()
                .path,
            vec![HostId(1), HostId(3)]
        );
        w.tick = w.next_at(EntityId(1));
        let (_, events) = w
            .apply(&Intent {
                actor: EntityId(1),
                verb: Verb::SetPower {
                    sx: 1,
                    sy: 0,
                    up: true,
                },
            })
            .unwrap();
        assert_eq!(events.last().map(|e| e.kind.as_str()), Some("power_up"));
        assert!(w.power.dark_sections().is_empty());
        assert_eq!(
            w.power
                .powered_route(&w.net, HostId(1), HostId(3))
                .unwrap()
                .path,
            vec![HostId(1), HostId(2), HostId(3)]
        );
        assert_eq!(w.ledger.live_sum(), w.ledger.audit_sum());
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
    fn pay_fine_settles_the_record_and_ends_pursuit() {
        use crate::crime::{Act, Crime, Standing};
        let mut w = World::new(1);
        w.spawn(EntityId(1), pos(8, 8), 1);
        let acct = w.open_account(EntityId(1), HostId(1)).unwrap();
        w.ledger.mint(acct, 2000).unwrap();
        // Nothing owed while clean.
        assert_eq!(w.fine_cost(), 0);
        w.tick = w.next_at(EntityId(1));
        let err = w
            .apply(&Intent {
                actor: EntityId(1),
                verb: Verb::PayFine,
            })
            .unwrap_err();
        assert_eq!(err.kind, "no_fine");
        // Wanted record (mass 8): fine quote is 800c.
        for _ in 0..2 {
            w.record.report(Crime {
                act: Act::Attack,
                at_host: None,
                tick: 1,
                victim: None,
                evidence: 1.0,
            });
        }
        assert_eq!(w.standing(), Standing::Wanted);
        assert_eq!(w.fine_cost(), 800);
        let bank_before = w.bank_balance(EntityId(1)).unwrap();
        let pocket_before = w.needs(EntityId(1)).unwrap().2;
        w.tick = w.next_at(EntityId(1));
        let (_, events) = w
            .apply(&Intent {
                actor: EntityId(1),
                verb: Verb::PayFine,
            })
            .unwrap();
        assert_eq!(events.last().map(|e| e.kind.as_str()), Some("paid_fine"));
        // Record cleared: clean standing, no more pursuit.
        assert_eq!(w.standing(), Standing::Clean);
        assert_eq!(w.record_mass(), 0.0);
        // Pocket-first then bank, and the ledger stays closed.
        let spent = (pocket_before - w.needs(EntityId(1)).unwrap().2)
            + (bank_before - w.bank_balance(EntityId(1)).unwrap());
        assert_eq!(spent, 800, "fine paid exactly");
        assert_eq!(w.ledger.live_sum(), w.ledger.audit_sum());
        // Advise no longer warns about pursuit.
        assert!(!w.advise(EntityId(1)).contains("Wanted"));
        // Too poor to settle: refused without moving money.
        w.record.report(Crime {
            act: Act::Murder,
            at_host: None,
            tick: 1,
            victim: None,
            evidence: 1.0,
        });
        w.set_needs(EntityId(1), 80, 70, 0, 3);
        let mut poor = World::new(2);
        poor.spawn(EntityId(1), pos(8, 8), 1);
        poor.open_account(EntityId(1), HostId(1)).unwrap();
        poor.record.report(Crime {
            act: Act::Murder,
            at_host: None,
            tick: 1,
            victim: None,
            evidence: 1.0,
        });
        poor.tick = poor.next_at(EntityId(1));
        let err = poor
            .apply(&Intent {
                actor: EntityId(1),
                verb: Verb::PayFine,
            })
            .unwrap_err();
        assert_eq!(err.kind, "overdraw");
        assert_eq!(poor.record_mass(), 8.0, "record untouched when broke");
    }

    #[test]
    fn sabotage_reports_crime_gated_by_witness_and_reaches_wanted() {
        use crate::crime::{Act, Standing};
        // Net with one wired link to sabotage.
        let net_world = |with_cop: bool| {
            let mut w = World::new(1);
            w.spawn(EntityId(1), pos(8, 8), 1);
            w.net
                .add_node(HostId(1), crate::net::NetKind::Oldnet, 10, false);
            w.net
                .add_node(HostId(2), crate::net::NetKind::Oldnet, 100, true);
            w.net
                .add_link(
                    HostId(1),
                    HostId(2),
                    crate::net::LinkKind::Wired { damaged: false },
                    5,
                    0,
                )
                .unwrap();
            if with_cop {
                // A cop two tiles away witnesses the act in full.
                w.spawn_npc(EntityId(11), Role::Cop, (10, 8), (10, 8), (10, 8), 15);
            }
            w
        };
        // Unobserved: only a rumor, so mass stays tiny.
        let mut solo = net_world(false);
        solo.tick = solo.next_at(EntityId(1));
        solo.apply(&Intent {
            actor: EntityId(1),
            verb: Verb::DamageWire(HostId(1), HostId(2)),
        })
        .unwrap();
        assert_eq!(solo.record().len(), 1, "rumor entry filed");
        assert!(
            solo.record_mass() < 1.0,
            "rumor mass {}",
            solo.record_mass()
        );
        assert_eq!(solo.standing(), Standing::Fined);

        // Witnessed by a cop: full evidence. Two Attacks reach Wanted.
        let mut watched = net_world(true);
        for _ in 0..2 {
            watched.tick = watched.next_at(EntityId(1));
            // Re-damage needs a fresh link each time; repair then hit again.
            watched.net.repair_wire(HostId(1), HostId(2)).unwrap();
            watched
                .apply(&Intent {
                    actor: EntityId(1),
                    verb: Verb::DamageWire(HostId(1), HostId(2)),
                })
                .unwrap();
        }
        assert_eq!(watched.record().len(), 2);
        assert_eq!(watched.record_mass(), 8.0, "two full-evidence attacks");
        assert_eq!(watched.standing(), Standing::Wanted);
        assert_ne!(watched.alignment(), crate::crime::Alignment::HonestActive);
        // The crime's act/host are recorded for evidence trails.
        assert_eq!(watched.record().len(), 2);
        let _ = Act::Attack;
    }

    #[test]
    fn deck_intrusion_counts_only_on_foreign_rigs() {
        // Range rig (warden-owned): practice draws no record entry.
        let mut range = World::new(1);
        range.spawn(EntityId(1), pos(8, 8), 1);
        range.ensure_host(HostId(1)).set_owner(Some(Host::WARDEN));
        range.tick = range.next_at(EntityId(1));
        range
            .apply(&Intent {
                actor: EntityId(1),
                verb: Verb::DeckLoad { x: 0, y: 0 },
            })
            .unwrap();
        assert_eq!(range.record().len(), 0, "range practice is not trespass");

        // Foreign rig: loading is trespass, killing is an attack.
        let mut foreign = World::new(1);
        foreign.spawn(EntityId(1), pos(8, 8), 1);
        let host = foreign.ensure_host(HostId(1));
        host.set_owner(Some(EntityId(11)));
        host.load(EntityId(50), EntityId(11), CellPos(5, 5), 1, 1)
            .unwrap();
        foreign.tick = foreign.next_at(EntityId(1));
        foreign
            .apply(&Intent {
                actor: EntityId(1),
                verb: Verb::DeckLoad { x: 0, y: 0 },
            })
            .unwrap();
        assert_eq!(foreign.record().len(), 1, "trespass filed");
        foreign.tick = foreign.next_at(EntityId(1));
        foreign
            .apply(&Intent {
                actor: EntityId(1),
                verb: Verb::DeckStrike { x: 5, y: 5 },
            })
            .unwrap();
        assert_eq!(foreign.record().len(), 2, "attack filed on kill");
    }

    #[test]
    fn wanted_record_draws_cop_pursuit() {
        use crate::crime::{Act, Crime, Standing};
        let mut w = World::new(1);
        w.spawn(EntityId(1), pos(8, 8), 1);
        // A cop posted far away.
        w.spawn_npc(EntityId(11), Role::Cop, (2, 2), (2, 2), (2, 2), 15);
        let start = w.npc_full()[0].1;
        // Clean: the cop follows its timetable, not the player.
        w.tick = w.next_at(EntityId(1));
        w.apply(&Intent {
            actor: EntityId(1),
            verb: Verb::Wait,
        })
        .unwrap();
        assert_eq!(w.standing(), Standing::Clean);
        // Make the player Wanted via the real record path.
        for _ in 0..2 {
            w.record.report(Crime {
                act: Act::Attack,
                at_host: None,
                tick: 1,
                victim: None,
                evidence: 1.0,
            });
        }
        assert_eq!(w.standing(), Standing::Wanted);
        // Now the cop closes on the player's tile over successive acts.
        let dist = |w: &World| {
            let c = w.npc_full()[0].1;
            (c.x as i32 - 8).abs() + (c.y as i32 - 8).abs()
        };
        let before = dist(&w);
        // NPC turns are gated by their speed, so give the cop enough ticks.
        for _ in 0..40 {
            w.tick = w.next_at(EntityId(1));
            w.apply(&Intent {
                actor: EntityId(1),
                verb: Verb::Wait,
            })
            .unwrap();
        }
        assert!(
            dist(&w) < before,
            "cop did not pursue: {before} -> {}",
            dist(&w)
        );
        let _ = start;
    }

    #[test]
    fn trojan_mail_lands_when_the_recipient_opens_it() {
        // An NPC shopkeeper who owns a rig, and the player with a Mailer+.
        let mut w = World::new(1);
        w.spawn(EntityId(1), pos(8, 8), 1);
        let acct = w.open_account(EntityId(1), HostId(1)).unwrap();
        w.ledger.mint(acct, 0).unwrap();
        w.spawn_npc(EntityId(11), Role::Shopkeeper, (3, 3), (3, 3), (3, 3), 20);
        w.ensure_host(HostId(2)).set_owner(Some(EntityId(11)));
        // Crafting a Mailer+ costs 3 algo.
        w.tick = w.next_at(EntityId(1));
        assert_eq!(
            w.apply(&Intent {
                actor: EntityId(1),
                verb: Verb::CraftMailerPlus,
            })
            .unwrap_err()
            .kind,
            "no_resource"
        );
        w.ensure_host(HostId(1))
            .add_stock(EntityId(1), crate::deck::Res::Algo, 3);
        w.tick = w.next_at(EntityId(1));
        let (_, events) = w
            .apply(&Intent {
                actor: EntityId(1),
                verb: Verb::CraftMailerPlus,
            })
            .unwrap();
        assert_eq!(events.last().map(|e| e.kind.as_str()), Some("mailer_plus"));
        assert!(w.has_mailer_plus());
        assert_eq!(w.units(EntityId(1), crate::deck::Res::Algo), 0);

        // Plain mail: delivered to the recipient's inbox, copied to ours.
        w.tick = w.next_at(EntityId(1));
        w.apply(&Intent {
            actor: EntityId(1),
            verb: Verb::SendMail {
                to: EntityId(11),
                body: "hello".into(),
            },
        })
        .unwrap();
        assert_eq!(w.inbox(EntityId(11)).len(), 1);
        assert_eq!(w.inbox(EntityId(11))[0].body, "hello");
        assert!(!w.inbox(EntityId(11))[0].trojan);

        // Trojan: delivered as an attachment, no breach needed. The
        // recipient's host is untouched until they open it.
        w.tick = w.next_at(EntityId(1));
        w.apply(&Intent {
            actor: EntityId(1),
            verb: Verb::SendTrojan { to: EntityId(11) },
        })
        .unwrap();
        assert!(w.inbox(EntityId(11)).iter().any(|m| m.trojan));
        assert!(!w.is_zombie_host(HostId(2)), "not yet opened");

        // The NPC opens its mail on its turn: the trojan runs and its rig
        // joins the planter's zombie net.
        for _ in 0..80 {
            w.tick = w.next_at(EntityId(1));
            w.apply(&Intent {
                actor: EntityId(1),
                verb: Verb::Wait,
            })
            .unwrap();
            if w.is_zombie_host(HostId(2)) {
                break;
            }
        }
        assert!(w.is_zombie_host(HostId(2)), "NPC never opened the trojan");

        // Daily spam income lands in the planter's account, capped per day.
        let bank = |w: &World| w.bank_balance(EntityId(1)).unwrap_or(0);
        let before = bank(&w);
        w.tick = w.next_at(EntityId(1)).max(crate::world::DAY_TICKS);
        w.apply(&Intent {
            actor: EntityId(1),
            verb: Verb::Wait,
        })
        .unwrap();
        assert_eq!(bank(&w) - before, 400, "one zombie host pays 400/day");
        assert_eq!(w.ledger.live_sum(), w.ledger.audit_sum());
    }

    #[test]
    fn opening_a_trojan_mail_enslaves_your_own_hosts() {
        // The player owns a rig; a hostile trojan arrives and the player
        // opens it, so their rig spams for the sender.
        let mut w = World::new(1);
        w.spawn(EntityId(1), pos(8, 8), 1);
        w.ensure_host(HostId(1)).set_owner(Some(EntityId(1)));
        // Hostile sender with a Mailer+ of their own (delivered directly).
        w.mailboxes
            .entry(1)
            .or_default()
            .deliver(Mail {
                from: EntityId(9),
                to: EntityId(1),
                body: "free credits!".into(),
                tick: 0,
                trojan: true,
            })
            .unwrap();
        assert_eq!(w.inbox(EntityId(1)).len(), 1);
        w.tick = w.next_at(EntityId(1));
        let (_, events) = w
            .apply(&Intent {
                actor: EntityId(1),
                verb: Verb::OpenMail { index: 0 },
            })
            .unwrap();
        assert_eq!(
            events.last().map(|e| e.kind.as_str()),
            Some("trojan_opened")
        );
        assert!(w.is_zombie_host(HostId(1)), "victim's rig enslaved");
        assert!(w.inbox(EntityId(1)).is_empty(), "mail consumed on open");
        // Opening an empty inbox is refused.
        w.tick = w.next_at(EntityId(1));
        assert_eq!(
            w.apply(&Intent {
                actor: EntityId(1),
                verb: Verb::OpenMail { index: 0 },
            })
            .unwrap_err()
            .kind,
            "no_mail"
        );
    }

    #[test]
    fn scrub_log_needs_access_or_privilege_and_firewall_toggles_routing() {
        use crate::net::{LinkKind, NetKind};
        let mut w = World::new(1);
        w.spawn(EntityId(1), pos(8, 8), 1);
        // Net triangle so a firewall filter can partition a route.
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
        // Firewall filter: host 2 blocks host 1, so 1->3 is partitioned.
        assert!(w.net.route(HostId(1), HostId(3)).is_ok());
        w.tick = w.next_at(EntityId(1));
        let (_, events) = w
            .apply(&Intent {
                actor: EntityId(1),
                verb: Verb::SetFirewall {
                    host: HostId(2),
                    source: HostId(1),
                    on: true,
                },
            })
            .unwrap();
        assert_eq!(events.last().map(|e| e.kind.as_str()), Some("firewall_on"));
        assert!(w.net.firewall_blocks(HostId(2), HostId(1)));
        assert!(
            w.net.route(HostId(1), HostId(3)).is_err(),
            "filtered transit"
        );
        // Toggling off heals the route.
        w.tick = w.next_at(EntityId(1));
        let (_, events) = w
            .apply(&Intent {
                actor: EntityId(1),
                verb: Verb::SetFirewall {
                    host: HostId(2),
                    source: HostId(1),
                    on: false,
                },
            })
            .unwrap();
        assert_eq!(events.last().map(|e| e.kind.as_str()), Some("firewall_off"));
        assert!(w.net.route(HostId(1), HostId(3)).is_ok());
        // Unknown host refuses.
        w.tick = w.next_at(EntityId(1));
        assert!(w
            .apply(&Intent {
                actor: EntityId(1),
                verb: Verb::SetFirewall {
                    host: HostId(9),
                    source: HostId(1),
                    on: true
                },
            })
            .is_err());

        // Scrub: no program and no privilege on the rig -> refused.
        w.ensure_host(HostId(1)).log(EntityId(1), 100, HostId(1));
        w.tick = w.next_at(EntityId(1));
        let err = w
            .apply(&Intent {
                actor: EntityId(1),
                verb: Verb::ScrubLog,
            })
            .unwrap_err();
        assert_eq!(err.kind, "no_access");
        // A program on the rig lets you scrub your trail.
        w.ensure_host(HostId(1))
            .load(EntityId(10), EntityId(1), CellPos(0, 0), 2, 1)
            .unwrap();
        w.ensure_host(HostId(1)).log(EntityId(1), 101, HostId(1));
        w.tick = w.next_at(EntityId(1));
        let (_, events) = w
            .apply(&Intent {
                actor: EntityId(1),
                verb: Verb::ScrubLog,
            })
            .unwrap();
        assert_eq!(events.last().map(|e| e.kind.as_str()), Some("scrubbed"));
        assert!(w
            .host(HostId(1))
            .unwrap()
            .logs_visible(EntityId(1))
            .is_empty());
        // Privilege alone also scrubs (no program left behind).
        let mut w2 = World::new(1);
        w2.spawn(EntityId(1), pos(8, 8), 1);
        let rig = w2.ensure_host(HostId(1));
        rig.add_vuln_kind("cve-2006-0005", true); // privilege
        rig.load(EntityId(10), EntityId(1), CellPos(0, 0), 2, 1)
            .unwrap();
        rig.log(EntityId(1), 100, HostId(1));
        rig.exploit(EntityId(10), "cve-2006-0005").unwrap();
        // Remove the program so only privilege remains.
        rig.strike(CellPos(0, 0)).unwrap();
        w2.tick = w2.next_at(EntityId(1));
        let (_, events) = w2
            .apply(&Intent {
                actor: EntityId(1),
                verb: Verb::ScrubLog,
            })
            .unwrap();
        assert_eq!(events.last().map(|e| e.kind.as_str()), Some("scrubbed"));
        assert!(w2
            .host(HostId(1))
            .unwrap()
            .logs_visible(EntityId(1))
            .is_empty());
        assert_eq!(w.ledger.live_sum(), w.ledger.audit_sum());
    }

    #[test]
    fn join_clan_costs_dues_and_hitting_a_clanmate_makes_an_enemy() {
        let mut w = World::new(1);
        w.spawn(EntityId(1), pos(8, 8), 1);
        let acct = w.open_account(EntityId(1), HostId(1)).unwrap();
        w.ledger.mint(acct, 600).unwrap();
        // Unbanked is refused; so is an empty purse.
        let mut poor = World::new(1);
        poor.spawn(EntityId(1), pos(8, 8), 1);
        poor.tick = poor.next_at(EntityId(1));
        assert!(poor
            .apply(&Intent {
                actor: EntityId(1),
                verb: Verb::JoinClan { clan: 1 },
            })
            .is_err());
        // Funded: joins, and the dues leave the player's pocket+bank.
        let before = w.bank_balance(EntityId(1)).unwrap() + w.needs(EntityId(1)).unwrap().2;
        w.tick = w.next_at(EntityId(1));
        let (_, events) = w
            .apply(&Intent {
                actor: EntityId(1),
                verb: Verb::JoinClan { clan: 1 },
            })
            .unwrap();
        assert_eq!(events.last().map(|e| e.kind.as_str()), Some("joined_clan"));
        assert!(w.clans().is_member(1, EntityId(1)));
        let after = w.bank_balance(EntityId(1)).unwrap() + w.needs(EntityId(1)).unwrap().2;
        assert_eq!(before - after, CLAN_FEE);
        assert_eq!(w.ledger.live_sum(), w.ledger.audit_sum());

        // A clanmate's rig: killing there marks lifetime enmity.
        w.clans.join(1, EntityId(11), true, true).unwrap();
        assert!(w.clans().friendly(EntityId(1), EntityId(11)));
        assert_eq!(w.clans().shared_clan(EntityId(1), EntityId(11)), Some(1));
        let host = w.ensure_host(HostId(1));
        host.set_owner(Some(EntityId(11)));
        host.load(EntityId(50), EntityId(11), CellPos(5, 5), 1, 1)
            .unwrap();
        w.tick = w.next_at(EntityId(1));
        w.apply(&Intent {
            actor: EntityId(1),
            verb: Verb::DeckStrike { x: 5, y: 5 },
        })
        .unwrap();
        assert!(w.clans().is_enemy(EntityId(1), 1));
        assert!(
            !w.clans().friendly(EntityId(1), EntityId(11)),
            "lifetime enemy"
        );
        assert_eq!(w.clans().shared_clan(EntityId(1), EntityId(11)), None);
    }

    #[test]
    fn license_arc_threat_kills_then_abuse_revokes() {
        // Range rig (warden-owned) so kills count as threat kills.
        let mut w = World::new(1);
        w.spawn(EntityId(1), pos(8, 8), 1);
        let rig = w.ensure_host(HostId(1));
        rig.set_owner(Some(Host::WARDEN));
        // Three warden programs posted at distinct cells to kill.
        for i in 0..crate::crime::LICENSE_KILLS {
            let x = 2 + (i as u8) * 3;
            rig.load(EntityId(70 + i as u64), Host::WARDEN, CellPos(x, 2), 1, 1)
                .unwrap();
        }
        for i in 0..crate::crime::LICENSE_KILLS {
            let x = 2 + (i as u8) * 3;
            w.tick = w.next_at(EntityId(1));
            w.apply(&Intent {
                actor: EntityId(1),
                verb: Verb::DeckStrike { x, y: 2 },
            })
            .unwrap();
        }
        assert_eq!(w.license().threat_kills, crate::crime::LICENSE_KILLS);
        assert!(w.license().eligible(w.record()));
        // Apply: granted.
        w.tick = w.next_at(EntityId(1));
        let (_, events) = w
            .apply(&Intent {
                actor: EntityId(1),
                verb: Verb::ApplyLicense,
            })
            .unwrap();
        assert_eq!(events.last().map(|e| e.kind.as_str()), Some("licensed"));
        assert!(w.license().granted);
        // Re-applying is refused.
        w.tick = w.next_at(EntityId(1));
        assert!(w
            .apply(&Intent {
                actor: EntityId(1),
                verb: Verb::ApplyLicense,
            })
            .is_err());

        // A licensed hacker entering a foreign rig is not trespassing.
        let mut w2 = World::new(1);
        w2.spawn(EntityId(1), pos(8, 8), 1);
        w2.license.granted = true;
        w2.ensure_host(HostId(1)).set_owner(Some(EntityId(11)));
        w2.tick = w2.next_at(EntityId(1));
        w2.apply(&Intent {
            actor: EntityId(1),
            verb: Verb::DeckLoad { x: 0, y: 0 },
        })
        .unwrap();
        assert_eq!(w2.record().len(), 0, "licensed entry is legal");
        // Hitting a defender while licensed is abuse: revoked + charged.
        w2.ensure_host(HostId(1))
            .load(EntityId(50), EntityId(11), CellPos(5, 5), 1, 1)
            .unwrap();
        w2.tick = w2.next_at(EntityId(1));
        w2.apply(&Intent {
            actor: EntityId(1),
            verb: Verb::DeckStrike { x: 5, y: 5 },
        })
        .unwrap();
        assert!(w2.license().revoked && !w2.license().granted);
        assert_eq!(w2.record_mass(), 4.0, "abuse files a full Attack");
        assert!(!w2.license().eligible(w2.record()));
        assert_eq!(w.ledger.live_sum(), w.ledger.audit_sum());
    }

    #[test]
    fn mods_are_persistent_and_cost_resources() {
        use crate::deck::{Mod, Res};
        let mut w = World::new(1);
        w.spawn(EntityId(1), pos(8, 8), 1);
        // Our len-6 program and a hostile len-8 one.
        let rig = w.ensure_host(HostId(1));
        rig.load(EntityId(10), EntityId(1), CellPos(0, 0), 6, 1)
            .unwrap();
        for i in 1..6 {
            rig.step(EntityId(10), CellPos(i, 0)).unwrap();
        }
        rig.load(EntityId(20), EntityId(9), CellPos(5, 5), 6, 1)
            .unwrap();
        for i in 1..6 {
            rig.step(EntityId(20), CellPos(5 + i, 5)).unwrap();
        }
        // Splash costs 2 loop; short stock refuses without installing.
        w.tick = w.next_at(EntityId(1));
        let err = w
            .apply(&Intent {
                actor: EntityId(1),
                verb: Verb::DeckMod {
                    x: 0,
                    y: 0,
                    card: "splash".into(),
                },
            })
            .unwrap_err();
        assert_eq!(err.kind, "no_resource");
        assert!(w.host(HostId(1)).unwrap().mods_of(EntityId(10)).is_empty());
        // Funded: installs, charges 2 loop.
        w.ensure_host(HostId(1))
            .add_stock(EntityId(1), Res::Loop, 5);
        w.tick = w.next_at(EntityId(1));
        let (_, events) = w
            .apply(&Intent {
                actor: EntityId(1),
                verb: Verb::DeckMod {
                    x: 0,
                    y: 0,
                    card: "splash".into(),
                },
            })
            .unwrap();
        assert_eq!(events.last().map(|e| e.kind.as_str()), Some("modded"));
        assert_eq!(w.units(EntityId(1), Res::Loop), 3, "2 loop spent");
        assert_eq!(
            w.host(HostId(1)).unwrap().mods_of(EntityId(10)),
            &[Mod::Splash]
        );
        // A mod is not a one-shot effect: it survives repeated attacks.
        for _ in 0..3 {
            let target = {
                let h = w.host(HostId(1)).unwrap();
                h.progs[&20]
                    .tail
                    .back()
                    .copied()
                    .unwrap_or(h.progs[&20].head)
            };
            w.ensure_host(HostId(1))
                .card_attack(EntityId(1), EntityId(10), target, None, None)
                .unwrap();
            assert_eq!(
                w.host(HostId(1)).unwrap().mods_of(EntityId(10)),
                &[Mod::Splash]
            );
        }
        // Re-installing the same mod is refused (no wasted cost).
        w.tick = w.next_at(EntityId(1));
        let err = w
            .apply(&Intent {
                actor: EntityId(1),
                verb: Verb::DeckMod {
                    x: 0,
                    y: 0,
                    card: "splash".into(),
                },
            })
            .unwrap_err();
        assert_eq!(err.kind, "dup_mod");
        // A non-moddable card and a foreign program are refused.
        w.tick = w.next_at(EntityId(1));
        assert!(w
            .apply(&Intent {
                actor: EntityId(1),
                verb: Verb::DeckMod {
                    x: 0,
                    y: 0,
                    card: "distract".into(),
                },
            })
            .is_err());
        w.tick = w.next_at(EntityId(1));
        assert!(w
            .apply(&Intent {
                actor: EntityId(1),
                verb: Verb::DeckMod {
                    x: 5,
                    y: 5,
                    card: "tank".into(),
                },
            })
            .is_err());
        assert_eq!(w.ledger.live_sum(), w.ledger.audit_sum());
    }

    #[test]
    fn cards_charge_their_resource_cost_and_refuse_when_short() {
        use crate::deck::Res;
        let mut w = World::new(1);
        w.spawn(EntityId(1), pos(8, 8), 1);
        let rig = w.ensure_host(HostId(1));
        rig.load(EntityId(10), EntityId(1), CellPos(0, 0), 6, 1)
            .unwrap();
        // `tank` costs 1 loop; with an empty stock it is refused and the
        // effect is not applied.
        w.tick = w.next_at(EntityId(1));
        let err = w
            .apply(&Intent {
                actor: EntityId(1),
                verb: Verb::DeckCard {
                    card: "tank".into(),
                    x: 0,
                    y: 0,
                },
            })
            .unwrap_err();
        assert_eq!(err.kind, "no_resource");
        assert!(!w
            .host(HostId(1))
            .unwrap()
            .has_fx(EntityId(10), |f| matches!(
                f,
                crate::deck::Fx::ShieldNext(_)
            )));
        // Fund it: the card lands and spends exactly one loop.
        w.ensure_host(HostId(1))
            .add_stock(EntityId(1), Res::Loop, 3);
        w.tick = w.next_at(EntityId(1));
        let (_, events) = w
            .apply(&Intent {
                actor: EntityId(1),
                verb: Verb::DeckCard {
                    card: "tank".into(),
                    x: 0,
                    y: 0,
                },
            })
            .unwrap();
        assert_eq!(events.first().map(|e| e.kind.as_str()), Some("card"));
        assert_eq!(w.units(EntityId(1), Res::Loop), 2, "one loop spent");
        assert!(w
            .host(HostId(1))
            .unwrap()
            .has_fx(EntityId(10), |f| matches!(
                f,
                crate::deck::Fx::ShieldNext(_)
            )));
        // Free cards (attacks fund their own damage) never charge the cost
        // table; `farm` is free and produces.
        assert_eq!(w.card_cost("farm"), None);
        assert_eq!(w.ledger.live_sum(), w.ledger.audit_sum());
    }

    #[test]
    fn every_card_id_resolves_through_apply() {
        use crate::deck::Res;
        // One program of ours and one enemy on the rig, so both self-buff
        // and debuff cards have a valid target.
        let mut w = World::new(1);
        w.spawn(EntityId(1), pos(8, 8), 1);
        let rig = w.ensure_host(HostId(1));
        rig.load(EntityId(10), EntityId(1), CellPos(0, 0), 6, 1)
            .unwrap();
        rig.load(EntityId(20), EntityId(9), CellPos(6, 0), 6, 1)
            .unwrap();
        // Two loop units so the funded attacks have stock.
        w.ensure_host(HostId(1))
            .add_stock(EntityId(1), Res::Loop, 4);

        // (card id, cell of our program, cell of the enemy).
        let mine = (0u8, 0u8);
        let foe = (6u8, 0u8);
        let cards = crate::deck::Host::implemented_cards();
        assert_eq!(cards.len(), 20, "table size");
        for card in cards {
            // Self cards target our program; hostile ones target the enemy.
            let self_card = matches!(
                card,
                "tank"
                    | "plating"
                    | "reflect"
                    | "splash"
                    | "extra"
                    | "patch"
                    | "load"
                    | "upgrade"
                    | "copy"
                    | "farm"
            );
            let (x, y) = if self_card { mine } else { foe };
            // Keep our program alive and the enemy present across the loop.
            if !w.host(HostId(1)).unwrap().progs.contains_key(&20) {
                w.ensure_host(HostId(1))
                    .load(EntityId(20), EntityId(9), CellPos(6, 0), 6, 1)
                    .unwrap();
            }
            if !w.host(HostId(1)).unwrap().progs.contains_key(&10) {
                w.ensure_host(HostId(1))
                    .load(EntityId(10), EntityId(1), CellPos(0, 0), 6, 1)
                    .unwrap();
            }
            // Fund every resource so each card's cost can be met.
            for res in Res::ALL {
                w.ensure_host(HostId(1)).add_stock(EntityId(1), res, 4);
            }
            w.tick = w.next_at(EntityId(1));
            let (_, events) = w
                .apply(&Intent {
                    actor: EntityId(1),
                    verb: Verb::DeckCard {
                        card: card.to_string(),
                        x,
                        y,
                    },
                })
                .unwrap_or_else(|e| panic!("card {card} refused: {e}"));
            assert_eq!(
                events.first().map(|e| e.kind.as_str()),
                Some("card"),
                "card {card} kind"
            );
        }
        // An unknown card id is refused.
        w.tick = w.next_at(EntityId(1));
        let err = w
            .apply(&Intent {
                actor: EntityId(1),
                verb: Verb::DeckCard {
                    card: "nope".to_string(),
                    x: 0,
                    y: 0,
                },
            })
            .unwrap_err();
        assert_eq!(err.kind, "no_card");
        assert_eq!(w.ledger.live_sum(), w.ledger.audit_sum());
    }

    #[test]
    fn farm_makers_produce_free_each_act() {
        use crate::deck::Res;
        let mut w = World::new(1);
        w.spawn(EntityId(1), pos(8, 8), 1);
        let range = w.ensure_host(HostId(1));
        range.set_owner(Some(crate::deck::Host::WARDEN));
        range.set_monitor(true);
        // Farm a loop producer: act kind farmed, and it yields 3/act free.
        w.tick = w.next_at(EntityId(1));
        let (_, events) = w
            .apply(&Intent {
                actor: EntityId(1),
                verb: Verb::DeckFarm {
                    x: 5,
                    y: 5,
                    res: Res::Loop,
                },
            })
            .unwrap();
        let kinds: Vec<&str> = events.iter().map(|e| e.kind.as_str()).collect();
        assert_eq!(kinds.first(), Some(&"farmed"), "kinds: {kinds:?}");
        // Farming draws no retaliation beat: no trace opened on the farm.
        assert!(!kinds.contains(&"traced"), "kinds: {kinds:?}");
        // The farm act itself already ticks the maker once.
        assert_eq!(w.units(EntityId(1), Res::Loop), 3);
        // Two more acts (Wait) yield 3 each, with a produced event.
        for _ in 0..2 {
            w.tick = w.next_at(EntityId(1));
            let (_, ev) = w
                .apply(&Intent {
                    actor: EntityId(1),
                    verb: Verb::Wait,
                })
                .unwrap();
            assert!(ev.iter().any(|e| e.kind == "produced"), "{ev:?}");
        }
        assert_eq!(w.units(EntityId(1), Res::Loop), 9);
        // Producer program is on the rig and costs its tier CPU (the
        // defended host also holds a warden, so count only our own).
        let h = w.host(HostId(1)).unwrap();
        let mine: u32 = h
            .progs
            .values()
            .filter(|p| p.owner == EntityId(1))
            .map(|p| 1 + p.len() as u32 * p.cpu_per_seg as u32)
            .sum();
        assert_eq!(mine, 2, "loop maker costs 2 CPU");
        assert!(h.progs.values().any(|p| p.owner == EntityId(1)));
        // Refused acts do not tick the maker (Work 4 needs algo we lack).
        w.tick = w.next_at(EntityId(1));
        assert!(w
            .apply(&Intent {
                actor: EntityId(1),
                verb: Verb::Work(4),
            })
            .is_err());
        assert_eq!(w.units(EntityId(1), Res::Loop), 9, "no yield on refusal");
        // Higher tiers cost more CPU and yield their own rate.
        w.tick = w.next_at(EntityId(1));
        w.apply(&Intent {
            actor: EntityId(1),
            verb: Verb::DeckFarm {
                x: 6,
                y: 5,
                res: Res::Algo,
            },
        })
        .unwrap();
        assert_eq!(w.units(EntityId(1), Res::Algo), 2);
        let h = w.host(HostId(1)).unwrap();
        let mine: u32 = h
            .progs
            .values()
            .filter(|p| p.owner == EntityId(1))
            .map(|p| 1 + p.len() as u32 * p.cpu_per_seg as u32)
            .sum();
        assert_eq!(mine, 2 + 5, "loop 2 + algo 5 CPU");
        assert_eq!(w.ledger.live_sum(), w.ledger.audit_sum());
    }

    #[test]
    fn warden_ignores_producers_and_farm_is_safe() {
        use crate::deck::Res;
        let mut w = World::new(1);
        w.spawn(EntityId(1), pos(8, 8), 1);
        let range = w.ensure_host(HostId(1));
        range.set_owner(Some(crate::deck::Host::WARDEN));
        range.set_monitor(true);
        // Farm a producer: it survives its own beat and yields.
        w.tick = w.next_at(EntityId(1));
        w.apply(&Intent {
            actor: EntityId(1),
            verb: Verb::DeckFarm {
                x: 0,
                y: 0,
                res: Res::Loop,
            },
        })
        .unwrap();
        assert_eq!(w.units(EntityId(1), Res::Loop), 3);
        // Now load a killer, which does trigger the warden. The warden
        // targets the killer, never the passive producer.
        w.tick = w.next_at(EntityId(1));
        w.apply(&Intent {
            actor: EntityId(1),
            verb: Verb::DeckLoad { x: 11, y: 7 },
        })
        .unwrap();
        let h = w.host(HostId(1)).unwrap();
        assert!(
            h.progs.values().any(|p| h.maker_info(p.id).is_some()),
            "producer survived the retaliation beat"
        );
        // Still yielding: the DeckLoad act ticked it once more (3 -> 6).
        assert_eq!(
            w.units(EntityId(1), Res::Loop),
            6,
            "producer still yielding"
        );
        // Repeated beats eventually kill the killer but spare the maker.
        for _ in 0..40 {
            w.tick = w.next_at(EntityId(1));
            let _ = w.apply(&Intent {
                actor: EntityId(1),
                verb: Verb::DeckStrike { x: 11, y: 7 },
            });
        }
        let h = w.host(HostId(1)).unwrap();
        assert!(
            h.progs.values().any(|p| h.maker_info(p.id).is_some()),
            "producer still alive after repeated warden beats"
        );
    }

    #[test]
    fn code_feeds_jobs_and_gate_refuses_when_short() {
        use crate::deck::Res;
        let mut w = World::new(1);
        w.spawn(EntityId(1), pos(8, 8), 1);
        // Job 0 wants 4 loop; working with an empty rig is refused.
        w.tick = w.next_at(EntityId(1));
        let err = w
            .apply(&Intent {
                actor: EntityId(1),
                verb: Verb::Work(0),
            })
            .unwrap_err();
        assert_eq!(err.kind, "no_resource");
        assert_eq!(w.units(EntityId(1), Res::Loop), 0);
        // Manual coding yields 1 unit/act at a mood price.
        let (_, mood0, money0, _) = w.needs(EntityId(1)).unwrap();
        w.tick = w.next_at(EntityId(1));
        let (_, events) = w
            .apply(&Intent {
                actor: EntityId(1),
                verb: Verb::Code { res: Res::Loop },
            })
            .unwrap();
        assert_eq!(events.last().map(|e| e.kind.as_str()), Some("coded"));
        let (_, mood1, money1, _) = w.needs(EntityId(1)).unwrap();
        assert_eq!(w.units(EntityId(1), Res::Loop), 1);
        // 1 act upkeep + 2 code cost = 3 mood, no cash movement.
        assert_eq!(mood0 - mood1, 3);
        assert_eq!(money0, money1);
        // Three more codes fill the requirement, then the job pays.
        for _ in 0..3 {
            w.tick = w.next_at(EntityId(1));
            w.apply(&Intent {
                actor: EntityId(1),
                verb: Verb::Code { res: Res::Loop },
            })
            .unwrap();
        }
        assert_eq!(w.units(EntityId(1), Res::Loop), 4);
        let (_, _, m0, _) = w.needs(EntityId(1)).unwrap();
        w.tick = w.next_at(EntityId(1));
        w.apply(&Intent {
            actor: EntityId(1),
            verb: Verb::Work(0),
        })
        .unwrap();
        assert_eq!(w.units(EntityId(1), Res::Loop), 0, "units consumed");
        assert_eq!(w.needs(EntityId(1)).unwrap().2, m0 + 60);
        // Coding the wrong resource does not satisfy the job.
        w.tick = w.next_at(EntityId(1));
        w.apply(&Intent {
            actor: EntityId(1),
            verb: Verb::Code { res: Res::Algo },
        })
        .unwrap();
        w.tick = w.next_at(EntityId(1));
        let err = w
            .apply(&Intent {
                actor: EntityId(1),
                verb: Verb::Work(0),
            })
            .unwrap_err();
        assert_eq!(err.kind, "no_resource");
        assert_eq!(w.units(EntityId(1), Res::Algo), 1, "algo untouched");
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
        // Job 1 needs 6 loop; stock the rig so the gate passes.
        w.ensure_host(HostId(1))
            .add_stock(EntityId(1), crate::deck::Res::Loop, 6);
        let (_, _, m0, _) = w.needs(EntityId(1)).unwrap();
        let _ = w.apply(&Intent {
            actor: EntityId(1),
            verb: Verb::Work(1),
        });
        let (_, _, m1, _) = w.needs(EntityId(1)).unwrap();
        assert_eq!(m1, m0 + 120); // file value, not code constant
        assert_eq!(w.units(EntityId(1), crate::deck::Res::Loop), 0); // 6 consumed
                                                                     // Old snapshots without a jobs table still load (serde default).
        let mut w2 = World::load_json(r#"{"tick":0,"seed":1,"beings":{}}"#).unwrap();
        w2.spawn(EntityId(9), pos(8, 8), 1);
        // Old save gets the default requirement table too, so stock the rig.
        w2.ensure_host(HostId(1))
            .add_stock(EntityId(9), crate::deck::Res::Loop, 4);
        let (_, _, n0, _) = w2.needs(EntityId(9)).unwrap();
        let _ = w2.apply(&Intent {
            actor: EntityId(9),
            verb: Verb::Work(0),
        });
        assert_eq!(w2.needs(EntityId(9)).unwrap().2, n0 + 60);
    }
}

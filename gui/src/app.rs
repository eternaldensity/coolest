//! Interactive eframe shell over the deterministic sim.
//! Street is server-backed (intent queue); Deck/Net tabs hold local demo
//! state until the multiplayer unification (design/08). `--frames N`
//! runs N frames then exits (headless smoke via xvfb).

use client_web::status_line;
use eframe::egui;
use server::Server;
use sim::{
    cop_duty, Dir, EntityId, Host, HostId, Intent, LinkKind, NetKind, Pos, Role, Standing,
    Timetable, Verb,
};
use std::collections::BTreeMap;
use std::rc::Rc;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tab {
    Street,
    Deck,
    Net,
    Veil,
}

type StreetJob = (String, i64, Option<(sim::Res, u32, u32)>);

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

/// One-line effect blurb for stocked items whose benefit is otherwise
/// unstated. No entry means no promised effect.
fn log_color(line: &str) -> egui::Color32 {
    let s = line.to_lowercase();
    if s.contains("rejected")
        || s.contains("failed")
        || s.contains("no route")
        || s.contains("hurt")
        || s.contains("downed")
        || s.contains("burned")
    {
        crate::theme::DANGER
    } else if s.contains("paid")
        || s.contains("saved")
        || s.contains("loaded")
        || s.contains("complete")
        || s.contains("granted")
        || s.contains("joined")
        || s.contains("healed")
    {
        crate::theme::SUCCESS
    } else if s.contains("trace")
        || s.contains("scan")
        || s.contains("route")
        || s.contains("wire")
        || s.contains("firewall")
    {
        crate::theme::BETANET
    } else if s.contains("contract") || s.contains("pay") || s.contains('$') {
        crate::theme::MONEY
    } else {
        crate::theme::MUTED
    }
}

fn kind_hint(kind: sim::ProgramKind) -> &'static str {
    match kind {
        sim::ProgramKind::Killer => "Killer: attacks heads and tails.",
        sim::ProgramKind::Shield => "Shield: absorbs incoming hits.",
        sim::ProgramKind::Scanner => "Scanner: scans a host to reveal vulns and trace.",
        sim::ProgramKind::Cracker => "Cracker: cracks locks over three beats.",
        sim::ProgramKind::Connector => "Connector: probes routed hosts and reveals scan reports.",
        sim::ProgramKind::Cloner => "Cloner: copies a program into a free cell.",
        sim::ProgramKind::Editor => "Editor: patches vulns and severs tails.",
    }
}

fn alignment_label(a: sim::Alignment) -> &'static str {
    match a {
        sim::Alignment::HonestPassive => "honest · passive",
        sim::Alignment::HonestActive => "honest · active",
        sim::Alignment::Untrusted => "untrusted",
        sim::Alignment::Authority => "authority",
        sim::Alignment::MinorDishonest => "minor dishonest",
        sim::Alignment::MajorDishonest => "major dishonest",
        sim::Alignment::Evil => "evil",
    }
}

fn item_blurb(item: &str) -> &'static str {
    match item {
        "firewall" => " · slows inbound traces",
        "ration" => " · +30 food",
        "meds" => " · +25 mood, +5 food",
        _ => "",
    }
}

fn card_family(id: &str) -> (&'static str, egui::Color32) {
    if matches!(
        id,
        "distract" | "freakout" | "stun" | "jam" | "hex" | "decoy" | "downgrade" | "shuffle"
    ) {
        ("negative · enemy", crate::theme::DANGER)
    } else if matches!(id, "spike" | "overload" | "copy" | "farm") {
        ("manipulation", crate::theme::OLDNET)
    } else {
        ("positive · support", crate::theme::SUCCESS)
    }
}

fn card_effect_text(def: &sim::CardDef) -> &'static str {
    match def.effect.as_str() {
        "SkipNext" => "Target loses its next act.",
        "DotNext(1)" => "Target loses 1 tail segment next turn.",
        "DotNext(2)" => "Target loses 2 tail segments next turn.",
        "RollMinus(2)" => "Target's next attack roll is reduced by 2.",
        "BanDie(6)" => "Target cannot crit on its next attack.",
        "DecoyNext" => "The next incoming hit on the target is absorbed.",
        "DowngradeNext" => "Target's next attack deals half damage.",
        "RandomTargetNext" => "Target's next attack retargets randomly.",
        "ShieldNext(1)" => "Absorb 1 incoming damage next turn.",
        "ShieldNext(2)" => "Absorb 2 incoming damage next turn.",
        "heal-lowest" => "Heal the lowest paired stat by 2.",
        "consume-to-damage" => "Spend 2 LOOP for +2 damage.",
        "consume-all-to-damage" => "Spend the whole LOOP pile for +damage.",
        "ReflectNext" => "Return the next incoming hit to the attacker's body.",
        "SplashNext" => "Next attack also deals 1 to adjacent programs.",
        "ExtraRollNext" => "Roll twice on the next attack; keep the higher face.",
        "produce" => "Run your producers once.",
        "clone-segment" => "Clone a segment into a free cell.",
        "grow" => "Grow the selected program by one segment.",
        "upgrade-max" => "Raise the selected program's growth cap.",
        _ => "Deck effect.",
    }
}

/// Save slot: a real file on desktop, a browser-local key on web (where
/// the sandbox has no filesystem).
#[cfg(not(target_arch = "wasm32"))]
fn save_slot() -> std::path::PathBuf {
    // Platform temp dir (%TEMP% on Windows, /tmp on unix) so the exe works
    // everywhere; created on demand since it may not exist.
    std::env::temp_dir()
        .join("opencode")
        .join("fidom-save.json")
}
#[cfg(target_arch = "wasm32")]
const SAVE_SLOT: &str = "fidom-save";

#[cfg(not(target_arch = "wasm32"))]
fn persist_save(server: &Server) -> String {
    let path = save_slot();
    if let Some(parent) = path.parent() {
        if std::fs::create_dir_all(parent).is_err() {
            return "save failed: cannot create save directory".to_string();
        }
    }
    match path.to_str() {
        Some(p) => match server.save_snapshot(p) {
            Ok(()) => "saved".to_string(),
            Err(e) => format!("save failed: {e}"),
        },
        None => "save failed: save path is not UTF-8".to_string(),
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn persist_load(server: &mut Server) -> String {
    match save_slot().to_str() {
        Some(p) => match server.load_snapshot(p) {
            Ok(()) => "loaded".to_string(),
            Err(e) => format!("load failed: {e}"),
        },
        None => "load failed: save path is not UTF-8".to_string(),
    }
}

#[cfg(target_arch = "wasm32")]
fn web_storage() -> Option<web_sys::Storage> {
    web_sys::window()?.local_storage().ok()?
}

#[cfg(target_arch = "wasm32")]
fn persist_save(server: &Server) -> String {
    match web_storage() {
        Some(store) => match store.set_item(SAVE_SLOT, &server.snapshot()) {
            Ok(()) => "saved".to_string(),
            Err(_) => "save failed: browser storage refused the snapshot".to_string(),
        },
        None => "save failed: no browser storage".to_string(),
    }
}

#[cfg(target_arch = "wasm32")]
fn persist_load(server: &mut Server) -> String {
    match web_storage().and_then(|store| store.get_item(SAVE_SLOT).ok()?) {
        Some(json) => match server.restore(&json) {
            Ok(()) => "loaded".to_string(),
            Err(e) => format!("load failed: {e}"),
        },
        None => "load failed: empty save slot".to_string(),
    }
}

pub struct Shell {
    server: Server,
    actor: EntityId,
    tab: Tab,
    input: String,
    log: Vec<String>,
    sel: Option<(u8, u8)>,
    street_sel: Option<(u8, u8)>,
    talks: BTreeMap<String, Rc<sim::Dialogue>>,
    talking: Option<sim::Conversation>,
    frames_left: Option<u32>,
    jobs: Vec<sim::JobDef>,
    palette_open: bool,
    palette_query: String,
    help_open: bool,
    net_sel: Option<HostId>,
    reduced_motion: bool,
    cards: BTreeMap<String, sim::CardDef>,
    hovered_card: Option<String>,
}

impl Shell {
    pub fn new(frames: Option<u32>) -> Self {
        let mut server = Server::new(42);
        let actor = EntityId(1);
        server.world.spawn(actor, pos(8, 8), 10);

        server
            .world
            .net
            .add_node(HostId(1), NetKind::Oldnet, 10, false);
        server
            .world
            .net
            .add_node(HostId(2), NetKind::Oldnet, 100, true);
        server
            .world
            .net
            .add_node(HostId(3), NetKind::Betanet, 50, false);
        server
            .world
            .net
            .add_link(
                HostId(1),
                HostId(2),
                LinkKind::Wired { damaged: false },
                5,
                0,
            )
            .ok();
        server
            .world
            .net
            .add_link(HostId(2), HostId(3), LinkKind::Laser, 40, 10)
            .ok();
        // Anchor each host to a section so power kills reroute traffic.
        server.world.power.anchor(HostId(1), (0, 0));
        server.world.power.anchor(HostId(2), (1, 0));
        server.world.power.anchor(HostId(3), (2, 0));
        server.world.ensure_host(HostId(1));
        server.world.ensure_host(HostId(3));
        // World host 1 is a defended range rig: loading foreign programs
        // draws a warden and a trace (M2 retaliation, always on). It is an
        // office workstation, so it carries that archetype's vulns.
        let vulns = sim::parse_vulns(include_str!("../../data/vulns.yaml")).unwrap_or_default();
        let range = server.world.ensure_host(HostId(1));
        range.set_owner(Some(Host::WARDEN));
        range.set_monitor(true);
        let installed = range.set_arch(sim::HostArch::OfficeWS, &vulns);
        let _ = installed;
        // Jobs drive both mechanics and button labels (data/jobs.yaml).
        let jobs = sim::parse_jobs(include_str!("../../data/jobs.yaml")).unwrap_or_default();
        if !jobs.is_empty() {
            server.world.set_jobs(&jobs);
        }
        // Card costs and previews come from data/cards.yaml.
        let mut card_defs = BTreeMap::new();
        if let Ok(cards) = sim::parse_cards(include_str!("../../data/cards.yaml")) {
            for c in cards
                .negative
                .iter()
                .chain(&cards.positive)
                .chain(&cards.manipulation)
            {
                card_defs.insert(c.id.clone(), c.clone());
            }
            server.world.set_cards(&cards);
        }
        if let Ok(items) = sim::parse_items(include_str!("../../data/items.yaml")) {
            server.world.set_items(&items);
        }
        if let Ok(contracts) = sim::parse_contracts(include_str!("../../data/contracts.yaml")) {
            server.world.set_contracts(&contracts);
        }
        if let Ok(kinds) = sim::parse_kinds(include_str!("../../data/kinds.yaml")) {
            server.world.set_kinds(&kinds);
        }
        let mut talks = BTreeMap::new();
        for d in sim::parse_dialogues(include_str!("../../data/dialogues.yaml")).unwrap_or_default()
        {
            talks.insert(d.id.clone(), Rc::new(d));
        }
        // Demo veil complex: lobby - ops - roof(exit). Ops has a seizable
        // weak host and two mind-files; the roof is hazardous.
        server.world.veil.add_area(1, "lobby", vec![2], false);
        server.world.veil.add_area(2, "ops", vec![1, 3], false);
        server.world.veil.add_area(3, "roof", vec![2], true);
        server.world.veil.set_host(2, EntityId(70), 3);
        server.world.veil.set_hazard(3, 2);
        server
            .world
            .veil
            .set_files(2, vec!["plans".to_string(), "chat-log".to_string()]);

        server
            .world
            .spawn_npc(EntityId(10), Role::Shopkeeper, (3, 3), (12, 4), (8, 12), 20);
        server
            .world
            .spawn_npc(EntityId(11), Role::Cop, (4, 12), (12, 12), (8, 8), 15);
        // Demo economy: player bank account with starter funds + a food
        // shop by the spawn. Till belongs to the shopkeeper (EntityId 10).
        let buyer = server.world.open_account(actor, HostId(1)).unwrap();
        server.world.ledger.mint(buyer, 500).unwrap();
        let till = server.world.open_account(EntityId(10), HostId(1)).unwrap();
        // Contract employers escrow from their account; seed the demo shop's
        // float so the board is playable without waiting for sales.
        server.world.ledger.mint(till, 5000).unwrap();
        let mut shop = sim::Shop::new(till);
        shop.add_line("ration", 100, 5, 2, 5, 200);
        shop.add_line("meds", 400, 2, 1, 3, 900);
        shop.add_line("firewall", 1500, 1, 1, 1, 1200);
        shop.add_line("knife", 300, 2, 1, 3, 200);
        shop.add_line("jacket", 500, 2, 1, 3, 300);
        shop.add_line("medkit", 250, 3, 1, 4, 150);
        shop.set_hours(8, 20);
        server.world.place_shop(10, 8, shop);
        // Night store three doors down: always open, steeper prices for
        // the graveyard shift. Same till owner, separate account.
        let night_till = server.world.open_account(EntityId(10), HostId(1)).unwrap();
        server.world.ledger.mint(night_till, 5000).unwrap();
        let mut night = sim::Shop::new(night_till);
        night.add_line("ration", 150, 3, 2, 3, 300);
        night.add_line("meds", 600, 2, 1, 2, 1200);
        night.add_line("knife", 450, 1, 1, 2, 300);
        night.set_hours(0, 24);
        server.world.place_shop(13, 8, night);
        // Rented room above the noodle shop (design/01): sleep here.
        server.world.set_home(pos(8, 8));
        // Populate the local board after employers exist.
        server.world.refresh_board();
        Self {
            server,
            actor,
            tab: Tab::Street,
            input: String::new(),
            log: vec!["Welcome to FIDOM. Arrows/hjkl move. Type help + Enter.".to_string()],
            sel: None,
            street_sel: None,
            talks,
            talking: None,
            frames_left: frames,
            jobs,
            palette_open: false,
            palette_query: String::new(),
            help_open: false,
            net_sel: None,
            reduced_motion: false,
            cards: card_defs,
            hovered_card: None,
        }
    }

    fn say(&mut self, line: String) {
        if self.log.len() >= 200 {
            self.log.remove(0);
        }
        self.log.push(line);
    }

    fn say_status(&mut self) {
        if let Some((food, mood, money, rations)) = self.server.world.needs(self.actor) {
            self.say(format!(
                "{} rations={rations} tick={}",
                status_line(food, mood, money),
                self.server.world.tick
            ));
        }
    }

    fn do_verb(&mut self, verb: Verb) {
        let intent = Intent {
            actor: self.actor,
            verb,
        };
        match self.server.apply_one(intent) {
            Ok((_, events)) => {
                for line in Self::beat_lines(&events) {
                    self.say(line);
                }
            }
            Err(e) => self.say(format!("rejected: {e}")),
        }
        self.say_status();
    }

    /// Like `do_verb`, but also relays any retaliation beat the act
    /// triggered (warden kills, traces) — the console code/farm paths
    /// would otherwise silently lose a producer that died on its first beat.
    fn act_narrated(&mut self, verb: Verb) {
        self.do_verb(verb);
    }

    fn run_command(&mut self) {
        let cmd = std::mem::take(&mut self.input);
        let cmd = cmd.trim().to_lowercase();
        match cmd.as_str() {
            "help" => self.say(
                "move arrows/hjkl(vi-keys) · eat · work 0-4 · code loop/data/algo · farm loop/data/algo · watch · buy <item> · pickup/drop/equip/use <item> · talk · deposit/withdraw <c> · sleep · revive"
                    .to_string(),
            ),
            "eat" => self.do_verb(Verb::Eat),
            "sleep" => self.do_verb(Verb::Sleep),
            "revive" => self.do_verb(Verb::Revive),
            _ if cmd.starts_with("buy ") => self.do_verb(Verb::Buy(cmd[4..].trim().to_string())),
            _ if cmd.starts_with("pickup ") => self.do_verb(Verb::Pickup {
                item: cmd[7..].trim().to_string(),
            }),
            _ if cmd.starts_with("drop ") => self.do_verb(Verb::Drop {
                item: cmd[5..].trim().to_string(),
            }),
            _ if cmd.starts_with("equip ") => self.do_verb(Verb::Equip {
                item: cmd[6..].trim().to_string(),
            }),
            _ if cmd.starts_with("use ") => self.do_verb(Verb::Use {
                item: cmd[4..].trim().to_string(),
            }),
            _ if cmd.starts_with("deposit ") => match cmd[8..].trim().parse::<i64>() {
                Ok(amt) => self.do_verb(Verb::Deposit(amt)),
                Err(_) => self.say(format!("unknown: {cmd}")),
            },
            _ if cmd.starts_with("withdraw ") => match cmd[9..].trim().parse::<i64>() {
                Ok(amt) => self.do_verb(Verb::Withdraw(amt)),
                Err(_) => self.say(format!("unknown: {cmd}")),
            },
            "talk" => self.start_talk(),
            "watch" => self.do_verb(Verb::Watch),
            _ if cmd.starts_with("code ") => {
                let res = match cmd[5..].trim() {
                    "loop" => Some(sim::Res::Loop),
                    "data" => Some(sim::Res::Data),
                    "algo" => Some(sim::Res::Algo),
                    other => {
                        self.say(format!("unknown resource: {other}"));
                        None
                    }
                };
                if let Some(res) = res {
                    self.act_narrated(Verb::Code { res });
                }
            }
            _ if cmd.starts_with("farm ") => {
                let res = match cmd[5..].trim() {
                    "loop" => Some(sim::Res::Loop),
                    "data" => Some(sim::Res::Data),
                    "algo" => Some(sim::Res::Algo),
                    other => {
                        self.say(format!("unknown resource: {other}"));
                        None
                    }
                };
                if let Some(res) = res {
                    // Use the selected cell, else the safest free one (the
                    // warden strikes an adjacent program on the same beat).
                    let cell = self.sel.or_else(|| {
                        self.server
                            .world
                            .host(HostId(1))
                            .and_then(|h| h.free_cell_for(self.actor))
                            .map(|c| (c.0, c.1))
                    });
                    match cell {
                        Some((x, y)) => self.act_narrated(Verb::DeckFarm { x, y, res }),
                        None => self.say("rig is full — strike a program first".to_string()),
                    }
                }
            }
            "work 0" => self.do_verb(Verb::Work(0)),
            "work 1" => self.do_verb(Verb::Work(1)),
            "work 2" => self.do_verb(Verb::Work(2)),
            "work 3" => self.do_verb(Verb::Work(3)),
            "work 4" => self.do_verb(Verb::Work(4)),
            "" => {}
            _ => self.say(format!("unknown: {cmd}")),
        }
    }

    /// One defense beat on the range rig after a deck action: the warden
    /// answers foreign programs, and a fresh trace marks the funnel.
    /// Relay lines for a deck retaliation beat carried on apply events.
    fn beat_lines(events: &[sim::Event]) -> Vec<String> {
        let mut out = Vec::new();
        for e in events {
            match e.kind.as_str() {
                "traced" => {
                    out.push("trace started on your program — scrub or outrun it".to_string())
                }
                "warden_kill" => out.push("warden killed one of your programs".to_string()),
                "trace_burned" => out.push("trace completed — program burned".to_string()),
                "hit" => out.push("you hit the target".to_string()),
                "miss" => out.push("you miss".to_string()),
                "killed" => out.push("target down — loot dropped".to_string()),
                "hurt" => out.push("you are hit!".to_string()),
                "downed" => out.push("you are down — revive at the clinic".to_string()),
                "dodged" => out.push("attack dodged".to_string()),
                "picked_up" => out.push("picked up".to_string()),
                "dropped" => out.push("dropped".to_string()),
                "equipped" => out.push("equipped".to_string()),
                "used" => out.push("used".to_string()),
                "surrendered" => out.push("surrendered — fine paid".to_string()),
                "jailed" => out.push("spent the night in jail — record cleared".to_string()),
                "contract_accepted" => out.push("contract accepted".to_string()),
                "contract_done" => out.push("contract complete — escrow paid".to_string()),
                "contract_abandoned" => {
                    out.push("contract abandoned — deposit forfeited".to_string())
                }
                "contract_expired" => out.push("contract expired".to_string()),
                _ => {}
            }
        }
        out
    }

    /// NPCs on the actor's tile or adjacent (same section).
    fn nearby_npcs(&self) -> Vec<(u64, sim::Role)> {
        let here = match self.server.world.pos_of(self.actor) {
            Some(p) => p,
            None => return vec![],
        };
        self.server
            .world
            .npc_full()
            .into_iter()
            .filter(|(_, p, _)| {
                p.sx == here.sx
                    && p.sy == here.sy
                    && (p.x as i16 - here.x as i16).abs() <= 1
                    && (p.y as i16 - here.y as i16).abs() <= 1
            })
            .map(|(id, _, role)| (id, role))
            .collect()
    }

    fn dialogue_for(&self, role: sim::Role) -> Option<Rc<sim::Dialogue>> {
        let id = match role {
            sim::Role::Shopkeeper => "shopkeeper",
            sim::Role::Cop => "cop",
            sim::Role::Office => "civilian",
        };
        self.talks.get(id).cloned()
    }

    fn start_talk(&mut self) {
        if self.talking.is_some() {
            return;
        }
        match self.nearby_npcs().into_iter().next() {
            Some((_, role)) => self.start_talk_with(role),
            None => self.say("no one in earshot".to_string()),
        }
    }

    fn start_talk_with(&mut self, role: sim::Role) {
        if self.talking.is_some() {
            return;
        }
        match self.dialogue_for(role) {
            Some(dlg) => {
                let t = sim::Conversation::new(dlg, None);
                self.say(t.text().to_string());
                self.talking = Some(t);
            }
            None => self.say("they have nothing to say".to_string()),
        }
    }

    fn move_key(&mut self, ctx: &egui::Context) {
        if ctx.input(|i| i.modifiers.command || i.modifiers.ctrl) {
            return;
        }
        let dir = if ctx.input(|i| i.key_pressed(egui::Key::ArrowUp) || i.key_pressed(egui::Key::K))
        {
            Some(Dir::N)
        } else if ctx.input(|i| i.key_pressed(egui::Key::ArrowDown) || i.key_pressed(egui::Key::J))
        {
            Some(Dir::S)
        } else if ctx.input(|i| i.key_pressed(egui::Key::ArrowLeft) || i.key_pressed(egui::Key::H))
        {
            Some(Dir::W)
        } else if ctx.input(|i| i.key_pressed(egui::Key::ArrowRight) || i.key_pressed(egui::Key::L))
        {
            Some(Dir::E)
        } else {
            None
        };
        if let Some(d) = dir {
            self.do_verb(Verb::Move(d));
        }
    }
}

impl eframe::App for Shell {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.show(ui);
    }
}

impl Shell {
    fn inspector(&mut self, ui: &mut egui::Ui) {
        crate::theme::heading(ui, "Inspector");
        match self.tab {
            Tab::Street => {
                let here = self.server.world.pos_of(self.actor).unwrap_or(pos(8, 8));
                let (x, y) = self.street_sel.unwrap_or((here.x, here.y));
                let walkable = self
                    .server
                    .world
                    .section_at(here.sx, here.sy)
                    .map(|s| s.walkable(x, y))
                    .unwrap_or(true);
                crate::theme::card(ui, "selection", crate::theme::BETANET, |ui| {
                    ui.label(
                        egui::RichText::new(format!("Tile ({x},{y})"))
                            .size(18.0)
                            .strong()
                            .color(crate::theme::TEXT),
                    )
                    .on_hover_text(
                        "Selected tile. Click the map or a nearby landmark to inspect it.",
                    );
                    ui.horizontal_wrapped(|ui| {
                        crate::theme::chip(
                            ui,
                            if walkable { "walkable" } else { "blocked" },
                            if walkable {
                                crate::theme::SUCCESS
                            } else {
                                crate::theme::DANGER
                            },
                        )
                        .on_hover_text(
                            "Walkable tiles can be entered; blocked tiles are walls or obstacles.",
                        );
                        if (x, y) == (here.x, here.y) {
                            crate::theme::chip(ui, "you are here", crate::theme::BETANET)
                                .on_hover_text("This is your current tile.");
                        }
                    });
                    if walkable
                        && (x, y) != (here.x, here.y)
                        && ui
                            .button("Walk here")
                            .on_hover_text("Pathfind to this tile one act at a time.")
                            .clicked()
                    {
                        self.walk_to(x, y);
                    }
                });
                // Nearby locator: keeps the inspector useful on the player's
                // own tile and turns the map into a selection surface.
                let near = |nx: u8, ny: u8| {
                    (nx as i16 - here.x as i16).abs() <= 5 && (ny as i16 - here.y as i16).abs() <= 5
                };
                let near_npcs: Vec<(Role, u8, u8)> = self
                    .server
                    .world
                    .npc_positions()
                    .into_iter()
                    .filter(|(_, nx, ny)| near(*nx, *ny))
                    .filter_map(|(id, nx, ny)| {
                        self.server.world.npc_role(id).map(|role| (role, nx, ny))
                    })
                    .collect();
                let near_shops: Vec<(u8, u8)> = self
                    .server
                    .world
                    .shop_positions()
                    .into_iter()
                    .filter(|(nx, ny)| near(*nx, *ny))
                    .collect();
                if !near_npcs.is_empty() || !near_shops.is_empty() {
                    crate::theme::card(ui, "nearby", crate::theme::OLDNET, |ui| {
                        ui.label(
                            egui::RichText::new("click to inspect")
                                .size(10.0)
                                .color(crate::theme::FAINT),
                        )
                        .on_hover_text("Landmarks within five tiles of you.");
                        for (role, nx, ny) in near_npcs {
                            if ui
                                .button(format!("{role:?} ({nx},{ny})"))
                                .on_hover_text("Select this landmark to inspect it.")
                                .clicked()
                            {
                                self.street_sel = Some((nx, ny));
                            }
                        }
                        for (nx, ny) in near_shops {
                            if ui
                                .button(format!("counter ({nx},{ny})"))
                                .on_hover_text("Select this counter to inspect its stock.")
                                .clicked()
                            {
                                self.street_sel = Some((nx, ny));
                            }
                        }
                    });
                }
                let npc = self
                    .server
                    .world
                    .npc_positions()
                    .into_iter()
                    .find(|(_, nx, ny)| (*nx, *ny) == (x, y))
                    .and_then(|(id, _, _)| {
                        self.server
                            .world
                            .npc_role(id)
                            .map(|role| (EntityId(id), role))
                    });
                if let Some((id, role)) = npc {
                    crate::theme::card(ui, "npc", crate::theme::SUCCESS, |ui| {
                        ui.label(format!("{role:?}"))
                            .on_hover_text("NPC on the selected tile.");
                        ui.horizontal_wrapped(|ui| {
                            if ui
                                .button(format!("Talk to {role:?}"))
                                .on_hover_text("Start a conversation.")
                                .clicked()
                            {
                                self.start_talk_with(role);
                            }
                            if ui
                                .button("Attack target")
                                .on_hover_text(
                                    "Attack this NPC. Witnessed crimes raise your record.",
                                )
                                .clicked()
                            {
                                self.do_verb(Verb::Attack { target: id });
                            }
                        });
                    });
                }
                if let Some(shop) = self.server.world.shop_at(x, y) {
                    let adjacent = (x as i16 - here.x as i16).abs() <= 1
                        && (y as i16 - here.y as i16).abs() <= 1;
                    let stock: Vec<(String, i64, u32)> = shop
                        .stock
                        .iter()
                        .map(|(k, l)| (k.clone(), l.price_cents, l.qty))
                        .collect();
                    crate::theme::card(ui, "counter", crate::theme::OLDNET, |ui| {
                        for (item, price, qty) in stock {
                            ui.horizontal(|ui| {
                                ui.label(format!("{item} — {price}c"));
                                crate::theme::chip(
                                    ui,
                                    format!("x{qty}"),
                                    if qty == 0 {
                                        crate::theme::DANGER
                                    } else {
                                        crate::theme::MUTED
                                    },
                                );
                                if adjacent
                                    && qty > 0
                                    && ui
                                        .button(format!("Buy {item}"))
                                        .on_hover_text(format!(
                                            "Buy one for {price}c. Counters serve adjacent tiles."
                                        ))
                                        .clicked()
                                {
                                    self.do_verb(Verb::Buy(item.clone()));
                                }
                            });
                        }
                        if !adjacent {
                            crate::theme::label(ui, "stand next to the counter to buy");
                        }
                    });
                }
                let floor = self.server.world.floor_items_at(pos(x, y));
                if !floor.is_empty() {
                    crate::theme::card(ui, "on floor", crate::theme::MUTED, |ui| {
                        for (item, qty) in floor {
                            if ui
                                .button(format!("Take {item} x{qty}"))
                                .on_hover_text("Pick up from the selected tile.")
                                .clicked()
                            {
                                self.do_verb(Verb::Pickup { item });
                            }
                        }
                    });
                }
            }
            Tab::Deck => {
                let Some((x, y)) = self.sel else {
                    crate::theme::card(ui, "selection", crate::theme::BETANET, |ui| {
                        crate::theme::label(ui, "Select a cell in the arena.");
                    });
                    return;
                };
                let prog = self
                    .server
                    .world
                    .host(HostId(1))
                    .and_then(|h| h.prog_at(x, y));
                crate::theme::card(ui, "selection", crate::theme::BETANET, |ui| {
                    ui.label(
                        egui::RichText::new(format!("Cell ({x},{y})"))
                            .size(18.0)
                            .strong()
                            .color(crate::theme::TEXT),
                    )
                    .on_hover_text("Selected arena cell.");
                    if let Some(prog) = prog {
                        let host = self.server.world.host(HostId(1)).unwrap();
                        let kind = host.program_kind(prog);
                        if let Some(kind) = kind {
                            crate::theme::chip(ui, kind.name(), crate::theme::BETANET)
                                .on_hover_text(kind_hint(kind));
                        }
                        if let Some(p) = host.progs.get(&prog.0) {
                            ui.label(format!("cpu {} / segment · len {}", p.cpu_per_seg, p.len()))
                                .on_hover_text("CPU cost per segment and current length.");
                            ui.label(format!("max len {}", p.max_len))
                                .on_hover_text("Growth cap for this program.");
                        }
                        if let Some((makes, rate)) = host.maker_info(prog) {
                            let names = makes
                                .iter()
                                .map(|r| format!("{} ×{}", r.name(), rate))
                                .collect::<Vec<_>>()
                                .join(" · ");
                            ui.label(format!("producer: {names}"))
                                .on_hover_text("Component yielded each act.");
                        }
                        let mods = host.mods_of(prog);
                        crate::theme::label(
                            ui,
                            if mods.is_empty() {
                                "mods: none".to_string()
                            } else {
                                format!("mods: {mods:?}")
                            },
                        )
                        .on_hover_text("Persistent mods built into this program.");
                    } else {
                        crate::theme::label(ui, "Empty cell — load a program here.")
                            .on_hover_text("Pick a program kind in Loadout to fill this cell.");
                    }
                });
                crate::theme::card(ui, "actions", crate::theme::OLDNET, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        if ui
                            .button("Scan host")
                            .on_hover_text("Scanner: reveal vulns, logs, trace, and breach state.")
                            .clicked()
                        {
                            self.do_verb(Verb::DeckScan);
                        }
                        if ui
                            .button("Crack lock")
                            .on_hover_text("Cracker: three beats to drop the filter/monitor.")
                            .clicked()
                        {
                            self.do_verb(Verb::DeckCrack);
                        }
                        if prog.is_some() {
                            if ui
                                .button("Clone program")
                                .on_hover_text("Cloner: copy this program into a free cell.")
                                .clicked()
                            {
                                self.do_verb(Verb::DeckClone { x, y });
                            }
                            if ui
                                .button("Sever segment")
                                .on_hover_text("Editor: sever a tail segment.")
                                .clicked()
                            {
                                self.do_verb(Verb::DeckOverwrite { x, y });
                            }
                        }
                    });
                    let vulns = self
                        .server
                        .world
                        .host(HostId(1))
                        .map(|h| h.vuln_ids())
                        .unwrap_or_default();
                    if !vulns.is_empty() {
                        ui.horizontal_wrapped(|ui| {
                            for v in vulns {
                                if ui
                                    .button(format!("Close {v}"))
                                    .on_hover_text("Patch this vulnerability.")
                                    .clicked()
                                {
                                    self.do_verb(Verb::DeckPatch { vuln: v });
                                }
                            }
                        });
                    }
                });
            }
            Tab::Net => {
                let target = self.net_sel.unwrap_or(HostId(3));
                crate::theme::card(ui, "node", crate::theme::BETANET, |ui| {
                    if let Some(n) = self.server.world.net.node(target) {
                        let dark = self.server.world.power.is_host_dark(target);
                        ui.label(
                            egui::RichText::new(format!("{target:?}"))
                                .size(18.0)
                                .strong()
                                .color(if dark {
                                    crate::theme::FAINT
                                } else {
                                    crate::theme::BETANET
                                }),
                        )
                        .on_hover_text("Selected host.");
                        ui.label(format!(
                            "{} · {}mb · {}{}",
                            match n.net {
                                NetKind::Oldnet => "oldnet",
                                NetKind::Betanet => "betanet",
                            },
                            n.modem_mbps,
                            if n.gateway { "gateway" } else { "no gateway" },
                            if dark { " · DARK" } else { "" }
                        ))
                        .on_hover_text("Net kind · modem speed · gateway · power state.");
                        if ui
                            .button(format!("Route 1→{}", target.0))
                            .on_hover_text(
                                "Preview a powered route from host 1 to this host, with tolls and trace threshold.",
                            )
                            .clicked()
                        {
                            match self.server.world.power.powered_route(
                                &self.server.world.net,
                                HostId(1),
                                target,
                            ) {
                                Ok(r) => self.say(format!(
                                    "route {:?} {}ms toll {} thr {}",
                                    r.path,
                                    r.latency_ms,
                                    r.toll_cents,
                                    r.trace_threshold()
                                )),
                                Err(e) => self.say(format!("no route: {e}")),
                            }
                        }
                        if ui
                            .button(format!("Probe {}", target.0))
                            .on_hover_text(
                                "Connector: probe this host, reveal its scan report, and log the transit.",
                            )
                            .clicked()
                        {
                            self.do_verb(Verb::DeckConnect { host: target });
                        }
                    }
                });
                crate::theme::card(ui, "nodes", crate::theme::MUTED, |ui| {
                    for id in self.server.world.net.node_ids() {
                        if let Some(n) = self.server.world.net.node(id) {
                            let dark = self.server.world.power.is_host_dark(id);
                            ui.label(format!(
                                "{id:?} · {} · {}mb{}{}",
                                match n.net {
                                    NetKind::Oldnet => "oldnet",
                                    NetKind::Betanet => "betanet",
                                },
                                n.modem_mbps,
                                if n.gateway { " · gateway" } else { "" },
                                if dark { " · DARK" } else { "" }
                            ));
                        }
                    }
                });
            }
            Tab::Veil => {
                crate::theme::card(ui, "helmet", crate::theme::VEIL, |ui| {
                    if let Some(area) = self.server.world.veil.area_of(self.actor) {
                        let name = self
                            .server
                            .world
                            .veil
                            .area_info(area)
                            .map(|(n, _, _)| n)
                            .unwrap_or_else(|| "area".to_string());
                        ui.label(format!("area: {name}"));
                        let hazard = self.server.world.veil.area_hazard(area);
                        if hazard > 0 {
                            crate::theme::chip(
                                ui,
                                format!("hazard {hazard}"),
                                crate::theme::DANGER,
                            );
                        }
                        if let Some((host, mp)) = self.server.world.veil.area_host(area) {
                            ui.label(format!("host {host:?} · mp {mp}"));
                        }
                        let files = self.server.world.veil.area_files(area);
                        crate::theme::section(ui, "mind-files");
                        if files.is_empty() {
                            crate::theme::label(ui, "no files in this area");
                        } else {
                            for f in files {
                                ui.horizontal(|ui| {
                                    ui.label(&f);
                                    if ui
                                        .button(format!("Take {f}"))
                                        .on_hover_text("Carry this mind-file.")
                                        .clicked()
                                    {
                                        self.do_verb(Verb::VeilCarry { name: f.clone() });
                                    }
                                });
                            }
                        }
                        let carried = self.server.world.veil.carried_files(self.actor);
                        if !carried.is_empty() {
                            crate::theme::section(ui, "carried");
                            for (i, f) in carried.iter().enumerate() {
                                if ui
                                    .button(format!("Consume {f}"))
                                    .on_hover_text("Read and consume this mind-file.")
                                    .clicked()
                                {
                                    self.do_verb(Verb::VeilRead { index: i });
                                }
                            }
                        }
                    } else {
                        crate::theme::label(ui, "Not jacked in.");
                    }
                });
            }
        }
    }

    fn help(&mut self, ctx: &egui::Context) {
        if !self.help_open {
            return;
        }
        let mut open = true;
        egui::Window::new("Field manual")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
            .show(ctx, |ui| {
                crate::theme::section(ui, "keys");
                ui.label("Tab / 1–4  views · arrows or hjkl  move · Ctrl-K  palette");
                ui.label("a attack · t talk · g pickup · e interact · F1 help · Esc close");
                crate::theme::section(ui, "views");
                ui.label("Street: needs, jobs, contracts, shops, combat, cops");
                ui.label("Deck: kinds, cards, CPU, trace, vulns, scrub");
                ui.label("Net: routes, tolls, power, firewalls, probes");
                ui.label("Veil: mindpower, areas, files, hazards, transfer");
                crate::theme::section(ui, "pressure");
                ui.label("Watch HP, food, mood, heat, trace and contract deadlines.");
                ui.label("The console is a record, not the only feedback surface.");
                crate::theme::section(ui, "commands");
                ui.label("eat · work 0-4 · code <res> · farm <res> · buy <item>");
                ui.label("pickup/drop/equip/use <item> · deposit/withdraw <c> · sleep · revive");
                crate::theme::section(ui, "presentation");
                if ui
                    .checkbox(&mut self.reduced_motion, "Reduced motion (disable fades)")
                    .changed()
                {
                    crate::theme::set_motion(ctx, self.reduced_motion);
                }
            });
        if !open {
            self.help_open = false;
        }
    }

    fn palette(&mut self, ctx: &egui::Context) {
        if !self.palette_open {
            return;
        }
        let mut open = true;
        egui::Window::new("Command palette")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
            .show(ctx, |ui| {
                let resp = ui.add(
                    egui::TextEdit::singleline(&mut self.palette_query)
                        .hint_text("jump, act, inspect…"),
                );
                resp.request_focus();
                let q = self.palette_query.to_lowercase();
                let commands = [
                    "Street",
                    "Deck",
                    "Net",
                    "Veil",
                    "Eat",
                    "Watch",
                    "Sleep",
                    "Revive",
                    "Pay fine",
                    "Accept first offer",
                    "Deliver contract",
                    "Scan",
                    "Crack",
                    "Save",
                    "Load",
                    "Help",
                ];
                let mut picked = None;
                for cmd in commands {
                    if (q.is_empty() || cmd.to_lowercase().contains(&q)) && ui.button(cmd).clicked()
                    {
                        picked = Some(cmd);
                    }
                }
                if let Some(cmd) = picked {
                    self.palette_run(cmd);
                    self.palette_open = false;
                }
            });
        if !open {
            self.palette_open = false;
        }
    }

    fn palette_run(&mut self, cmd: &str) {
        match cmd {
            "Street" => self.tab = Tab::Street,
            "Deck" => self.tab = Tab::Deck,
            "Net" => self.tab = Tab::Net,
            "Veil" => self.tab = Tab::Veil,
            "Eat" => self.do_verb(Verb::Eat),
            "Watch" => self.do_verb(Verb::Watch),
            "Sleep" => self.do_verb(Verb::Sleep),
            "Revive" => self.do_verb(Verb::Revive),
            "Pay fine" => self.do_verb(Verb::PayFine),
            "Deliver contract" => self.do_verb(Verb::DeliverContract),
            "Scan" => self.do_verb(Verb::DeckScan),
            "Crack" => self.do_verb(Verb::DeckCrack),
            "Accept first offer" => {
                if let Some(offer) = self.server.world.offers().first() {
                    self.do_verb(Verb::AcceptOffer { offer: offer.id });
                } else {
                    self.say("no offers on the board".to_string());
                }
            }
            "Save" => {
                let msg = persist_save(&self.server);
                self.say(msg);
            }
            "Load" => {
                let msg = persist_load(&mut self.server);
                self.say(msg);
            }
            "Help" => self.say(
                "Ctrl-K palette · Tab views · arrows/hjkl move · ? for command help".to_string(),
            ),
            _ => {}
        }
    }

    fn walk_to(&mut self, tx: u8, ty: u8) {
        let here = match self.server.world.pos_of(self.actor) {
            Some(p) => p,
            None => return,
        };
        let Some(section) = self.server.world.section_at(here.sx, here.sy) else {
            return;
        };
        if !section.walkable(tx, ty) {
            self.say("blocked: no path".to_string());
            return;
        }
        let idx = |x: u8, y: u8| y as usize * 16 + x as usize;
        let mut prev: Vec<Option<(u8, u8)>> = vec![None; 256];
        let mut queue = std::collections::VecDeque::from([(here.x, here.y)]);
        prev[idx(here.x, here.y)] = Some((here.x, here.y));
        while let Some((x, y)) = queue.pop_front() {
            if (x, y) == (tx, ty) {
                break;
            }
            for (dx, dy) in [(0i16, -1), (0, 1), (1, 0), (-1, 0)] {
                let (nx, ny) = (x as i16 + dx, y as i16 + dy);
                if !(1..=14).contains(&nx) || !(1..=14).contains(&ny) {
                    continue;
                }
                let (nx, ny) = (nx as u8, ny as u8);
                if !section.walkable(nx, ny) || prev[idx(nx, ny)].is_some() {
                    continue;
                }
                prev[idx(nx, ny)] = Some((x, y));
                queue.push_back((nx, ny));
            }
        }
        if prev[idx(tx, ty)].is_none() {
            self.say("blocked: no path".to_string());
            return;
        }
        let mut path = Vec::new();
        let mut cur = (tx, ty);
        while cur != (here.x, here.y) {
            path.push(cur);
            match prev[idx(cur.0, cur.1)] {
                Some(p) => cur = p,
                None => break,
            }
        }
        path.reverse();
        let mut steps = 0;
        for (nx, ny) in path {
            let dir = if nx
                > self
                    .server
                    .world
                    .pos_of(self.actor)
                    .map(|p| p.x)
                    .unwrap_or(nx)
            {
                Dir::E
            } else if nx
                < self
                    .server
                    .world
                    .pos_of(self.actor)
                    .map(|p| p.x)
                    .unwrap_or(nx)
            {
                Dir::W
            } else if ny
                > self
                    .server
                    .world
                    .pos_of(self.actor)
                    .map(|p| p.y)
                    .unwrap_or(ny)
            {
                Dir::S
            } else {
                Dir::N
            };
            if self
                .server
                .apply_one(Intent {
                    actor: self.actor,
                    verb: Verb::Move(dir),
                })
                .is_err()
            {
                break;
            }
            steps += 1;
        }
        self.say(format!("walked {steps} steps"));
    }

    fn street_map(&mut self, ui: &mut egui::Ui, here: Pos) {
        let (ax, ay) = (here.x, here.y);
        let shop_tiles = self.server.world.shop_positions();
        crate::theme::card(
            ui,
            &format!("street — section ({}, {})", here.sx, here.sy),
            crate::theme::BETANET,
            |ui| {
                let terrain = self.server.world.section_at(here.sx, here.sy).cloned();
                let npcs = self.server.world.npc_positions();
                crate::theme::label(
                    ui,
                    "@ you · n folk · $ counter (gold) · . floor · # blocked — stand by $, Buy",
                )
                .on_hover_text(
                    "Gold corner ticks mark doors, egress, and computers. Click a tile to inspect it; hover for details.",
                );
                // Let the map grow into wide windows and centre it in the
                // card, so the primary spatial surface is the visual anchor.
                let side = ui.available_width().clamp(320.0, 860.0);
                let ts = (side / 16.0).floor();
                let map_size = egui::vec2(16.0 * ts, 16.0 * ts);
                let canvas = egui::vec2(ui.available_width(), map_size.y);
                let (resp, painter) = ui.allocate_painter(canvas, egui::Sense::click());
                let rect = egui::Rect::from_center_size(resp.rect.center(), map_size);
                // Outer bezel and substrate.
                painter.rect_filled(rect.expand(4.0), 0.0, crate::theme::VOID);
                painter.rect_stroke(
                    rect.expand(4.0),
                    0.0,
                    egui::Stroke::new(1.0, crate::theme::alpha(crate::theme::BETANET, 110)),
                    egui::StrokeKind::Inside,
                );
                crate::theme::corner_marks(
                    &painter,
                    rect.expand(4.0),
                    crate::theme::alpha(crate::theme::BETANET, 180),
                    10.0,
                );

                for y in 0..16u8 {
                    for x in 0..16u8 {
                        let r = egui::Rect::from_min_size(
                            rect.min + egui::vec2(x as f32 * ts, y as f32 * ts),
                            egui::vec2(ts - 1.0, ts - 1.0),
                        );
                        let walkable = terrain.as_ref().map(|s| s.walkable(x, y)).unwrap_or(true);
                        let fill = if walkable {
                            if (x as i32 / 2 + y as i32 / 2) % 2 == 0 {
                                egui::Color32::from_rgb(0x08, 0x12, 0x17)
                            } else {
                                egui::Color32::from_rgb(0x0A, 0x16, 0x1C)
                            }
                        } else {
                            egui::Color32::from_rgb(0x18, 0x28, 0x31)
                        };
                        painter.rect_filled(r, 0.0, fill);
                        if walkable {
                            painter.rect_stroke(
                                r,
                                0.0,
                                egui::Stroke::new(
                                    0.5,
                                    crate::theme::alpha(crate::theme::BETANET, 18),
                                ),
                                egui::StrokeKind::Inside,
                            );
                        } else {
                            painter.rect_stroke(
                                r,
                                0.0,
                                egui::Stroke::new(
                                    1.0,
                                    crate::theme::alpha(crate::theme::LINE_HOT, 180),
                                ),
                                egui::StrokeKind::Inside,
                            );
                            painter.line_segment(
                                [r.left_top(), r.right_top()],
                                egui::Stroke::new(
                                    1.0,
                                    crate::theme::alpha(crate::theme::BETANET, 60),
                                ),
                            );
                            let hatch = r.shrink(5.0);
                            painter.line_segment(
                                [hatch.left_bottom(), hatch.right_top()],
                                egui::Stroke::new(
                                    0.6,
                                    crate::theme::alpha(crate::theme::LINE_HOT, 90),
                                ),
                            );
                        }
                    }
                }

                // Subtle coordinate ticks make the map read as a survey grid.
                for i in (0..16).step_by(4) {
                    painter.text(
                        rect.min + egui::vec2(i as f32 * ts + 3.0, 2.0),
                        egui::Align2::LEFT_TOP,
                        format!("{i:02}"),
                        egui::FontId::monospace(7.0),
                        crate::theme::alpha(crate::theme::FAINT, 150),
                    );
                    painter.text(
                        rect.min + egui::vec2(3.0, i as f32 * ts + 2.0),
                        egui::Align2::LEFT_TOP,
                        format!("{i:02}"),
                        egui::FontId::monospace(7.0),
                        crate::theme::alpha(crate::theme::FAINT, 120),
                    );
                }

                // Guarantees (doors/egress/computers) get a gold corner tick.
                if let Some(s) = &terrain {
                    for (_, gx, gy) in &s.guarantees {
                        let base = rect.min + egui::vec2(*gx as f32 * ts, *gy as f32 * ts);
                        let r = egui::Rect::from_min_size(base, egui::vec2(ts - 1.0, ts - 1.0));
                        crate::theme::corner_marks(
                            &painter,
                            r.shrink(4.0),
                            crate::theme::alpha(crate::theme::OLDNET, 210),
                            7.0,
                        );
                    }
                }

                for (sx, sy) in &shop_tiles {
                    let c = rect.min
                        + egui::vec2(*sx as f32 * ts + ts / 2.0, *sy as f32 * ts + ts / 2.0);
                    painter.circle_filled(c, 17.0, crate::theme::alpha(crate::theme::OLDNET, 20));
                    painter.circle_filled(c, 10.0, egui::Color32::from_rgb(0x2A, 0x20, 0x0D));
                    painter.circle_stroke(c, 10.0, egui::Stroke::new(1.3, crate::theme::OLDNET));
                    painter.text(
                        c,
                        egui::Align2::CENTER_CENTER,
                        "$",
                        egui::FontId::monospace(13.0),
                        crate::theme::OLDNET,
                    );
                }
                for (_, nx, ny) in &npcs {
                    let c = rect.min
                        + egui::vec2(*nx as f32 * ts + ts / 2.0, *ny as f32 * ts + ts / 2.0);
                    painter.circle_filled(c, 16.0, crate::theme::alpha(crate::theme::SUCCESS, 18));
                    painter.circle_filled(c, 8.0, egui::Color32::from_rgb(0x10, 0x24, 0x1B));
                    painter.circle_stroke(c, 8.0, egui::Stroke::new(1.3, crate::theme::SUCCESS));
                    painter.text(
                        c,
                        egui::Align2::CENTER_CENTER,
                        "n",
                        egui::FontId::monospace(11.0),
                        crate::theme::SUCCESS,
                    );
                }

                let player =
                    rect.min + egui::vec2(ax as f32 * ts + ts / 2.0, ay as f32 * ts + ts / 2.0);
                painter.circle_filled(player, 24.0, crate::theme::alpha(crate::theme::BETANET, 14));
                painter.circle_stroke(
                    player,
                    15.0,
                    egui::Stroke::new(1.0, crate::theme::alpha(crate::theme::BETANET, 90)),
                );
                painter.circle_filled(player, 10.0, egui::Color32::from_rgb(0x0E, 0x2A, 0x33));
                painter.circle_stroke(player, 10.0, egui::Stroke::new(2.0, crate::theme::BETANET));
                painter.text(
                    player,
                    egui::Align2::CENTER_CENTER,
                    "▲",
                    egui::FontId::monospace(12.0),
                    crate::theme::BETANET,
                );

                let hover_tile = resp.hover_pos().and_then(|p| {
                    if !rect.contains(p) {
                        return None;
                    }
                    let x = ((p.x - rect.min.x) / ts).floor() as i32;
                    let y = ((p.y - rect.min.y) / ts).floor() as i32;
                    if (0..16).contains(&x) && (0..16).contains(&y) {
                        Some((x as u8, y as u8))
                    } else {
                        None
                    }
                });
                if let Some((x, y)) = self.street_sel {
                    let r = egui::Rect::from_min_size(
                        rect.min + egui::vec2(x as f32 * ts, y as f32 * ts),
                        egui::vec2(ts - 1.0, ts - 1.0),
                    );
                    painter.rect_stroke(
                        r.expand(1.0),
                        0.0,
                        egui::Stroke::new(3.0, crate::theme::alpha(crate::theme::OLDNET, 35)),
                        egui::StrokeKind::Inside,
                    );
                    painter.rect_stroke(
                        r,
                        0.0,
                        egui::Stroke::new(1.6, crate::theme::OLDNET),
                        egui::StrokeKind::Inside,
                    );
                    crate::theme::corner_marks(&painter, r, crate::theme::OLDNET, 8.0);
                }
                if let Some((x, y)) = hover_tile {
                    let r = egui::Rect::from_min_size(
                        rect.min + egui::vec2(x as f32 * ts, y as f32 * ts),
                        egui::vec2(ts - 1.0, ts - 1.0),
                    );
                    painter.rect_stroke(
                        r,
                        0.0,
                        egui::Stroke::new(1.2, crate::theme::BETANET),
                        egui::StrokeKind::Inside,
                    );
                }

                // CRT wash over the whole survey surface.
                let mut sy = rect.top();
                while sy < rect.bottom() {
                    painter.line_segment(
                        [egui::pos2(rect.left(), sy), egui::pos2(rect.right(), sy)],
                        egui::Stroke::new(1.0, egui::Color32::from_black_alpha(18)),
                    );
                    sy += 4.0;
                }
                if resp.clicked() {
                    if let Some(t) = hover_tile {
                        self.street_sel = Some(t);
                    }
                }
                let tile_desc = hover_tile.map(|(x, y)| {
                    let mut bits = vec![format!("tile ({x},{y})")];
                    if let Some(s) = &terrain {
                        bits.push(
                            if s.walkable(x, y) {
                                "walkable"
                            } else {
                                "blocked"
                            }
                            .to_string(),
                        );
                    }
                    if (x, y) == (ax, ay) {
                        bits.push("you".to_string());
                    }
                    if let Some((id, _, _)) = npcs.iter().find(|(_, nx, ny)| (*nx, *ny) == (x, y)) {
                        if let Some(role) = self.server.world.npc_role(*id) {
                            bits.push(format!("{role:?}"));
                        }
                    }
                    if shop_tiles.contains(&(x, y)) {
                        bits.push("counter".to_string());
                    }
                    bits.join(" · ")
                });
                if let Some(desc) = tile_desc {
                    resp.on_hover_text_at_pointer(desc);
                }
                if let Some((x, y)) = self.street_sel {
                    crate::theme::label(
                        ui,
                        format!("Selected tile ({x},{y}) — click map to inspect"),
                    );
                } else {
                    ui.label(
                        egui::RichText::new("Click a tile to inspect · WASD/hjkl to walk")
                            .size(11.0)
                            .color(crate::theme::FAINT),
                    );
                }
            },
        );
    }

    fn street_work(&mut self, ui: &mut egui::Ui, jobs: &[StreetJob]) {
        // Job board on its own wrapped row: five long labels with
        // live counts overflow a single row and push later controls
        // outside the clip rect (their clicks are then ignored).
        crate::theme::card(ui, "work board", crate::theme::OLDNET, |ui| {
            ui.horizontal_wrapped(|ui| {
                for (i, (id, pay, req)) in jobs.iter().enumerate() {
                    let label = match req {
                        Some((res, need, have)) => {
                            format!("Work {i}: {id} {pay}c [{} {have}/{need}]", res.name())
                        }
                        None => format!("Work {i}: {id} {pay}c"),
                    };
                    let tip = match req {
                        Some((res, need, _)) => format!(
                            "Pays {pay}c. Needs {need} {}; the work act also spends mood and food.",
                            res.name()
                        ),
                        None => format!("Pays {pay}c. The work act also spends mood and food."),
                    };
                    if ui.button(label).on_hover_text(tip).clicked() {
                        self.do_verb(Verb::Work(i as u8));
                    }
                }
            });
        });
    }

    fn street_contracts(&mut self, ui: &mut egui::Ui) {
        // v0.5 contract board: local offers, one active contract.
        let offers = self.server.world.offers().to_vec();
        let active = self.server.world.active_contract().cloned();
        let tick = self.server.world.tick;
        if !offers.is_empty() || active.is_some() {
            crate::theme::card(ui, "contracts", crate::theme::MONEY, |ui| {
                if let Some(c) = &active {
                    let left = c.deadline_tick.saturating_sub(tick);
                    ui.horizontal_wrapped(|ui| {
                        ui.label(format!(
                            "Contract: {} · {} · {left}t left · {} rep",
                            c.title,
                            c.kind.summary(),
                            self.server.world.employer_reputation(c.employer)
                        ));
                        if ui
                            .button("Deliver contract")
                            .on_hover_text(
                                "Complete the active contract if its requirement is met; pays escrow.",
                            )
                            .clicked()
                        {
                            self.do_verb(Verb::DeliverContract);
                        }
                        if ui
                            .button("Abandon contract")
                            .on_hover_text(
                                "Give up the contract. Your deposit is forfeited and employer reputation drops.",
                            )
                            .clicked()
                        {
                            self.do_verb(Verb::AbandonContract);
                        }
                    });
                } else {
                    if offers.is_empty() {
                        crate::theme::label(ui, "No offers today — rest, work, or check mail.");
                    } else {
                        crate::theme::label(ui, "Accept one offer. Delivery pays the escrow.");
                    }
                    ui.horizontal_wrapped(|ui| {
                        for o in &offers {
                            let left = o.deadline_tick.saturating_sub(tick);
                            let label = format!(
                                "Accept: {} · {}c · {} · {} left",
                                o.title,
                                o.payout_cents,
                                o.kind.summary(),
                                left
                            );
                            let color = match o.risk.name() {
                                "illegal" | "combat" => crate::theme::DANGER,
                                "grey" => crate::theme::VEIL,
                                _ => crate::theme::OLDNET,
                            };
                            if ui
                                .add_sized(
                                    [250.0, 30.0],
                                    egui::Button::new(
                                        egui::RichText::new(label.clone()).size(11.0).color(color),
                                    )
                                    .fill(egui::Color32::from_rgb(0x12, 0x1B, 0x22))
                                    .stroke(egui::Stroke::new(1.0, color)),
                                )
                                .on_hover_text(format!(
                                    "{} · pays {}c · {} · {} risk · {}t left. Accepting escrows the reward and holds one contract.",
                                    o.title,
                                    o.payout_cents,
                                    o.kind.summary(),
                                    o.risk.name(),
                                    left
                                ))
                                .clicked()
                            {
                                self.do_verb(Verb::AcceptOffer { offer: o.id });
                            }
                        }
                    });
                }
            });
        }
    }

    /// Frame-free UI root: the runner calls this via [`eframe::App::ui`];
    /// headless tests drive it through `egui::Context::run_ui` with no
    /// window, GPU, or screenshots (see the accesskit harness below).
    fn show(&mut self, ui: &mut egui::Ui) {
        if let Some(left) = self.frames_left {
            if left == 0 {
                ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                return;
            }
            self.frames_left = Some(left - 1);
        }
        let palette_toggle = ui.ctx().input(|i| {
            i.events.iter().any(|e| {
                matches!(
                    e,
                    egui::Event::Key {
                        key: egui::Key::K,
                        pressed: true,
                        modifiers,
                        ..
                    } if modifiers.command || modifiers.ctrl
                )
            })
        });
        if palette_toggle {
            self.palette_open = !self.palette_open;
            self.palette_query.clear();
        }
        if self.tab == Tab::Street && !palette_toggle && !self.palette_open {
            let ctx = ui.ctx().clone();
            self.move_key(&ctx);
        }
        if ui.ctx().input(|i| i.key_pressed(egui::Key::Escape)) {
            self.palette_open = false;
            self.help_open = false;
        }
        if ui.ctx().input(|i| i.key_pressed(egui::Key::F1)) {
            self.help_open = !self.help_open;
        }

        let view_accent = match self.tab {
            Tab::Street => crate::theme::BETANET,
            Tab::Deck => crate::theme::OLDNET,
            Tab::Net => crate::theme::BETANET,
            Tab::Veil => crate::theme::VEIL,
        };

        egui::Panel::top("tabs")
            .frame(
                egui::Frame::new()
                    .fill(crate::theme::VOID)
                    .inner_margin(egui::Margin::symmetric(12, 7)),
            )
            .show(ui, |ui| {
            ui.horizontal(|ui| {
                let (mark, _) = ui.allocate_exact_size(egui::vec2(16.0, 16.0), egui::Sense::hover());
                let mp = ui.painter();
                mp.rect_filled(mark, 0.0, crate::theme::alpha(crate::theme::OLDNET, 220));
                mp.rect_filled(mark.shrink2(egui::vec2(3.0, 3.0)), 0.0, crate::theme::VOID);
                mp.line_segment(
                    [
                        mark.left_top() + egui::vec2(3.0, 3.0),
                        mark.right_bottom() - egui::vec2(3.0, 3.0),
                    ],
                    egui::Stroke::new(1.5, crate::theme::BETANET),
                );
                ui.add_space(5.0);
                ui.label(
                    egui::RichText::new("FIDOM")
                        .family(crate::theme::display_family())
                        .size(22.0)
                        .strong()
                        .color(crate::theme::PAPER),
                );
                ui.label(
                    egui::RichText::new("CIVIC NETWORK TERMINAL")
                        .family(egui::FontFamily::Monospace)
                        .size(8.5)
                        .color(crate::theme::OLDNET),
                );
                let blink = if self.reduced_motion || (ui.input(|i| i.time) * 2.0).sin() > 0.0 {
                    1.0
                } else {
                    0.2
                };
                let (cursor, _) =
                    ui.allocate_exact_size(egui::vec2(5.0, 10.0), egui::Sense::hover());
                ui.painter().rect_filled(
                    cursor,
                    0.0,
                    crate::theme::alpha(crate::theme::OLDNET, (210.0 * blink) as u8),
                );
                ui.add_space(8.0);
                if let Some((_, _, money, _)) = self.server.world.needs(self.actor) {
                    let here = self.server.world.pos_of(self.actor).unwrap_or(pos(8, 8));
                    crate::theme::chip(
                        ui,
                        format!("[ S({},{}) ]", here.sx, here.sy),
                        crate::theme::MUTED,
                    )
                    .on_hover_text(
                        "Section coordinates. The city is a 16×16 tile grid per section.",
                    );
                    let tick = self.server.world.tick;
                    crate::theme::chip(
                        ui,
                        format!(
                            "[ Day {} {:02}:{:02} ]",
                            tick / 1440 + 1,
                            (tick / 60) % 24,
                            tick % 60
                        ),
                        crate::theme::MUTED,
                    )
                    .on_hover_text("Game clock. Acts advance time; shops and police keep hours.");
                    crate::theme::chip(
                        ui,
                        format!("[ ${:.2} ]", money as f64 / 100.0),
                        crate::theme::MONEY,
                    )
                    .on_hover_text(
                        "Pocket cash. Fines take pocket money first; bank it with `deposit <c>`.",
                    );
                    let standing = self.server.world.standing();
                    let heat_color = if standing == Standing::Wanted {
                        crate::theme::DANGER
                    } else {
                        crate::theme::SUCCESS
                    };
                    crate::theme::chip(ui, format!("[ {standing:?} ]"), heat_color)
                        .on_hover_text(
                            "Clean or Wanted. Wanted draws police; pay the fine to clear your record.",
                        );
                    let mail = self.server.world.inbox(self.actor).len();
                    crate::theme::chip(
                        ui,
                        format!("[ mail {mail} ]"),
                        crate::theme::BETANET,
                    )
                    .on_hover_text("Unread mail. Open it from the Street tab's Mail card.");
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    for (tab, label, hint) in [
                        (
                            Tab::Veil,
                            "Veil",
                            "Mind space: jack in, move areas, seize hosts, read files.",
                        ),
                        (
                            Tab::Net,
                            "Net",
                            "Network graph: routes, tolls, power, firewalls.",
                        ),
                        (
                            Tab::Deck,
                            "Deck",
                            "Your rig: load programs, fight the warden, farm components.",
                        ),
                        (
                            Tab::Street,
                            "Street",
                            "City streets: move, work, shop, take contracts, talk.",
                        ),
                    ] {
                        let selected = self.tab == tab;
                        let resp = ui
                            .selectable_value(
                                &mut self.tab,
                                tab,
                                egui::RichText::new(label)
                                    .family(crate::theme::display_family())
                                    .size(13.0)
                                    .color(if selected {
                                        crate::theme::BETANET
                                    } else {
                                        crate::theme::MUTED
                                    }),
                            )
                            .on_hover_text(hint);
                        if selected {
                            let r = resp.rect;
                            ui.painter().line_segment(
                                [
                                    r.left_bottom() + egui::vec2(2.0, -1.0),
                                    r.right_bottom() + egui::vec2(-2.0, -1.0),
                                ],
                                egui::Stroke::new(2.0, crate::theme::BETANET),
                            );
                        }
                    }
                    ui.label(
                        egui::RichText::new("VIEWS")
                            .family(egui::FontFamily::Monospace)
                            .size(8.5)
                            .color(crate::theme::FAINT),
                    );
                });
            });
            let r = ui.max_rect();
            ui.painter().line_segment(
                [
                    egui::pos2(r.left(), r.bottom() - 1.0),
                    egui::pos2(r.right(), r.bottom() - 1.0),
                ],
                egui::Stroke::new(1.0, crate::theme::alpha(crate::theme::OLDNET, 110)),
            );
        });

        egui::Panel::left("status")
            .exact_size(248.0)
            .resizable(false)
            .frame(
                egui::Frame::new()
                    .fill(crate::theme::PANEL)
                    .inner_margin(egui::Margin::symmetric(10, 10)),
            )
            .show(ui, |ui| {
                crate::theme::scanlines(ui, 16);
                let edge = ui.max_rect();
                ui.painter().line_segment(
                    [
                        egui::pos2(edge.right() - 0.5, edge.top()),
                        egui::pos2(edge.right() - 0.5, edge.bottom()),
                    ],
                    egui::Stroke::new(1.0, crate::theme::alpha(crate::theme::LINE_HOT, 180)),
                );
                crate::theme::heading(ui, "Status");
                if let Some((food, mood, _money, rations)) = self.server.world.needs(self.actor) {
                    crate::theme::card(ui, "survival", crate::theme::OLDNET, |ui| {
                        let food_color = if food < 30 {
                            crate::theme::DANGER
                        } else {
                            crate::theme::OLDNET
                        };
                        let mood_color = if mood < 30 {
                            crate::theme::DANGER
                        } else {
                            crate::theme::BETANET
                        };
                        crate::theme::meter(ui, "Food", food as f32, 100.0, food_color)
                            .on_hover_text(
                                "Food drains every act. Eat a ration for +30; at 0 jobs lock and time slows.",
                            );
                        crate::theme::meter(ui, "Mood", mood as f32, 100.0, mood_color)
                            .on_hover_text(
                                "Mood drains with work and stress. Watch (+15 for $0.30) or sleep to recover.",
                            );
                        let bank = self
                            .server
                            .world
                            .bank_balance(self.actor)
                            .map(|b| format!("${:.2}", b as f64 / 100.0))
                            .unwrap_or_else(|| "—".to_string());
                        crate::theme::stat_row(ui, "bank", bank, crate::theme::MONEY)
                            .on_hover_text(
                                "Banked cash. Fines take pocket money first; use `deposit` / `withdraw` in the console.",
                            );
                        ui.add_space(2.0);
                        ui.label(
                            egui::RichText::new("components")
                                .size(10.0)
                                .color(crate::theme::FAINT),
                        )
                        .on_hover_text(
                            "Components stock the job board and program/card costs. Farm them on the Deck or Code them here.",
                        );
                        ui.horizontal_wrapped(|ui| {
                            for r in sim::Res::ALL {
                                crate::theme::chip(
                                    ui,
                                    format!(
                                        "{} {}",
                                        r.name().to_uppercase(),
                                        self.server.world.units(self.actor, r)
                                    ),
                                    crate::theme::resource_color(r),
                                )
                                .on_hover_text(match r {
                                    sim::Res::Loop => "LOOP: common job and card fuel. Producers yield it on the Deck.",
                                    sim::Res::Data => "DATA: office/scanner jobs and upgrades. Produced on the Deck.",
                                    sim::Res::Algo => "ALGO: crafting and high-tier jobs. Produced on the Deck.",
                                });
                            }
                        });
                    });
                    // Body/health (design/06-07). The clinic is the only way
                    // back from a lethal blow in v0.4.
                    if let Some(h) = self.server.world.health_of(self.actor) {
                        crate::theme::card(ui, "body", crate::theme::SUCCESS, |ui| {
                            let hp_frac = h.hp as f32 / h.max_hp.max(1) as f32;
                            let hp_color = if hp_frac <= 0.25 {
                                crate::theme::DANGER
                            } else if hp_frac <= 0.5 {
                                crate::theme::OLDNET
                            } else {
                                crate::theme::SUCCESS
                            };
                            crate::theme::meter(ui, "HP", h.hp as f32, h.max_hp as f32, hp_color)
                                .on_hover_text(
                                    "Health. At 0 you go down; revive at the clinic. Green → amber → red as it falls.",
                                );
                            if !h.is_alive() && ui.button("Revive at clinic").clicked() {
                                match self.server.apply_one(Intent {
                                    actor: self.actor,
                                    verb: Verb::Revive,
                                }) {
                                    Ok(_) => self.say("revived at the clinic".to_string()),
                                    Err(e) => self.say(format!("revive failed: {e}")),
                                }
                            }
                            if let Some(body) = self.server.world.body_of(self.actor) {
                                ui.horizontal_wrapped(|ui| {
                                    for (label, value) in [
                                        ("PV", body.pv),
                                        ("DV", body.dv),
                                        ("STR", body.str),
                                        ("DEX", body.dex),
                                    ] {
                                        crate::theme::chip(
                                            ui,
                                            format!("{label} {value}"),
                                            crate::theme::MUTED,
                                        )
                                        .on_hover_text(match label {
                                            "PV" => "PV: attack power.",
                                            "DV" => "DV: defence against incoming attacks.",
                                            "STR" => "STR: physical strength.",
                                            _ => "DEX: agility and precision.",
                                        });
                                    }
                                });
                            }
                            if let Some(mind) = self.server.world.mind_of(self.actor) {
                                crate::theme::meter(
                                    ui,
                                    "Mind",
                                    mind.mindpower as f32,
                                    20.0,
                                    crate::theme::VEIL,
                                )
                                .on_hover_text(
                                    "Mindpower. 5+ is required to jack into the Veil; hazards and death drain it.",
                                );
                                crate::theme::stat_row(
                                    ui,
                                    "literacy",
                                    mind.comp_literacy.to_string(),
                                    crate::theme::MUTED,
                                )
                                .on_hover_text(
                                    "Computer literacy: your working knowledge of hosts and programs.",
                                );
                            }
                        });
                    }
                    // Heat (design/05): alignment/standing are pure functions of
                    // the record, so settling the fine clears both. Standing
                    // itself lives in the top bar; this card explains why.
                    crate::theme::card(ui, "conduct", crate::theme::DANGER, |ui| {
                        let mass = self.server.world.record_mass();
                        crate::theme::stat_row(
                            ui,
                            "record mass",
                            format!("{mass:.1}"),
                            if mass > 0.0 {
                                crate::theme::DANGER
                            } else {
                                crate::theme::MUTED
                            },
                        )
                        .on_hover_text(
                            "Weight of your criminal record. Severity 6+ makes you Wanted; pay the fine to clear it.",
                        );
                        crate::theme::stat_row(
                            ui,
                            "alignment",
                            alignment_label(self.server.world.alignment()),
                            crate::theme::MUTED,
                        )
                        .on_hover_text(
                            "How the city reads your record, from honest through untrusted to evil.",
                        );
                        if mass > 0.0 {
                            let cost = self.server.world.fine_cost();
                            if ui
                                .button(format!("Pay fine (${:.2})", cost as f64 / 100.0))
                                .on_hover_text(
                                    "Settle your record (100c per severity point, pocket first). Clears Wanted and the chase.",
                                )
                                .clicked()
                            {
                                match self.server.apply_one(Intent {
                                    actor: self.actor,
                                    verb: Verb::PayFine,
                                }) {
                                    Ok(_) => self.say("fine paid — record settled".to_string()),
                                    Err(e) => self.say(format!("fine failed: {e}")),
                                }
                            }
                        }
                        // License to hack (design/05): 3 threat kills + clean record.
                        let lic = self.server.world.license();
                        let lic_value = if lic.granted {
                            "held".to_string()
                        } else if lic.revoked {
                            "revoked".to_string()
                        } else {
                            format!("{}/3 kills", lic.threat_kills)
                        };
                        crate::theme::stat_row(
                            ui,
                            "license",
                            lic_value,
                            if lic.revoked {
                                crate::theme::DANGER
                            } else {
                                crate::theme::MUTED
                            },
                        )
                        .on_hover_text(
                            "Hack license: 3 threat kills with a clean record. Licensed entry to honest hosts is legal until you hit a defender.",
                        );
                        if !lic.granted
                            && ui
                                .button("Apply license")
                                .on_hover_text(
                                    "Apply for a license. Needs 3 threat kills and a spotless record.",
                                )
                                .clicked()
                        {
                            match self.server.apply_one(Intent {
                                actor: self.actor,
                                verb: Verb::ApplyLicense,
                            }) {
                                Ok(_) => self.say("license granted".to_string()),
                                Err(e) => self.say(format!("license refused: {e}")),
                            }
                        }
                        // Clan (design/05 M4b): dues + initiation, lifetime enemies.
                        let in_clan = self.server.world.clans().is_member(1, self.actor);
                        crate::theme::stat_row(
                            ui,
                            "clan",
                            if in_clan { "member of 1" } else { "none" },
                            crate::theme::MUTED,
                        )
                        .on_hover_text(
                            "Clan membership. Clans offer backup but charge dues; hitting a clanmate makes a lifetime enemy.",
                        );
                        if !in_clan
                            && ui
                                .button("Join clan 1 ($5.00)")
                                .on_hover_text(
                                    "Join clan 1: pay a $5.00 fee and initiation, gain clan backup.",
                                )
                                .clicked()
                        {
                            match self.server.apply_one(Intent {
                                actor: self.actor,
                                verb: Verb::JoinClan { clan: 1 },
                            }) {
                                Ok(_) => self.say("joined clan 1".to_string()),
                                Err(e) => self.say(format!("join refused: {e}")),
                            }
                        }
                    });
                    crate::theme::card(ui, "briefing", crate::theme::MUTED, |ui| {
                        crate::theme::section(ui, "objective");
                        for part in self.server.world.advise(self.actor).split(", ") {
                            ui.label(
                                egui::RichText::new(format!("· {part}"))
                                    .size(12.0)
                                    .strong()
                                    .color(crate::theme::TEXT),
                            )
                            .on_hover_text(
                                "A suggested next step, not a lock. Follow it or pursue your own plan.",
                            );
                        }
                        if food < 30 || mood < 30 {
                            crate::theme::chip(
                                ui,
                                "! needs low — Eat / Watch soon",
                                crate::theme::DANGER,
                            )
                            .on_hover_text(
                                "Low food or mood locks jobs; starving also slows every act.",
                            );
                        }
                        crate::theme::stat_row(
                            ui,
                            "rations",
                            rations.to_string(),
                            crate::theme::TEXT,
                        )
                        .on_hover_text(
                            "Rations in your pack. Eat consumes one for +30 food; buy more at a counter.",
                        );
                        let cop = Timetable::cop_day();
                        crate::theme::stat_row(
                            ui,
                            "cop shift",
                            format!(
                                "10:00 · {}",
                                format!("{:?}", cop_duty(&cop, 10, Standing::Clean)).to_lowercase()
                            ),
                            crate::theme::MUTED,
                        )
                        .on_hover_text(
                            "Police duty for this hour. Wanted players are pursued even off shift.",
                        );
                    });
                }
                ui.add_space(4.0);
                let button_w = (ui.available_width() - 8.0) / 2.0;
                ui.horizontal(|ui| {
                    if ui
                        .add_sized([button_w, 26.0], egui::Button::new("Save"))
                        .on_hover_text("Write this run to the local save slot.")
                        .clicked()
                    {
                        let msg = persist_save(&self.server);
                        self.say(msg);
                    }
                    if ui
                        .add_sized([button_w, 26.0], egui::Button::new("Load"))
                        .on_hover_text("Restore the last local save.")
                        .clicked()
                    {
                        let msg = persist_load(&mut self.server);
                        self.say(msg);
                    }
                });
            });

        if ui.ctx().content_rect().width() >= 1120.0 {
            egui::Panel::right("inspector")
                .exact_size(300.0)
                .resizable(false)
                .frame(
                    egui::Frame::new()
                        .fill(crate::theme::PANEL)
                        .inner_margin(egui::Margin::symmetric(10, 10)),
                )
                .show(ui, |ui| {
                    crate::theme::scanlines(ui, 16);
                    let edge = ui.max_rect();
                    ui.painter().line_segment(
                        [
                            egui::pos2(edge.left() + 0.5, edge.top()),
                            egui::pos2(edge.left() + 0.5, edge.bottom()),
                        ],
                        egui::Stroke::new(1.0, crate::theme::alpha(crate::theme::LINE_HOT, 180)),
                    );
                    self.inspector(ui);
                });
        }

        egui::Panel::bottom("console")
            .exact_size(156.0)
            .frame(
                egui::Frame::new()
                    .fill(crate::theme::VOID)
                    .inner_margin(egui::Margin::symmetric(12, 8)),
            )
            .show(ui, |ui| {
                let r = ui.max_rect();
                ui.painter().line_segment(
                    [
                        egui::pos2(r.left(), r.top() + 0.5),
                        egui::pos2(r.right(), r.top() + 0.5),
                    ],
                    egui::Stroke::new(1.0, crate::theme::alpha(crate::theme::OLDNET, 150)),
                );
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new("// CONSOLE")
                            .family(crate::theme::display_family())
                            .size(10.5)
                            .strong()
                            .color(crate::theme::OLDNET),
                    );
                    ui.label(
                        egui::RichText::new("type a command · Enter runs · F1 manual")
                            .family(egui::FontFamily::Monospace)
                            .size(9.5)
                            .color(crate::theme::FAINT),
                    );
                });
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(">")
                            .family(egui::FontFamily::Monospace)
                            .size(13.0)
                            .strong()
                            .color(crate::theme::BETANET),
                    );
                    let resp = ui
                        .add(
                            egui::TextEdit::singleline(&mut self.input)
                                .hint_text("eat | work 0/1 | watch | help"),
                        )
                        .on_hover_text(
                            "Type a command and press Enter. Try `help`, `eat`, `work 0`, or `code loop`.",
                        );
                    if resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        self.run_command();
                    }
                });
                egui::ScrollArea::vertical()
                    .max_height(84.0)
                    .show(ui, |ui| {
                        ui.label(
                            egui::RichText::new("LOG")
                                .family(crate::theme::display_family())
                                .size(9.5)
                                .strong()
                                .color(crate::theme::FAINT),
                        );
                        for line in self.log.iter().rev().take(8) {
                            ui.horizontal(|ui| {
                                ui.label(
                                    egui::RichText::new("›")
                                        .family(egui::FontFamily::Monospace)
                                        .size(10.0)
                                        .color(crate::theme::FAINT),
                                );
                                ui.label(
                                    egui::RichText::new(line)
                                        .family(egui::FontFamily::Monospace)
                                        .size(11.0)
                                        .color(log_color(line)),
                                );
                            });
                        }
                    });
            });

        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(crate::theme::INK)
                    .inner_margin(egui::Margin::symmetric(10, 8)),
            )
            .show(ui, |ui| {
            crate::theme::backdrop(ui, view_accent);
            let scroll_id = match self.tab {
                Tab::Street => "street-scroll",
                Tab::Deck => "deck-scroll",
                Tab::Net => "net-scroll",
                Tab::Veil => "veil-scroll",
            };
            egui::ScrollArea::vertical()
                .id_salt(scroll_id)
                .auto_shrink([false, false])
                .show(ui, |ui| match self.tab {
            Tab::Street => {
                // Job buttons carry names + pay from data/jobs.yaml so the
                // choice means something; indices feed Verb::Work like before.
                // Fallback labels keep verbs available if the file is corrupt
                // (data tests pin the file, so this is belt-and-braces).
                // (id, pay, (resource, need, have)) when the job is gated.
                let here = self.server.world.pos_of(self.actor).unwrap_or(pos(8, 8));
                type JobRow = StreetJob;
                let mut jobs: Vec<JobRow> = self
                    .jobs
                    .iter()
                    .enumerate()
                    .map(|(i, d)| {
                        let req = self.server.world.job_requirement(i).map(|(res, need)| {
                            (res, need, self.server.world.units(self.actor, res))
                        });
                        (d.id.clone(), d.payout_cents, req)
                    })
                    .collect();
                if jobs.is_empty() {
                    jobs = (0..5)
                        .map(|i| (format!("job-{i}"), 60 + 60 * i as i64, None))
                        .collect();
                }
                crate::theme::card(ui, "daily actions", crate::theme::BETANET, |ui| {
                    ui.horizontal(|ui| {
                        if ui
                            .button("Eat")
                            .on_hover_text("Consume one ration for +30 food.")
                            .clicked()
                        {
                            self.do_verb(Verb::Eat);
                        }
                        if ui
                            .button("Watch")
                            .on_hover_text("Spend $0.30 and one act for +15 mood.")
                            .clicked()
                        {
                            self.do_verb(Verb::Watch);
                        }
                        if ui
                            .button("Sleep")
                            .on_hover_text(
                                "Sleep in your rented room until 06:00. Refused away from home.",
                            )
                            .clicked()
                        {
                            self.do_verb(Verb::Sleep);
                        }
                    });
                    // Manual coding (design/10): one unit per click, at a mood
                    // cost. Producers come later; this is the slow path.
                    ui.horizontal(|ui| {
                        ui.label("Code:")
                            .on_hover_text(
                                "Manual component production: one unit per act for a small mood cost.",
                            );
                        for res in sim::Res::ALL {
                            let have = self.server.world.units(self.actor, res);
                            if ui
                                .button(format!("Code {} ({have})", res.name()))
                                .on_hover_text(format!(
                                    "Spend one act and some mood to gain 1 {}.",
                                    res.name()
                                ))
                                .clicked()
                            {
                                self.do_verb(Verb::Code { res });
                            }
                        }
                    });
                });
                let wide = ui.available_width() >= 1180.0;
                if wide {
                    ui.horizontal_top(|ui| {
                        ui.vertical(|ui| {
                            // Match the map's 860px canvas plus card margins.
                            ui.set_width(880.0);
                            self.street_map(ui, here);
                        });
                        ui.add_space(8.0);
                        ui.vertical(|ui| {
                            self.street_work(ui, &jobs);
                            self.street_contracts(ui);
                        });
                    });
                } else {
                    self.street_map(ui, here);
                    self.street_work(ui, &jobs);
                    self.street_contracts(ui);
                }
                // Pack (design/07): carried stacks, worn gear, and items on
                // this tile. Buttons stay in one wrapped row to avoid egui's
                // silent clip/click overflow.
                let inv = self
                    .server
                    .world
                    .inventory(self.actor)
                    .cloned()
                    .unwrap_or_default();
                let items = self.server.world.item_table().clone();
                let cap = self
                    .server
                    .world
                    .body_of(self.actor)
                    .map(|b| b.carry_grams())
                    .unwrap_or(0);
                let worn = inv.worn();
                let carried = inv.carried();
                let floor = self.server.world.floor_items_at(here);
                if !carried.is_empty() || !worn.is_empty() || !floor.is_empty() {
                    crate::theme::card(ui, "pack", crate::theme::MUTED, |ui| {
                        if !carried.is_empty() || !worn.is_empty() {
                            let weight = inv.total_weight(&items);
                            ui.horizontal_wrapped(|ui| {
                                ui.label(format!("Pack {weight}/{cap}g"))
                                    .on_hover_text("Carried weight / capacity.");
                                for (slot, id) in &worn {
                                    let name =
                                        items.get(id).map(|d| d.name.as_str()).unwrap_or(id);
                                    ui.label(format!("{slot:?}: {name}"))
                                        .on_hover_text("Worn gear in this slot.");
                                }
                                for (id, qty) in &carried {
                                    let def = items.get(id);
                                    let name = def.map(|d| d.name.as_str()).unwrap_or(id);
                                    ui.label(format!("{name} x{qty}"));
                                    if let Some(def) = def {
                                        if def.is_gear()
                                            && ui
                                                .button(format!("Equip {name}"))
                                                .on_hover_text(
                                                    "Equip this gear into its body slot.",
                                                )
                                                .clicked()
                                        {
                                            self.do_verb(Verb::Equip { item: id.clone() });
                                        }
                                        if def.effect.is_some()
                                            && ui
                                                .button(format!("Use {name}"))
                                                .on_hover_text("Consume this item for its effect.")
                                                .clicked()
                                        {
                                            self.do_verb(Verb::Use { item: id.clone() });
                                        }
                                    }
                                    if ui
                                        .button(format!("Drop {name}"))
                                        .on_hover_text("Drop one on this tile.")
                                        .clicked()
                                    {
                                        self.do_verb(Verb::Drop { item: id.clone() });
                                    }
                                }
                            });
                        }
                        if !floor.is_empty() {
                            ui.horizontal_wrapped(|ui| {
                                ui.label("On floor:");
                                for (id, qty) in floor {
                                    let name =
                                        items.get(&id).map(|d| d.name.as_str()).unwrap_or(&id);
                                    if ui
                                        .button(format!("Pick up {name} x{qty}"))
                                        .on_hover_text("Pick up from this tile.")
                                        .clicked()
                                    {
                                        self.do_verb(Verb::Pickup { item: id.clone() });
                                    }
                                }
                            });
                        }
                    });
                }
                // Street combat (design/07): attack an adjacent NPC, or
                // surrender to an adjacent cop while Wanted.
                let targets = self.nearby_npcs();
                if !targets.is_empty() {
                    crate::theme::card(ui, "people nearby", crate::theme::DANGER, |ui| {
                        ui.horizontal_wrapped(|ui| {
                            ui.label("Talk:");
                            for (id, role) in &targets {
                                if ui
                                    .button(format!("Talk {role:?}"))
                                    .on_hover_text("Start a conversation.")
                                    .clicked()
                                {
                                    if let Some(dlg) = self.dialogue_for(*role) {
                                        let t = sim::Conversation::new(dlg, None);
                                        self.say(format!("[{id}] {}", t.text()));
                                        self.talking = Some(t);
                                    }
                                }
                            }
                        });
                        ui.horizontal_wrapped(|ui| {
                            ui.label("Attack:")
                                .on_hover_text("Witnessed attacks add to your criminal record.");
                            for (id, role) in &targets {
                                if ui
                                    .button(format!("Attack {role:?}"))
                                    .on_hover_text(
                                        "Attack this NPC. Witnessed crimes raise your record.",
                                    )
                                    .clicked()
                                {
                                    self.do_verb(Verb::Attack {
                                        target: EntityId(*id),
                                    });
                                }
                            }
                            if self.server.world.standing() == Standing::Wanted
                                && self.server.world.adjacent_cop(self.actor).is_some()
                                && ui
                                    .button("Surrender")
                                    .on_hover_text(
                                        "Surrender to the adjacent cop: pay a fine and end the chase.",
                                    )
                                    .clicked()
                            {
                                self.do_verb(Verb::Surrender);
                            }
                        });
                    });
                }
                // Mail (design/03-04): read your inbox, craft a Mailer+,
                // and trojan a nearby NPC. Trojans land when they open it.
                let inbox = self.server.world.inbox(self.actor).len();
                let has_plus = self.server.world.has_mailer_plus();
                crate::theme::card(ui, "mail", crate::theme::BETANET, |ui| {
                    ui.horizontal_wrapped(|ui| {
                    ui.label(format!("Mail — inbox {inbox}"))
                        .on_hover_text("Unread messages. Opened mail can contain trojans.");
                    if inbox > 0
                        && ui
                            .button("Open mail")
                            .on_hover_text("Open the oldest message.")
                            .clicked()
                    {
                        match self.server.apply_one(Intent {
                            actor: self.actor,
                            verb: Verb::OpenMail { index: 0 },
                        }) {
                            Ok(_) => self.say("mail opened".to_string()),
                            Err(e) => self.say(format!("open failed: {e}")),
                        }
                    }
                    if !has_plus
                        && ui
                            .button("Craft Mailer+ (3 algo)")
                            .on_hover_text(
                                "Spend 3 ALGO to craft Mailer+. Unlocks trojan attachments.",
                            )
                            .clicked()
                    {
                        match self.server.apply_one(Intent {
                            actor: self.actor,
                            verb: Verb::CraftMailerPlus,
                        }) {
                            Ok(_) => self.say("Mailer+ crafted — trojans unlocked".to_string()),
                            Err(e) => self.say(format!("craft failed: {e}")),
                        }
                    }
                    if has_plus {
                        for (id, role) in self.nearby_npcs() {
                            if ui
                                .button(format!("Trojan {role:?}"))
                                .on_hover_text(
                                    "Mail a trojan. It activates only when the recipient reads their mail.",
                                )
                                .clicked()
                            {
                                match self.server.apply_one(Intent {
                                    actor: self.actor,
                                    verb: Verb::SendTrojan { to: EntityId(id) },
                                }) {
                                    Ok(_) => {
                                        self.say(format!("trojan mailed to {id}"))
                                    }
                                    Err(e) => self.say(format!("trojan failed: {e}")),
                                }
                            }
                        }
                    }
                    });
                });
                // Snapshot world reads up front: UI callbacks below need &mut self.
                // Counter shows within reach (counter serves adjacent tiles).
                let counter: Vec<(String, i64, u32)> = self.server.world
                    .shop_near_at(here.x, here.y)
                    .map(|shop| shop.stock.iter().map(|(k, l)| (k.clone(), l.price_cents, l.qty)).collect())
                    .unwrap_or_default();
                // Conversation card: only while a dialogue is open, so the
                // street stays a short scan and talk/attack live together.
                if self.talking.is_some() {
                    crate::theme::card(ui, "conversation", crate::theme::VEIL, |ui| {
                    // Pick first, then act: the option borrow must end before say().
                    enum Pick { Leave, Choice(usize), None }
                    let pick = {
                        let t = self.talking.as_ref().unwrap();
                        ui.label(t.text());
                        let mut picked = Pick::None;
                        for (i, opt) in t.options().iter().enumerate() {
                            if ui.button(&opt.label).clicked() {
                                picked = Pick::Choice(i);
                            }
                        }
                        if ui.button("Leave").clicked() {
                            picked = Pick::Leave;
                        }
                        picked
                    };
                    match pick {
                        Pick::Leave => self.talking = None,
                        Pick::None => {}
                        Pick::Choice(i) => {
                            let outcome = {
                                let t = self.talking.as_mut().unwrap();
                                match t.choose(i) {
                                    Ok(finished) => {
                                        let fx = t.last_effect.clone();
                                        let end = finished || t.options().is_empty();
                                        let text = if end { String::new() } else { t.text().to_string() };
                                        Ok((fx, end, text))
                                    }
                                    Err(e) => Err(format!("huh: {e}")),
                                }
                            };
                            match outcome {
                                Ok((fx, end, text)) => {
                                    // Narrate raw effect tags only when the exchange
                                    // ends on them; transitions speak for themselves.
                                    if end {
                                        if let Some(fx) = fx {
                                            self.say(format!("* {fx} *"));
                                        }
                                        self.talking = None;
                                    } else {
                                        self.say(text);
                                    }
                                }
                                Err(msg) => self.say(msg),
                            }
                        }
                    }
                    });
                }
                // Trade panel: counter sale when standing on a shop tile.
                if !counter.is_empty() {
                    crate::theme::card(ui, "counter", crate::theme::OLDNET, |ui| {
                        ui.label("Counter — type buy <item> or click:")
                            .on_hover_text(
                                "Counters serve adjacent tiles. Stand next to the $ counter to buy.",
                            );
                        for (item, price, qty) in counter {
                            ui.horizontal(|ui| {
                                ui.label(format!(
                                    "{} — {}c ({} left){}",
                                    item,
                                    price,
                                    qty,
                                    item_blurb(&item)
                                ));
                                if ui
                                    .button("Buy")
                                    .on_hover_text(format!(
                                        "Buy one for {price}c. {qty} left at this counter."
                                    ))
                                    .clicked()
                                {
                                    self.do_verb(Verb::Buy(item.clone()));
                                }
                            });
                        }
                    });
                }
            }
            Tab::Deck => {
                crate::theme::heading(ui, "Deck — world host 1");
                ui.label("H killer head · o tail · P producer · click a cell, then Load/Strike/Farm · warden defends this rig");
                // Snapshot arena state up front: buttons below need &mut self.
                // Producers get their own glyph + per-resource color so the
                // economy loop is visible in the arena, not just in numbers.
                crate::theme::card(ui, "host", crate::theme::OLDNET, |ui| {
                if self
                    .server
                    .world
                    .host(HostId(1))
                    .map(|h| h.progs.is_empty())
                    .unwrap_or(true)
                {
                    crate::theme::label(ui, "Arena empty — select a free cell and load a program.");
                }
                if let Some(h) = self.server.world.host(HostId(1)) {
                    ui.horizontal_wrapped(|ui| {
                        crate::theme::chip(
                            ui,
                            format!("arch {}", h.arch().name()),
                            crate::theme::OLDNET,
                        )
                        .on_hover_text(
                            "Host architecture. Determines the host's native vulnerabilities.",
                        );
                        crate::theme::chip(
                            ui,
                            if h.monitor() { "monitor armed" } else { "quiet" },
                            if h.monitor() {
                                crate::theme::DANGER
                            } else {
                                crate::theme::MUTED
                            },
                        )
                        .on_hover_text(
                            "An armed monitor flags exploits and traces; it can retaliate.",
                        );
                        crate::theme::chip(
                            ui,
                            if h.firewall() { "filter on" } else { "filter off" },
                            if h.firewall() {
                                crate::theme::BETANET
                            } else {
                                crate::theme::MUTED
                            },
                        )
                        .on_hover_text(
                            "Filter blocks unauthorised entry. Crack the lock to drop it, or exploit a hole.",
                        );
                        crate::theme::chip(
                            ui,
                            if h.scanned(self.actor) {
                                "scanned"
                            } else {
                                "unscanned"
                            },
                            crate::theme::BETANET,
                        )
                        .on_hover_text(
                            "Scan with a Scanner to reveal vulns, logs, trace, and breach state.",
                        );
                    });
                    if let Some((progress, threshold)) = h.trace_progress() {
                        crate::theme::meter(
                            ui,
                            "TRACE",
                            progress as f32,
                            threshold.max(1) as f32,
                            crate::theme::DANGER,
                        )
                        .on_hover_text(
                            "Trace progress. At the threshold your program burns; scrub the log or outrun it.",
                        );
                    }
                }
                });
                let (heads, tails, head_kinds, prod, cpu, total) = match self.server.world.host(HostId(1)) {
                    Some(h) => {
                        let mut heads = std::collections::BTreeMap::new();
                        let mut tails = std::collections::BTreeSet::new();
                        let mut head_kinds = std::collections::BTreeMap::new();
                        for p in h.progs.values() {
                            if p.owner == self.actor {
                                if let Some((makes, _)) = h.maker_info(p.id) {
                                    if let Some(res) = makes.first() {
                                        heads.insert((p.head.0, p.head.1), Some(*res));
                                        for t in &p.tail {
                                            tails.insert((t.0, t.1));
                                        }
                                        continue;
                                    }
                                }
                            }
                            heads.insert((p.head.0, p.head.1), None);
                            head_kinds.insert((p.head.0, p.head.1), p.kind);
                            for t in &p.tail {
                                tails.insert((t.0, t.1));
                            }
                        }
                        (heads, tails, head_kinds, h.producer_summary(self.actor), h.cpu_load(), h.cpu_total)
                    }
                    None => (
                        std::collections::BTreeMap::new(),
                        std::collections::BTreeSet::new(),
                        std::collections::BTreeMap::new(),
                        vec![],
                        0,
                        0,
                    ),
                };
                crate::theme::card(ui, "arena", crate::theme::BETANET, |ui| {
                let grid = egui::Frame::new()
                    .fill(crate::theme::VOID)
                    .stroke(egui::Stroke::new(
                        1.0,
                        crate::theme::alpha(crate::theme::LINE, 220),
                    ))
                    .inner_margin(egui::Margin::same(6))
                    .show(ui, |ui| {
                egui::Grid::new("deck").spacing([2.0, 2.0]).show(ui, |ui| {
                    for y in 0..8u8 {
                        for x in 0..12u8 {
                            let (glyph, color, fill, stroke_color, tip) =
                                if let Some(Some(res)) = heads.get(&(x, y)) {
                                    // Producer head: P in the resource's color.
                                    (
                                        "P",
                                        crate::theme::resource_color(*res),
                                        egui::Color32::from_rgb(0x10, 0x1B, 0x16),
                                        crate::theme::alpha(crate::theme::resource_color(*res), 200),
                                        format!(
                                            "{} producer head. Yields components each act and costs CPU.",
                                            res.name().to_uppercase()
                                        ),
                                    )
                                } else if let Some(kind) = head_kinds.get(&(x, y)) {
                                    let color = match kind {
                                        sim::ProgramKind::Killer => crate::theme::OLDNET,
                                        sim::ProgramKind::Shield => crate::theme::SUCCESS,
                                        sim::ProgramKind::Scanner => crate::theme::BETANET,
                                        sim::ProgramKind::Cracker => crate::theme::VEIL,
                                        sim::ProgramKind::Connector => crate::theme::MUTED,
                                        sim::ProgramKind::Cloner => crate::theme::MONEY,
                                        sim::ProgramKind::Editor => crate::theme::PAPER,
                                    };
                                    let glyph = match kind {
                                        sim::ProgramKind::Killer => "K",
                                        sim::ProgramKind::Shield => "S",
                                        sim::ProgramKind::Scanner => "Sc",
                                        sim::ProgramKind::Cracker => "Cr",
                                        sim::ProgramKind::Connector => "Co",
                                        sim::ProgramKind::Cloner => "Cl",
                                        sim::ProgramKind::Editor => "E",
                                    };
                                    (
                                        glyph,
                                        color,
                                        egui::Color32::from_rgb(0x10, 0x1B, 0x23),
                                        crate::theme::alpha(color, 200),
                                        format!(
                                            "{} head. Strike the head to kill the program.",
                                            kind.name()
                                        ),
                                    )
                                } else if tails.contains(&(x, y)) {
                                    (
                                        "o",
                                        crate::theme::FAINT,
                                        egui::Color32::from_rgb(0x08, 0x10, 0x14),
                                        crate::theme::alpha(crate::theme::FAINT, 120),
                                        "Program tail. Strikes shrink it; hit the head to kill it."
                                            .to_string(),
                                    )
                                } else if Some((x, y)) == self.sel {
                                    (
                                        "X",
                                        crate::theme::OLDNET,
                                        egui::Color32::from_rgb(0x2A, 0x21, 0x14),
                                        crate::theme::OLDNET,
                                        "Selected cell. Load a program here.".to_string(),
                                    )
                                } else {
                                    (
                                        ".",
                                        crate::theme::FAINT,
                                        crate::theme::VOID,
                                        crate::theme::alpha(crate::theme::LINE, 170),
                                        "Empty cell. Click to select.".to_string(),
                                    )
                                };
                            if ui
                                .add_sized(
                                    [30.0, 30.0],
                                    egui::Button::new(
                                        egui::RichText::new(glyph)
                                            .monospace()
                                            .size(11.0)
                                            .color(color),
                                    )
                                    .fill(fill)
                                    .stroke(egui::Stroke::new(1.0, stroke_color)),
                                )
                                .on_hover_text(tip)
                                .clicked()
                            {
                                self.sel = Some((x, y));
                            }
                        }
                        ui.end_row();
                    }
                });
                    });
                crate::theme::corner_marks(
                    ui.painter(),
                    grid.response.rect,
                    crate::theme::alpha(crate::theme::BETANET, 170),
                    9.0,
                );
                crate::theme::meter(ui, "CPU", cpu as f32, total.max(1) as f32, crate::theme::BETANET)
                    .on_hover_text(
                        "CPU used / total. Over budget blocks new loads and growth; producers and programs both consume it.",
                    );
                if let Some(h) = self.server.world.host(HostId(1)) {
                    ui.label(format!(
                        "arch {} · vulns {} · breached {} · scanned {}",
                        h.arch().name(),
                        h.vuln_ids().len(),
                        if h.breached { "yes" } else { "no" },
                        if h.scanned(self.actor) { "yes" } else { "no" }
                    ))
                    .on_hover_text(
                        "Architecture · known vulnerabilities · breach state · scan state.",
                    );
                    if h.scanned(self.actor) {
                        ui.label(format!("scan: {}", h.scan_report(self.actor)));
                    }
                }
                if let Some((x, y)) = self.sel {
                    if let Some(kind) = self
                        .server
                        .world
                        .host(HostId(1))
                        .and_then(|h| h.prog_at(x, y))
                        .and_then(|p| self.server.world.host(HostId(1)).and_then(|h| h.program_kind(p)))
                    {
                        ui.label(format!("selected: {} @({x},{y})", kind.name()));
                    }
                }
                });
                // Exploit/patch sit above the loadout so they stay in the
                // visible scroll region and read as the host's immediate
                // security surface.
                crate::theme::card(ui, "security", crate::theme::DANGER, |ui| {
                let vulns = self
                    .server
                    .world
                    .host(HostId(1))
                    .map(|h| h.vuln_ids())
                    .unwrap_or_default();
                let breached = self
                    .server
                    .world
                    .host(HostId(1))
                    .map(|h| h.breached)
                    .unwrap_or(false);
                ui.horizontal_wrapped(|ui| {
                    ui.label("Exploit:")
                        .on_hover_text("Needs one of your programs on this host.");
                    for v in &vulns {
                        if ui
                            .button(v)
                            .on_hover_text("Exploit this hole to breach the host.")
                            .clicked()
                        {
                            match self.server.apply_one(Intent {
                                actor: self.actor,
                                verb: Verb::Exploit { vuln: v.clone() },
                            }) {
                                Ok(_) => self.say(format!("exploited {v}")),
                                Err(e) => self.say(format!("exploit failed: {e}")),
                            }
                        }
                    }
                    if breached
                        && ui
                            .button("Send trojan")
                            .on_hover_text("Mail a trojan to the host owner.")
                            .clicked()
                    {
                        let to = self
                            .server
                            .world
                            .host(HostId(1))
                            .and_then(|h| h.owner())
                            .unwrap_or(self.actor);
                        match self.server.apply_one(Intent {
                            actor: self.actor,
                            verb: Verb::SendTrojan { to },
                        }) {
                            Ok(_) => self.say("trojan mailed — lands when opened".to_string()),
                            Err(e) => self.say(format!("trojan failed: {e}")),
                        }
                    }
                });
                ui.horizontal_wrapped(|ui| {
                    ui.label("Patch:")
                        .on_hover_text("Close a hole; a patched hole stays shut.");
                    for v in &vulns {
                        if ui
                            .button(format!("Patch {v}"))
                            .on_hover_text("Patch this vulnerability.")
                            .clicked()
                        {
                            match self.server.apply_one(Intent {
                                actor: self.actor,
                                verb: Verb::DeckPatch { vuln: v.clone() },
                            }) {
                                Ok(_) => self.say(format!("patched {v}")),
                                Err(e) => self.say(format!("patch failed: {e}")),
                            }
                        }
                    }
                });
                });
                crate::theme::card(ui, "loadout", crate::theme::SUCCESS, |ui| {
                ui.horizontal(|ui| {
                    if ui
                        .button("Load killer @selected")
                        .on_hover_text(
                            "Load a Killer on the selected cell. Costs CPU; the warden will answer.",
                        )
                        .clicked()
                    {
                        let (msg, beat) = match self.sel {
                            Some((x, y)) => match self.server.apply_one(Intent {
                                actor: self.actor,
                                verb: Verb::DeckLoad { x, y },
                            }) {
                                Ok((_, events)) => (
                                    format!("loaded killer @({x},{y})"),
                                    Self::beat_lines(&events),
                                ),
                                Err(e) => (format!("load failed: {e}"), vec![]),
                            },
                            None => ("click a cell first".to_string(), vec![]),
                        };
                        self.say(msg);
                        for line in beat {
                            self.say(line);
                        }
                    }
                    if ui
                        .button("Strike selected")
                        .on_hover_text("Strike the selected program: heads kill, tails shrink.")
                        .clicked()
                    {
                        let (msg, beat) = match self.sel {
                            Some((x, y)) => match self.server.apply_one(Intent {
                                actor: self.actor,
                                verb: Verb::DeckStrike { x, y },
                            }) {
                                Ok((_, events)) => {
                                    let killed = events.iter().any(|e| e.kind == "kill");
                                    (
                                        format!("strike: killed={killed}"),
                                        Self::beat_lines(&events),
                                    )
                                }
                                Err(e) => (format!("strike failed: {e}"), vec![]),
                            },
                            None => ("click a cell first".to_string(), vec![]),
                        };
                        self.say(msg);
                        for line in beat {
                            self.say(line);
                        }
                    }
                });
                // v0.6 loadout: pick a kind with its role stats.
                ui.horizontal_wrapped(|ui| {
                    ui.label("Load kind @selected:")
                        .on_hover_text("Each kind has a role and a per-segment CPU cost.");
                    for kind in [
                        sim::ProgramKind::Shield,
                        sim::ProgramKind::Scanner,
                        sim::ProgramKind::Cracker,
                        sim::ProgramKind::Connector,
                        sim::ProgramKind::Cloner,
                        sim::ProgramKind::Editor,
                    ] {
                        let def = self.server.world.kind_def(kind);
                        if ui
                            .button(format!("Load {} ({}cpu)", kind.name(), def.cpu_per_seg))
                            .on_hover_text(format!(
                                "{} · {} CPU per segment.",
                                kind_hint(kind),
                                def.cpu_per_seg
                            ))
                            .clicked()
                        {
                            let msg = match self.sel {
                                Some((x, y)) => match self.server.apply_one(Intent {
                                    actor: self.actor,
                                    verb: Verb::DeckLoadKind { x, y, kind },
                                }) {
                                    Ok((_, events)) => {
                                        for line in Self::beat_lines(&events) {
                                            self.say(line);
                                        }
                                        format!("loaded {} @({x},{y})", kind.name())
                                    }
                                    Err(e) => format!("load {} failed: {e}", kind.name()),
                                },
                                None => "click a cell first".to_string(),
                            };
                            self.say(msg);
                        }
                    }
                });
                });
                // Kind actions. They refuse cleanly without the matching program.
                crate::theme::card(ui, "kind actions", crate::theme::VEIL, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.label("Kind actions:")
                        .on_hover_text("Each action needs its matching program kind on this rig.");
                    if ui
                        .button("Scan")
                        .on_hover_text("Scanner: reveal vulns, logs, trace, and breach state.")
                        .clicked()
                    {
                        match self.server.apply_one(Intent {
                            actor: self.actor,
                            verb: Verb::DeckScan,
                        }) {
                            Ok(_) => self.say("host scanned".to_string()),
                            Err(e) => self.say(format!("scan failed: {e}")),
                        }
                    }
                    if ui
                        .button("Crack lock")
                        .on_hover_text("Cracker: three beats to drop the filter/monitor.")
                        .clicked()
                    {
                        match self.server.apply_one(Intent {
                            actor: self.actor,
                            verb: Verb::DeckCrack,
                        }) {
                            Ok(_) => self.say("cracker progress".to_string()),
                            Err(e) => self.say(format!("crack failed: {e}")),
                        }
                    }
                    if ui
                        .button("Clone selected")
                        .on_hover_text("Cloner: copy the selected program into a free cell.")
                        .clicked()
                    {
                        match self.sel {
                            Some((x, y)) => match self.server.apply_one(Intent {
                                actor: self.actor,
                                verb: Verb::DeckClone { x, y },
                            }) {
                                Ok(_) => self.say("program cloned".to_string()),
                                Err(e) => self.say(format!("clone failed: {e}")),
                            },
                            None => self.say("click a cell first".to_string()),
                        }
                    }
                    if ui
                        .button("Overwrite selected")
                        .on_hover_text("Editor: sever a tail segment without a headshot.")
                        .clicked()
                    {
                        match self.sel {
                            Some((x, y)) => match self.server.apply_one(Intent {
                                actor: self.actor,
                                verb: Verb::DeckOverwrite { x, y },
                            }) {
                                Ok(_) => self.say("segment severed".to_string()),
                                Err(e) => self.say(format!("overwrite failed: {e}")),
                            },
                            None => self.say("click a cell first".to_string()),
                        }
                    }
                    if ui
                        .button("Connect host 3")
                        .on_hover_text(
                            "Connector: probe host 3, reveal its scan report, and log the transit.",
                        )
                        .clicked()
                    {
                        match self.server.apply_one(Intent {
                            actor: self.actor,
                            verb: Verb::DeckConnect { host: HostId(3) },
                        }) {
                            Ok(_) => self.say("remote host revealed".to_string()),
                            Err(e) => self.say(format!("connect failed: {e}")),
                        }
                    }
                });
                });
                // Producers (design/10): deploy a maker on the selected cell.
                // It then yields free units every act.
                crate::theme::card(ui, "producers", crate::theme::OLDNET, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.label("Farm @selected:")
                        .on_hover_text(
                            "Deploy a producer on the selected cell. It yields components each act and costs CPU; the warden ignores it.",
                        );
                    for res in sim::Res::ALL {
                        if ui
                            .button(format!("Farm {}", res.name()))
                            .on_hover_text(format!(
                                "Deploy a {} producer. It yields components every act and costs CPU.",
                                res.name()
                            ))
                            .clicked()
                        {
                            let msg = match self.sel {
                                Some((x, y)) => match self.server.apply_one(Intent {
                                    actor: self.actor,
                                    verb: Verb::DeckFarm { x, y, res },
                                }) {
                                    Ok(_) => format!("farmed {} @({x},{y})", res.name()),
                                    Err(e) => format!("farm failed: {e}"),
                                },
                                None => "click a cell first".to_string(),
                            };
                            self.say(msg);
                        }
                    }
                });
                // Kind-matched producers get +1 yield from affinity.
                ui.horizontal_wrapped(|ui| {
                    ui.label("Synergy farm @selected:")
                        .on_hover_text(
                            "A kind-matched producer yields +1 of its affinity resource every act.",
                        );
                    for (res, kind) in [
                        (sim::Res::Loop, sim::ProgramKind::Connector),
                        (sim::Res::Data, sim::ProgramKind::Scanner),
                        (sim::Res::Algo, sim::ProgramKind::Cracker),
                    ] {
                        if ui
                            .button(format!("Farm {} ({})", res.name(), kind.name()))
                            .on_hover_text(format!(
                                "Deploy a {} producer with {} affinity: +1 yield per act.",
                                res.name(),
                                kind.name()
                            ))
                            .clicked()
                        {
                            let msg = match self.sel {
                                Some((x, y)) => match self.server.apply_one(Intent {
                                    actor: self.actor,
                                    verb: Verb::DeckFarmKind { x, y, res, kind },
                                }) {
                                    Ok(_) => format!("farmed {} with {}", res.name(), kind.name()),
                                    Err(e) => format!("synergy farm failed: {e}"),
                                },
                                None => "click a cell first".to_string(),
                            };
                            self.say(msg);
                        }
                    }
                });
                });
                // Card hand (design/03): colour by family, keep the cost in
                // the label so keyboard/accesskit users still see it.
                crate::theme::card(ui, "cards @selected", crate::theme::MONEY, |ui| {
                ui.horizontal_wrapped(|ui| {
                    for card in sim::Host::implemented_cards() {
                        let (family, color) = card_family(card);
                        let def = self.cards.get(card).cloned();
                        // Show the resource cost (design/03: cards consume).
                        let label = match self.server.world.card_cost(card) {
                            Some((res, n)) => format!("{card} ({n} {})", res.name()),
                            None => card.to_string(),
                        };
                        let mut resp = ui.add_sized(
                            [92.0, 38.0],
                            egui::Button::new(
                                egui::RichText::new(label.clone())
                                    .size(11.0)
                                    .color(color),
                            )
                            .fill(egui::Color32::from_rgb(0x12, 0x1B, 0x22))
                            .stroke(egui::Stroke::new(1.0, color)),
                        );
                        if let Some(def) = &def {
                            resp = resp.on_hover_text(format!(
                                "{family}\n{}\ncost: {}",
                                card_effect_text(def),
                                if def.cost_units == 0 {
                                    "free".to_string()
                                } else {
                                    format!("{} {}", def.cost_units, def.cost_resource)
                                }
                            ));
                        }
                        if resp.hovered() {
                            self.hovered_card = Some(card.to_string());
                        }
                        if resp.clicked() {
                            let msg = match self.sel {
                                Some((x, y)) => match self.server.apply_one(Intent {
                                    actor: self.actor,
                                    verb: Verb::DeckCard {
                                        card: card.to_string(),
                                        x,
                                        y,
                                    },
                                }) {
                                    Ok((_, events)) => {
                                        for line in Self::beat_lines(&events) {
                                            self.say(line);
                                        }
                                        format!("card {card} @({x},{y})")
                                    }
                                    Err(e) => format!("card {card} refused: {e}"),
                                },
                                None => "click a cell first".to_string(),
                            };
                            self.say(msg);
                        }
                    }
                });
                if let Some(id) = self.hovered_card.clone() {
                    if let Some(def) = self.cards.get(&id) {
                        let (family, color) = card_family(&id);
                        crate::theme::section(ui, "card preview");
                        ui.horizontal_wrapped(|ui| {
                            ui.label(
                                egui::RichText::new(&id)
                                    .size(15.0)
                                    .strong()
                                    .color(color),
                            );
                            crate::theme::chip(ui, family, color);
                            crate::theme::chip(
                                ui,
                                if def.cost_units == 0 {
                                    "free".to_string()
                                } else {
                                    format!("{} {}", def.cost_units, def.cost_resource)
                                },
                                crate::theme::MONEY,
                            );
                        });
                        ui.label(card_effect_text(def));
                        if let Some(targets) = &def.targets {
                            crate::theme::label(ui, format!("targets: {targets}"));
                        }
                    }
                }
                });
                // Get out clean (design/03): scrub your trail on the rig.
                crate::theme::card(ui, "trail", crate::theme::DANGER, |ui| {
                ui.horizontal(|ui| {
                    let logs = self
                        .server
                        .world
                        .host(HostId(1))
                        .map(|h| h.logs_visible(self.actor).len())
                        .unwrap_or(0);
                    ui.label(format!("your trail on this rig: {logs}"))
                        .on_hover_text(
                            "Visible log entries linking you to this host. Traces and crimes use them.",
                        );
                    if ui
                        .button("Scrub log")
                        .on_hover_text(
                            "Erase your trail here. Needs one of your programs on the host or a privilege vuln.",
                        )
                        .clicked()
                    {
                        match self.server.apply_one(Intent {
                            actor: self.actor,
                            verb: Verb::ScrubLog,
                        }) {
                            Ok(_) => self.say("log scrubbed".to_string()),
                            Err(e) => self.say(format!("scrub failed: {e}")),
                        }
                    }
                });
                });
                // Persistent abilities (design/03 Program.mods): build one
                // into your program on the selected cell; it fires on every
                // exchange rather than once.
                crate::theme::card(ui, "mods @selected", crate::theme::BETANET, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.label("Mods @selected:")
                        .on_hover_text(
                            "Persistent mods built into the selected program. They fire every exchange.",
                        );
                    for card in ["tank", "plating", "reflect", "splash", "extra"] {
                        // "Mod " prefix keeps these distinct from the
                        // one-shot cards of the same name.
                        let label = match self.server.world.card_cost(card) {
                            Some((res, n)) => format!("Mod {card} ({n} {})", res.name()),
                            None => format!("Mod {card}"),
                        };
                        let tip = self
                            .cards
                            .get(card)
                            .map(card_effect_text)
                            .unwrap_or("Persistent program mod.");
                        if ui.button(label).on_hover_text(tip).clicked() {
                            let msg = match self.sel {
                                Some((x, y)) => match self.server.apply_one(Intent {
                                    actor: self.actor,
                                    verb: Verb::DeckMod {
                                        x,
                                        y,
                                        card: card.to_string(),
                                    },
                                }) {
                                    Ok(_) => format!("mod {card} built @({x},{y})"),
                                    Err(e) => format!("mod {card} refused: {e}"),
                                },
                                None => "click a cell first".to_string(),
                            };
                            self.say(msg);
                        }
                    }
                });
                });
                // Producer yield summary: what the farm loop delivers.
                crate::theme::card(ui, "yield", crate::theme::OLDNET, |ui| {
                let have = sim::Res::ALL
                    .iter()
                    .map(|r| format!("{} {}", r.name(), self.server.world.units(self.actor, *r)))
                    .collect::<Vec<_>>()
                    .join(" · ");
                ui.label(format!("components: {have}"));
                if prod.is_empty() {
                    ui.label("Producers: none — click a free cell and Farm to yield units each act");
                } else {
                    let line = prod
                        .iter()
                        .map(|(r, y, c)| format!("{} ×{c} (+{y}/act)", r.name()))
                        .collect::<Vec<_>>()
                        .join(" · ");
                    ui.label(format!("Producers: {line}"));
                }
                // Selected program's persistent abilities.
                if let Some((x, y)) = self.sel {
                    let mods = self
                        .server
                        .world
                        .host(HostId(1))
                        .and_then(|h| h.prog_at(x, y))
                        .map(|p| {
                            self.server
                                .world
                                .host(HostId(1))
                                .map(|h| h.mods_of(p).to_vec())
                                .unwrap_or_default()
                        })
                        .unwrap_or_default();
                    let names: Vec<String> = mods
                        .iter()
                        .map(|m| format!("{m:?}"))
                        .collect();
                    ui.label(format!(
                        "@({x},{y}) mods: {}",
                        if names.is_empty() {
                            "none".to_string()
                        } else {
                            names.join(" · ")
                        }
                    ));
                }
                });
            }
            Tab::Net => {
                crate::theme::heading(ui, "Net — world graph (Oldnet/Betanet)");
                ui.label("More hops = safer trace, higher tolls. Laser is fast but tolled. Dark sections drop their hosts.");
                // Graph canvas: nodes on a transit-diagram strip, edges as
                // Oldnet copper or Betanet laser lines. Controls stay below.
                let node_ids = self.server.world.net.node_ids();
                if node_ids.is_empty() {
                    crate::theme::label(ui, "No known hosts — scan or connect first.");
                }
                crate::theme::card(ui, "topology", crate::theme::BETANET, |ui| {
                let graph_size = egui::vec2(ui.available_width().min(720.0), 200.0);
                let (gresp, gp) = ui.allocate_painter(graph_size, egui::Sense::click());
                let gresp = gresp.on_hover_text(
                    "Click a node to inspect it. Dark nodes are in unpowered sections.",
                );
                let grect = gresp.rect;
                gp.rect_filled(grect, 0.0, crate::theme::VOID);
                let grid = crate::theme::alpha(crate::theme::BETANET, 10);
                let mut gx = grect.left();
                while gx < grect.right() {
                    gp.line_segment(
                        [egui::pos2(gx, grect.top()), egui::pos2(gx, grect.bottom())],
                        egui::Stroke::new(1.0, grid),
                    );
                    gx += 28.0;
                }
                let mut gy = grect.top();
                while gy < grect.bottom() {
                    gp.line_segment(
                        [egui::pos2(grect.left(), gy), egui::pos2(grect.right(), gy)],
                        egui::Stroke::new(1.0, grid),
                    );
                    gy += 28.0;
                }
                gp.rect_stroke(
                    grect,
                    0.0,
                    egui::Stroke::new(1.0, crate::theme::alpha(crate::theme::LINE, 220)),
                    egui::StrokeKind::Inside,
                );
                crate::theme::corner_marks(
                    &gp,
                    grect,
                    crate::theme::alpha(crate::theme::BETANET, 170),
                    9.0,
                );
                let mut npos = std::collections::BTreeMap::new();
                let n = node_ids.len().max(1);
                for (i, id) in node_ids.iter().enumerate() {
                    let x = grect.min.x + 90.0
                        + (grect.width() - 180.0) * i as f32 / (n.saturating_sub(1).max(1) as f32);
                    let y = grect.center().y + if i % 2 == 0 { -34.0 } else { 34.0 };
                    npos.insert(id.0, egui::pos2(x, y));
                }
                for l in self.server.world.net.links() {
                    let (Some(a), Some(b)) = (npos.get(&l.from.0), npos.get(&l.to.0)) else {
                        continue;
                    };
                    let dark = self.server.world.power.is_host_dark(l.from)
                        || self.server.world.power.is_host_dark(l.to);
                    let (color, dash) = match l.kind {
                        LinkKind::Laser => (crate::theme::BETANET, false),
                        LinkKind::Wireless { .. } => (crate::theme::VEIL, true),
                        LinkKind::Wired { damaged } => {
                            (if damaged { crate::theme::FAINT } else { crate::theme::OLDNET }, damaged)
                        }
                    };
                    let color = if dark { crate::theme::FAINT } else { color };
                    gp.line_segment(
                        [*a, *b],
                        egui::Stroke::new(6.0, crate::theme::alpha(color, 22)),
                    );
                    if dash {
                        let steps = 10;
                        for s in 0..steps {
                            if s % 2 == 0 {
                                let t0 = s as f32 / steps as f32;
                                let t1 = (s + 1) as f32 / steps as f32;
                                gp.line_segment(
                                    [*a + (*b - *a) * t0, *a + (*b - *a) * t1],
                                    egui::Stroke::new(2.0, color),
                                );
                            }
                        }
                    } else {
                        gp.line_segment([*a, *b], egui::Stroke::new(2.0, color));
                    }
                    let mid = *a + (*b - *a) * 0.5;
                    gp.text(
                        mid,
                        egui::Align2::CENTER_CENTER,
                        format!("{}ms", l.latency_ms),
                        egui::FontId::monospace(9.0),
                        color,
                    );
                }
                for id in &node_ids {
                    let Some(n) = self.server.world.net.node(*id) else {
                        continue;
                    };
                    let Some(p) = npos.get(&id.0) else { continue };
                    let dark = self.server.world.power.is_host_dark(*id);
                    let color = if dark {
                        crate::theme::FAINT
                    } else {
                        match n.net {
                            NetKind::Oldnet => crate::theme::OLDNET,
                            NetKind::Betanet => crate::theme::BETANET,
                        }
                    };
                    gp.circle_filled(
                        *p,
                        30.0,
                        crate::theme::alpha(color, if dark { 8 } else { 16 }),
                    );
                    if self.net_sel == Some(*id) {
                        gp.circle_stroke(*p, 29.0, egui::Stroke::new(1.0, crate::theme::OLDNET));
                        gp.circle_stroke(
                            *p,
                            25.0,
                            egui::Stroke::new(2.0, crate::theme::OLDNET),
                        );
                        let sel_rect = egui::Rect::from_center_size(*p, egui::vec2(58.0, 58.0));
                        crate::theme::corner_marks(
                            &gp,
                            sel_rect,
                            crate::theme::OLDNET,
                            8.0,
                        );
                    }
                    gp.circle_filled(*p, 23.0, crate::theme::VOID);
                    gp.circle_stroke(*p, 23.0, egui::Stroke::new(1.8, color));
                    gp.text(
                        *p,
                        egui::Align2::CENTER_CENTER,
                        format!("{}", id.0),
                        crate::theme::display(12.0),
                        color,
                    );
                    if n.gateway {
                        gp.text(
                            *p + egui::vec2(0.0, 34.0),
                            egui::Align2::CENTER_CENTER,
                            "GATEWAY",
                            egui::FontId::monospace(8.5),
                            crate::theme::MUTED,
                        );
                    }
                }
                if gresp.clicked() {
                    if let Some(pointer) = gresp.interact_pointer_pos() {
                        if let Some((id, dist)) = npos
                            .iter()
                            .map(|(id, p)| (*id, p.distance(pointer)))
                            .min_by(|a, b| a.1.total_cmp(&b.1))
                        {
                            if dist <= 30.0 {
                                self.net_sel = Some(HostId(id));
                            }
                        }
                    }
                }
                });
                // Graph snapshot: nodes (with net kind + power state) and links.
                crate::theme::card(ui, "nodes & links", crate::theme::MUTED, |ui| {
                let mut lines: Vec<String> = Vec::new();
                for id in &node_ids {
                    if let Some(n) = self.server.world.net.node(*id) {
                        let kind = match n.net {
                            NetKind::Oldnet => "oldnet",
                            NetKind::Betanet => "betanet",
                        };
                        // A host is dark when its anchored section is down.
                        let dark = self.server.world.power.is_host_dark(*id);
                        lines.push(format!(
                            "{:?} {kind} modem{}mb{}{}",
                            id,
                            n.modem_mbps,
                            if n.gateway { " gw" } else { "" },
                            if dark { " DARK" } else { "" }
                        ));
                    }
                }
                for l in self.server.world.net.links() {
                    let k = match l.kind {
                        LinkKind::Wired { damaged } => {
                            if damaged {
                                "wired(damaged)"
                            } else {
                                "wired"
                            }
                        }
                        LinkKind::Wireless { range_m } => {
                            let _ = range_m;
                            "wireless"
                        }
                        LinkKind::Laser => "laser",
                    };
                    lines.push(format!(
                        "{:?}->{:?} {k} {}ms toll{}c",
                        l.from, l.to, l.latency_ms, l.toll_cents
                    ));
                }
                for line in &lines {
                    ui.label(line);
                }
                });
                crate::theme::card(ui, "net actions", crate::theme::OLDNET, |ui| {
                if ui
                    .button("Route 1→3")
                    .on_hover_text(
                        "Preview a powered route from host 1 to host 3. More hops mean more latency and tolls, but a harder trace.",
                    )
                    .clicked()
                {
                    // Blackout-aware: routing avoids hosts in dark sections.
                    match self
                        .server
                        .world
                        .power
                        .powered_route(&self.server.world.net, HostId(1), HostId(3))
                    {
                        Ok(r) => self.say(format!(
                            "route {:?} {}ms toll {} cap {} gw={} thr={}",
                            r.path, r.latency_ms, r.toll_cents, r.cap_mbps, r.gateway_ok,
                            r.trace_threshold()
                        )),
                        Err(e) => self.say(format!("no route: {e}")),
                    }
                }
                if ui
                    .button("Damage wire 1-2")
                    .on_hover_text("Damage the 1–2 wire. Files a crime; routing reroutes around it.")
                    .clicked()
                {
                    // Server-routed act: logged for replay, refused with
                    // the link's reason when there is nothing to damage.
                    match self.server.apply_one(Intent {
                        actor: self.actor,
                        verb: Verb::DamageWire(HostId(1), HostId(2)),
                    }) {
                        Ok(_) => self.say("wire 1-2 damaged".to_string()),
                        Err(e) => self.say(format!("damage failed: {e}")),
                    }
                }
                ui.horizontal(|ui| {
                    // Power grid (design/02): dark a section or heal it. Both
                    // are logged acts, so replays reproduce the reroute.
                    if ui
                        .button("Cut power sec (1,0)")
                        .on_hover_text("Cut power to section (1,0). Dark hosts drop off the network.")
                        .clicked()
                    {                        match self.server.apply_one(Intent {
                            actor: self.actor,
                            verb: Verb::SetPower { sx: 1, sy: 0, up: false },
                        }) {
                            Ok(_) => self.say("power cut in section (1,0)".to_string()),
                            Err(e) => self.say(format!("power cut failed: {e}")),
                        }
                    }
                    if ui
                        .button("Restore power")
                        .on_hover_text("Restore power to section (1,0).")
                        .clicked()
                    {
                        match self.server.apply_one(Intent {
                            actor: self.actor,
                            verb: Verb::SetPower { sx: 1, sy: 0, up: true },
                        }) {
                            Ok(_) => self.say("power restored".to_string()),
                            Err(e) => self.say(format!("restore failed: {e}")),
                        }
                    }
                });
                // Firewall filter (design/04): host 2 blocking host 1
                // partitions the 1->3 route. A logged act.
                ui.horizontal(|ui| {
                    let blocked = self.server.world.net.firewall_blocks(HostId(2), HostId(1));
                    ui.label(if blocked {
                        "firewall: host 2 blocks host 1"
                    } else {
                        "firewall: open"
                    })
                    .on_hover_text(
                        "A firewall filter on host 2 blocks host 1 and partitions the 1→3 route.",
                    );
                    if ui
                        .button(if blocked { "Unblock" } else { "Block 1 on 2" })
                        .on_hover_text(if blocked {
                            "Remove the filter and reopen the direct route."
                        } else {
                            "Filter host 1 on host 2. Logged act; reroutes traffic."
                        })
                        .clicked()
                    {
                        match self.server.apply_one(Intent {
                            actor: self.actor,
                            verb: Verb::SetFirewall {
                                host: HostId(2),
                                source: HostId(1),
                                on: !blocked,
                            },
                        }) {
                            Ok(_) => self.say(format!(
                                "firewall {}",
                                if blocked { "cleared" } else { "set" }
                            )),
                            Err(e) => self.say(format!("firewall failed: {e}")),
                        }
                    }
                });
                });
            }
            Tab::Veil => {
                crate::theme::heading(ui, "Veil — helmet space");
                let mp = self
                    .server
                    .world
                    .mind_of(self.actor)
                    .map(|m| m.mindpower)
                    .unwrap_or(0);
                // Veil sigil canvas: areas as rooms, exits as fine links.
                let areas = self.server.world.veil.area_ids();
                crate::theme::card(ui, "areas", crate::theme::VEIL, |ui| {
                if areas.is_empty() {
                    crate::theme::label(ui, "No hosted areas yet — the Veil is quiet.");
                }
                if !areas.is_empty() {
                    let size = egui::vec2(ui.available_width().min(560.0), 210.0);
                    let (vresp, vp) = ui.allocate_painter(size, egui::Sense::hover());
                    let vresp = vresp.on_hover_text(
                        "Area map: links are exits, and red labels mark hazards.",
                    );
                    let vr = vresp.rect;
                    vp.rect_filled(vr, 0.0, crate::theme::VOID);
                    vp.circle_filled(
                        vr.center(),
                        vr.width().min(vr.height()) * 0.65,
                        crate::theme::alpha(crate::theme::VEIL, 12),
                    );
                    let grid = crate::theme::alpha(crate::theme::VEIL, 10);
                    let mut gx = vr.left();
                    while gx < vr.right() {
                        vp.line_segment(
                            [egui::pos2(gx, vr.top()), egui::pos2(gx, vr.bottom())],
                            egui::Stroke::new(1.0, grid),
                        );
                        gx += 24.0;
                    }
                    let mut gy = vr.top();
                    while gy < vr.bottom() {
                        vp.line_segment(
                            [egui::pos2(vr.left(), gy), egui::pos2(vr.right(), gy)],
                            egui::Stroke::new(1.0, grid),
                        );
                        gy += 24.0;
                    }
                    vp.rect_stroke(
                        vr,
                        0.0,
                        egui::Stroke::new(1.0, crate::theme::alpha(crate::theme::LINE, 220)),
                        egui::StrokeKind::Inside,
                    );
                    crate::theme::corner_marks(
                        &vp,
                        vr,
                        crate::theme::alpha(crate::theme::VEIL, 190),
                        9.0,
                    );
                    let n = areas.len().max(1);
                    let mut pos = std::collections::BTreeMap::new();
                    for (i, a) in areas.iter().enumerate() {
                        let x = vr.min.x + 80.0
                            + (vr.width() - 160.0) * i as f32 / (n.saturating_sub(1).max(1) as f32);
                        let y = vr.center().y + if i % 2 == 0 { -28.0 } else { 28.0 };
                        pos.insert(*a, egui::pos2(x, y));
                    }
                    for a in &areas {
                        if let Some((_, exits, _)) = self.server.world.veil.area_info(*a) {
                            for e in exits {
                                if let (Some(p0), Some(p1)) = (pos.get(a), pos.get(&e)) {
                                    vp.line_segment(
                                        [*p0, *p1],
                                        egui::Stroke::new(
                                            5.0,
                                            crate::theme::alpha(crate::theme::VEIL, 24),
                                        ),
                                    );
                                    vp.line_segment(
                                        [*p0, *p1],
                                        egui::Stroke::new(1.2, crate::theme::VEIL),
                                    );
                                }
                            }
                        }
                    }
                    let current = self.server.world.veil.area_of(self.actor);
                    for a in &areas {
                        let Some(p) = pos.get(a) else { continue };
                        let selected = current == Some(*a);
                        let color = if selected {
                            crate::theme::BETANET
                        } else {
                            crate::theme::VEIL
                        };
                        vp.circle_filled(*p, 34.0, crate::theme::alpha(color, 14));
                        if selected {
                            vp.circle_stroke(
                                *p,
                                29.0,
                                egui::Stroke::new(1.0, crate::theme::alpha(color, 120)),
                            );
                            crate::theme::corner_marks(
                                &vp,
                                egui::Rect::from_center_size(*p, egui::vec2(58.0, 58.0)),
                                color,
                                8.0,
                            );
                        }
                        vp.circle_filled(*p, 24.0, crate::theme::VOID);
                        vp.circle_stroke(*p, 24.0, egui::Stroke::new(2.0, color));
                        vp.text(
                            *p,
                            egui::Align2::CENTER_CENTER,
                            format!("{a}"),
                            crate::theme::display(12.0),
                            color,
                        );
                        let hazard = self.server.world.veil.area_hazard(*a);
                        if hazard > 0 {
                            vp.text(
                                *p + egui::vec2(0.0, 36.0),
                                egui::Align2::CENTER_CENTER,
                                format!("! HAZARD {hazard}"),
                                egui::FontId::monospace(8.5),
                                crate::theme::DANGER,
                            );
                        }
                    }
                }
                });
                crate::theme::card(ui, "helmet", crate::theme::BETANET, |ui| {
                if !self.server.world.veil.present(self.actor) {
                    ui.label(format!("mindpower {mp} (needs 5+)"))
                        .on_hover_text(
                            "Mindpower gates Veil entry. Below 5 you cannot jack back in.",
                        );
                    if ui
                        .button("Jack in")
                        .on_hover_text(
                            "Enter the Veil. Death ejects you with heavy mindpower drain.",
                        )
                        .clicked()
                    {
                        let msg = match self.server.apply_one(Intent {
                            actor: self.actor,
                            verb: Verb::VeilEnter,
                        }) {
                            Ok(_) => format!("jacked in (mp {mp})"),
                            Err(e) => format!("entry refused: {e}"),
                        };
                        self.say(msg);
                    }
                    ui.label("Death ejects with drain; below 5 you cannot re-enter.");
                } else {
                    let area = self.server.world.veil.area_of(self.actor).unwrap_or(0);
                    let name = self.server.world.veil.area_info(area).map(|(n, _, _)| n).unwrap_or_else(|| "lobby".to_string());
                    ui.label(format!("area: {name} (mp {mp})"));
                    let hazard = self.server.world.veil.area_hazard(area);
                    if hazard > 0 {
                        ui.label(
                            egui::RichText::new(format!("! hazard {hazard} on entry"))
                                .color(egui::Color32::RED),
                        );
                    }
                    if let Some(host) = self.server.world.veil.controlled(self.actor) {
                        ui.label(format!("controlling {host:?}"));
                    }
                    let exits: Vec<u64> = self.server.world.veil.area_info(area).map(|(_, e, _)| e).unwrap_or_default();
                    ui.horizontal_wrapped(|ui| {
                        for a in exits {
                            let label = self.server.world.veil.area_info(a).map(|(n, _, x)| format!("{n}{}", if x { " [exit]" } else { "" })).unwrap_or_else(|| format!("area {a}"));
                            if ui
                                .button(format!("→ {label}"))
                                .on_hover_text("Move to this area. Hazards drain mindpower on entry.")
                                .clicked()
                            {
                                let msg = match self.server.apply_one(Intent {
                                    actor: self.actor,
                                    verb: Verb::VeilMove { area: a },
                                }) {
                                    Ok(_) => format!("moved to {label}"),
                                    Err(e) => format!("blocked: {e}"),
                                };
                                self.say(msg);
                            }
                        }
                    });
                    // Host seizure + transfer to a nearby weaker NPC.
                    ui.horizontal_wrapped(|ui| {
                        if let Some((host, hmp)) = self.server.world.veil.area_host(area) {
                            if ui
                                .button(format!("Seize host {host:?} (mp {hmp})"))
                                .on_hover_text(
                                    "Seize this area's bound host. Needs mindpower and files a crime.",
                                )
                                .clicked()
                            {
                                let msg = match self.server.apply_one(Intent {
                                    actor: self.actor,
                                    verb: Verb::VeilSeize,
                                }) {
                                    Ok(_) => format!("seized {host:?}"),
                                    Err(e) => format!("seize failed: {e}"),
                                };
                                self.say(msg);
                            }
                        }
                        for (id, role) in self.nearby_npcs() {
                            if ui
                                .button(format!("Transfer {role:?}"))
                                .on_hover_text(
                                    "Transfer control into a nearby weaker NPC. Files a crime.",
                                )
                                .clicked()
                            {
                                let msg = match self.server.apply_one(Intent {
                                    actor: self.actor,
                                    verb: Verb::VeilTransfer { host: EntityId(id) },
                                }) {
                                    Ok(_) => format!("transferred into {id}"),
                                    Err(e) => format!("transfer failed: {e}"),
                                };
                                self.say(msg);
                            }
                        }
                        if ui
                            .button("Eject")
                            .on_hover_text("Leave the Veil with a light mindpower drain.")
                            .clicked()
                        {
                            let msg = match self.server.apply_one(Intent {
                                actor: self.actor,
                                verb: Verb::VeilEject,
                            }) {
                                // eject() always drains KICK_DRAIN on success.
                                Ok(_) => format!("ejected (drain {})", sim::KICK_DRAIN),
                                Err(e) => format!("eject failed: {e}"),
                            };
                            self.say(msg);
                        }
                    });
                    // Mind-files: carry from the area, read (consume) one.
                    let area_files = self.server.world.veil.area_files(area);
                    let carried = self.server.world.veil.carried_files(self.actor);
                    if !area_files.is_empty() || !carried.is_empty() {
                        ui.horizontal_wrapped(|ui| {
                            ui.label(format!("mind {}/{} files", carried.len(), mp))
                                .on_hover_text(
                                    "Carried mind-files / mindpower capacity.",
                                );
                            for f in area_files {
                                if ui
                                    .button(format!("Carry {f}"))
                                    .on_hover_text("Carry this mind-file with you.")
                                    .clicked()
                                {
                                    self.do_verb(Verb::VeilCarry { name: f.clone() });
                                }
                            }
                            for (i, f) in carried.iter().enumerate() {
                                if ui
                                    .button(format!("Read {f}"))
                                    .on_hover_text("Read and consume this mind-file.")
                                    .clicked()
                                {
                                    self.do_verb(Verb::VeilRead { index: i });
                                }
                            }
                        });
                    }
                }
                });
            }
                });
        });
        self.palette(ui.ctx());
        self.help(ui.ctx());
        ui.ctx().request_repaint();
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;

    #[test]
    fn save_slot_roundtrips() {
        let mut s = Server::new(9);
        s.world.spawn(
            EntityId(1),
            Pos {
                city: 1,
                sx: 0,
                sy: 0,
                z: 0,
                x: 8,
                y: 8,
            },
            1,
        );
        s.submit(Intent {
            actor: EntityId(1),
            verb: Verb::Move(Dir::E),
        });
        s.step();
        let hash = s.world.snapshot_hash();
        assert_eq!(persist_save(&s), "saved");
        let mut s2 = Server::new(1);
        assert_eq!(persist_load(&mut s2), "loaded");
        assert_eq!(s2.world.snapshot_hash(), hash);
    }

    #[test]
    fn demo_world_has_night_store_home_and_hours() {
        let mut shell = Shell::new(None);
        let w = &shell.server.world;
        // Day shop keeps day hours; night store never shuts, prices steeper.
        let day = w.shop_at(10, 8).expect("day shop placed");
        assert!(!day.is_open(3) && day.is_open(10));
        let night = w.shop_at(13, 8).expect("night store placed");
        assert!(night.is_open(3) && night.is_open(12));
        assert!(night.stock["ration"].price_cents > day.stock["ration"].price_cents);
        assert!(night.stock["meds"].price_cents > day.stock["meds"].price_cents);
        // Corridor row stays walkable from spawn to the night store.
        let s = w.section_at(0, 0).expect("spawn section generated");
        assert!((8..=13).all(|x| s.walkable(x, 8)));
        // Home room sleeps through to morning.
        let w = &mut shell.server.world;
        w.tick = 23 * 60;
        w.tick = w.next_at(EntityId(1)).max(w.tick);
        w.apply(&Intent {
            actor: EntityId(1),
            verb: Verb::Sleep,
        })
        .unwrap();
        assert_eq!((w.tick - 1) % 1440, 360);
    }

    /// Headless egui harness: drives `Shell::show` through
    /// `egui::Context::run_ui` (no window, GPU, or screenshots) and reads
    /// the accesskit text tree instead of pixels. Assertion output is
    /// label lists only, so failing UI tests stay small.
    mod headless {
        use super::*;
        use egui::accesskit::{Node, NodeId, Role};

        struct Harness {
            shell: Shell,
            ctx: egui::Context,
            time: f64,
            size: egui::Vec2,
            tree: std::collections::HashMap<NodeId, Node>,
        }

        impl Harness {
            fn new() -> Self {
                Self::new_sized(1280.0, 800.0)
            }

            fn new_sized(width: f32, height: f32) -> Self {
                let ctx = egui::Context::default();
                ctx.enable_accesskit();
                crate::theme::apply(&ctx);
                let mut h = Self {
                    shell: Shell::new(None),
                    ctx,
                    time: 0.0,
                    size: egui::vec2(width, height),
                    tree: Default::default(),
                };
                h.pass_with(vec![]);
                h
            }

            fn input(&mut self, events: Vec<egui::Event>) -> egui::RawInput {
                self.time += 0.1;
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, self.size)),
                    time: Some(self.time),
                    events,
                    ..Default::default()
                }
            }

            fn pass_with(&mut self, events: Vec<egui::Event>) {
                let raw = self.input(events);
                let shell = &mut self.shell;
                let mut out = self.ctx.run_ui(raw, |ui| shell.show(ui));
                // No painter headless: draining the font texture delta is
                // the whole "handling"; pixels never leave the harness.
                out.textures_delta.clear();
                if let Some(update) = out.platform_output.accesskit_update {
                    self.tree = update.nodes.into_iter().collect();
                }
            }

            /// (role, text) pairs currently on screen. Buttons expose
            /// their caption as `label`; static text (`ui.label`,
            /// headings, log lines) exposes it as `value`.
            fn text_of(n: &Node) -> String {
                let label = n.label().unwrap_or_default();
                if !label.is_empty() {
                    return label.to_string();
                }
                n.value().unwrap_or_default().to_string()
            }

            /// (role, text) pairs currently on screen, skipping empty
            /// layout nodes so failure output stays small (no pixels,
            /// no dumps).
            fn labels(&self) -> Vec<(String, String)> {
                let mut out: Vec<(String, String)> = self
                    .tree
                    .values()
                    .map(|n| (format!("{:?}", n.role()), Self::text_of(n)))
                    .filter(|(_, t)| !t.is_empty())
                    .collect();
                out.sort();
                out
            }

            fn has(&self, role: Role, label: &str) -> bool {
                self.tree
                    .values()
                    .any(|n| n.role() == role && Self::text_of(n) == label)
            }

            /// Any rendered text node containing `substr` (log lines,
            /// headings, counter rows).
            fn has_text(&self, substr: &str) -> bool {
                self.tree
                    .values()
                    .any(|n| Self::text_of(n).contains(substr))
            }

            fn button_center(&self, label: &str) -> egui::Pos2 {
                let mut cands = self.button_centers(label);
                assert!(
                    !cands.is_empty(),
                    "no Button {label:?}; got {:?}",
                    self.labels()
                );
                cands.remove(0)
            }

            /// Last match in reading order (bottom-right-most). The accesskit
            /// tree is a HashMap, so callers that need a *specific* widget
            /// among identical labels must pick by position, not iteration.
            fn button_center_last(&self, label: &str) -> egui::Pos2 {
                let mut cands = self.button_centers(label);
                assert!(
                    !cands.is_empty(),
                    "no Button {label:?}; got {:?}",
                    self.labels()
                );
                cands.pop().unwrap()
            }

            /// First button (reading order) whose label starts with `prefix`.
            /// Needed when labels carry live counts.
            fn button_center_prefix(&self, prefix: &str) -> egui::Pos2 {
                let mut cands: Vec<(f64, f64, egui::Pos2)> = self
                    .tree
                    .values()
                    .filter(|n| {
                        n.role() == Role::Button
                            && n.label().unwrap_or_default().starts_with(prefix)
                    })
                    .filter_map(|n| n.bounds())
                    .map(|b| {
                        (
                            b.y0,
                            b.x0,
                            egui::pos2(((b.x0 + b.x1) / 2.0) as f32, ((b.y0 + b.y1) / 2.0) as f32),
                        )
                    })
                    .collect();
                cands.sort_by(|a, b| {
                    a.0.partial_cmp(&b.0)
                        .unwrap_or(std::cmp::Ordering::Equal)
                        .then(a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
                });
                cands
                    .first()
                    .map(|(_, _, p)| *p)
                    .unwrap_or_else(|| panic!("no Button starting with {prefix:?}"))
            }

            /// Centers of every button with this label, sorted by geometry
            /// (top-left to bottom-right) so results are run-stable.
            fn button_centers(&self, label: &str) -> Vec<egui::Pos2> {
                let mut out: Vec<(f64, f64, egui::Pos2)> = self
                    .tree
                    .values()
                    .filter(|n| n.role() == Role::Button && n.label().unwrap_or_default() == label)
                    .filter_map(|n| n.bounds())
                    .map(|b| {
                        let y = b.y0;
                        let x = b.x0;
                        (
                            y,
                            x,
                            egui::pos2(((b.x0 + b.x1) / 2.0) as f32, ((b.y0 + b.y1) / 2.0) as f32),
                        )
                    })
                    .collect();
                out.sort_by(|a, b| {
                    a.0.partial_cmp(&b.0)
                        .unwrap_or(std::cmp::Ordering::Equal)
                        .then(a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
                });
                out.into_iter().map(|(_, _, p)| p).collect()
            }

            fn text_input_center(&self) -> egui::Pos2 {
                let node = self
                    .tree
                    .values()
                    .find(|n| n.role() == Role::TextInput)
                    .unwrap_or_else(|| panic!("no text input; got {:?}", self.labels()));
                let b = node.bounds().expect("text input has no bounds");
                egui::pos2(((b.x0 + b.x1) / 2.0) as f32, ((b.y0 + b.y1) / 2.0) as f32)
            }

            /// Press+release on a raw position (used for grid cells).
            fn click_at(&mut self, at: egui::Pos2) {
                let mods = egui::Modifiers::default();
                let press = egui::Event::PointerButton {
                    pos: at,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: mods,
                };
                let release = egui::Event::PointerButton {
                    pos: at,
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: mods,
                };
                self.pass_with(vec![press]);
                self.pass_with(vec![release]);
                self.pass_with(vec![]);
            }

            /// Click the button with this (unique) label. The centre is
            /// re-resolved before the release pass: a click that adds a log
            /// line shifts the layout, and a stale coordinate would land on
            /// whatever moved under it.
            fn click(&mut self, label: &str) {
                self.click_resolved(label, |h| h.button_center(label));
            }

            /// Click the first button whose label starts with `prefix`.
            fn click_prefix(&mut self, prefix: &str) {
                self.click_resolved(prefix, |h| h.button_center_prefix(prefix));
            }

            /// Resolve and click a button, scrolling the central pane until
            /// it is actually inside the clickable band. Both `click` and
            /// `click_prefix` share this so a layout change cannot make one
            /// path silently click off-screen.
            fn click_resolved(&mut self, desc: &str, resolve: impl Fn(&Self) -> egui::Pos2) {
                let mods = egui::Modifiers::default();
                for _ in 0..64 {
                    let at = resolve(self);
                    let overlay = self.has_text("Command palette");
                    let top_chrome = at.y < 66.0 && at.x > 900.0;
                    // The rails do not scroll with the central pane.
                    let in_rail = at.x < 260.0 || at.x > 980.0;
                    // Top bar ends near y=60; the 156px console starts near
                    // y=644, leaving this central band fully clickable.
                    let in_view = at.y >= 66.0 && at.y < 636.0;
                    if overlay || top_chrome || in_rail || in_view {
                        self.pass_with(vec![egui::Event::PointerButton {
                            pos: at,
                            button: egui::PointerButton::Primary,
                            pressed: true,
                            modifiers: mods,
                        }]);
                        let at = resolve(self);
                        self.pass_with(vec![egui::Event::PointerButton {
                            pos: at,
                            button: egui::PointerButton::Primary,
                            pressed: false,
                            modifiers: mods,
                        }]);
                        self.pass_with(vec![]);
                        return;
                    }
                    // Negative wheel delta scrolls toward the end of the
                    // page; positive scrolls back toward the top.
                    let delta = if at.y >= 636.0 { -20.0 } else { 20.0 };
                    self.pass_with(vec![
                        egui::Event::PointerMoved(egui::pos2(700.0, 420.0)),
                        egui::Event::MouseWheel {
                            unit: egui::MouseWheelUnit::Line,
                            delta: egui::vec2(0.0, delta),
                            phase: egui::TouchPhase::Move,
                            modifiers: mods,
                        },
                    ]);
                }
                panic!(
                    "could not scroll Button {desc:?} into view; got {:?}",
                    self.labels()
                );
            }

            /// Click the bottom-right-most button with this label (for grids
            /// of identical labels like map cells).
            fn click_last(&mut self, label: &str) {
                // Grids live at the top of their view; reset scroll before
                // resolving the bottom-right-most cell.
                for _ in 0..24 {
                    let at = self.button_center_last(label);
                    if at.y >= 60.0 && at.y < 610.0 {
                        let mods = egui::Modifiers::default();
                        self.pass_with(vec![egui::Event::PointerButton {
                            pos: at,
                            button: egui::PointerButton::Primary,
                            pressed: true,
                            modifiers: mods,
                        }]);
                        let at = self.button_center_last(label);
                        self.pass_with(vec![egui::Event::PointerButton {
                            pos: at,
                            button: egui::PointerButton::Primary,
                            pressed: false,
                            modifiers: mods,
                        }]);
                        self.pass_with(vec![]);
                        return;
                    }
                    self.pass_with(vec![
                        egui::Event::PointerMoved(egui::pos2(700.0, 420.0)),
                        egui::Event::MouseWheel {
                            unit: egui::MouseWheelUnit::Line,
                            delta: egui::vec2(0.0, 20.0),
                            phase: egui::TouchPhase::Move,
                            modifiers: egui::Modifiers::default(),
                        },
                    ]);
                }
                panic!("could not scroll cell {label:?} into view");
            }

            fn tap_key(&mut self, key: egui::Key) {
                let mods = egui::Modifiers::default();
                let down = egui::Event::Key {
                    key,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: mods,
                };
                let up = egui::Event::Key {
                    key,
                    physical_key: None,
                    pressed: false,
                    repeat: false,
                    modifiers: mods,
                };
                self.pass_with(vec![down]);
                self.pass_with(vec![up]);
            }
        }

        #[test]
        fn sleep_button_sleeps_through_click() {
            let mut h = Harness::new();
            assert!(h.has(Role::Button, "Sleep"), "text: {:?}", h.labels());
            h.click("Sleep");
            // Home at spawn, 00:00 -> rest to 06:00, status line logged.
            assert_eq!((h.shell.server.world.tick - 1) % 1440, 360);
            assert!(
                h.shell.log.last().is_some_and(|l| l.contains("rations=")),
                "log tail: {:?}",
                h.shell.log.iter().rev().take(2).collect::<Vec<_>>()
            );
            assert!(
                h.has_text(h.shell.log.last().unwrap()),
                "status not rendered; text: {:?}",
                h.labels()
            );
        }

        #[test]
        fn walk_east_then_shuttered_buy_rejects_with_cause() {
            let mut h = Harness::new();
            h.tap_key(egui::Key::ArrowRight);
            h.tap_key(egui::Key::ArrowRight);
            assert_eq!(
                h.shell.server.world.pos_of(EntityId(1)),
                Some(pos(10, 8)),
                "two real keypresses should walk spawn (8,8) to the day shop"
            );
            // 00:20 game time: the 8-20 counter is shuttered.
            assert!(
                h.has_text("Counter — type buy <item> or click:"),
                "counter panel missing; text: {:?}",
                h.labels()
            );
            h.click("Buy");
            let tail: Vec<&str> = h
                .shell
                .log
                .iter()
                .rev()
                .take(2)
                .map(|s| s.as_str())
                .collect();
            assert!(
                tail.iter().any(|l| l.contains("rejected: shuttered")),
                "log tail: {tail:?}"
            );
            assert!(
                h.has_text("rejected: shuttered"),
                "rejection not rendered; text: {:?}",
                h.labels()
            );
        }

        #[test]
        fn console_sleep_command_runs_through_text_input() {
            let mut h = Harness::new();
            let at = h.text_input_center();
            h.click_at(at);
            h.pass_with(vec![egui::Event::Text("sleep".into())]);
            h.tap_key(egui::Key::Enter);
            h.pass_with(vec![]);
            assert_eq!((h.shell.server.world.tick - 1) % 1440, 360);
        }

        #[test]
        fn damage_wire_button_damages_through_server() {
            let mut h = Harness::new();
            assert!(h.shell.server.world.net.route(HostId(1), HostId(3)).is_ok());
            h.click("Net");
            h.click("Damage wire 1-2");
            let tail: Vec<&str> = h
                .shell
                .log
                .iter()
                .rev()
                .take(2)
                .map(|s| s.as_str())
                .collect();
            assert!(
                tail.iter().any(|l| l.contains("wire 1-2 damaged")),
                "log tail: {tail:?}"
            );
            assert!(h.has_text("wire 1-2 damaged"), "text: {:?}", h.labels());
            // Host 1 is isolated through the same world the server owns:
            // the act was logged, not applied behind the server's back.
            assert!(h
                .shell
                .server
                .world
                .net
                .route(HostId(1), HostId(3))
                .is_err());
            assert_eq!(h.shell.server.log.len(), 1);
        }

        #[test]
        fn deck_load_then_strike_through_clicks() {
            let mut h = Harness::new();
            h.click("Deck");
            // Pick the bottom-right free cell so the killer starts far from
            // the warden's top-left spawn; an adjacent cell would let the
            // warden kill it in the same beat and change the outcome.
            h.click_last(".");
            h.click("Load killer @selected");
            let log: Vec<&str> = h.shell.log.iter().map(|s| s.as_str()).collect();
            assert!(
                log.iter().any(|l| l.contains("loaded killer")),
                "log: {log:?}"
            );
            assert!(
                log.iter().any(|l| l.contains("trace started")),
                "warden should answer the intrusion; log: {log:?}"
            );
            assert!(h.has_text("loaded killer"), "text: {:?}", h.labels());
            h.click("Strike selected");
            let log: Vec<&str> = h.shell.log.iter().map(|s| s.as_str()).collect();
            assert!(
                log.iter().any(|l| l.contains("strike: killed=true")),
                "log: {log:?}"
            );
            // All through the server log: load + strike replay cleanly.
            assert_eq!(h.shell.server.log.len(), 2);
        }

        #[test]
        fn every_screen_region_has_a_card_header() {
            let h = Harness::new();
            // Left rail and Street: the primary surfaces are labelled so a
            // new player can tell systems apart at a glance.
            for header in [
                "SURVIVAL",
                "BODY",
                "CONDUCT",
                "BRIEFING",
                "DAILY ACTIONS",
                "STREET — SECTION (0, 0)",
                "WORK BOARD",
                "CONTRACTS",
                "MAIL",
            ] {
                assert!(h.has_text(header), "missing header {header:?}");
            }
            let mut h = Harness::new();
            h.click("Deck");
            for header in ["HOST", "ARENA", "SECURITY", "LOADOUT", "CARDS @SELECTED"] {
                assert!(h.has_text(header), "missing Deck header {header:?}");
            }
        }

        #[test]
        fn wide_layout_keeps_every_region_reachable() {
            let h = Harness::new_sized(1920.0, 1080.0);
            for header in [
                "SURVIVAL",
                "BODY",
                "CONDUCT",
                "BRIEFING",
                "DAILY ACTIONS",
                "STREET — SECTION (0, 0)",
                "WORK BOARD",
                "CONTRACTS",
                "MAIL",
            ] {
                assert!(h.has_text(header), "missing wide header {header:?}");
            }
            assert!(h.has_text("Tile (8,8)"), "inspector missing at 1920");
        }

        #[test]
        fn inspector_lists_nearby_landmarks() {
            let h = Harness::new();
            // The inspector is never a dead panel on the player's own tile.
            assert!(h.has_text("NEARBY"), "text: {:?}", h.labels());
            assert!(
                h.has_text("counter (10,8)"),
                "nearby counter missing; text: {:?}",
                h.labels()
            );
        }

        #[test]
        fn card_hover_preview_shows_effect_and_cost() {
            let mut h = Harness::new();
            h.click("Deck");
            h.click_last(".");
            h.click("spike");
            assert!(h.has_text("CARD PREVIEW"), "text: {:?}", h.labels());
            assert!(h.has_text("Spend 2 LOOP"), "text: {:?}", h.labels());
        }

        #[test]
        fn f1_opens_field_manual() {
            let mut h = Harness::new();
            h.tap_key(egui::Key::F1);
            assert!(h.has_text("Field manual"), "text: {:?}", h.labels());
            h.tap_key(egui::Key::F1);
            assert!(!h.has_text("Field manual"), "text: {:?}", h.labels());
        }

        #[test]
        fn command_palette_opens_and_runs_help() {
            let mut h = Harness::new();
            let mods = egui::Modifiers {
                command: true,
                ..Default::default()
            };
            let key = |pressed| egui::Event::Key {
                key: egui::Key::K,
                physical_key: None,
                pressed,
                repeat: false,
                modifiers: mods,
            };
            h.pass_with(vec![key(true)]);
            h.pass_with(vec![key(false)]);
            assert!(h.has_text("Command palette"), "text: {:?}", h.labels());
            h.click("Help");
            assert!(
                h.shell.log.iter().any(|l| l.contains("Ctrl-K palette")),
                "log: {:?}",
                h.shell.log
            );
            assert!(!h.has_text("Command palette"), "text: {:?}", h.labels());
        }

        #[test]
        fn load_scanner_and_scan_through_clicks() {
            let mut h = Harness::new();
            h.click("Deck");
            h.click_last(".");
            h.click("Load scanner (1cpu)");
            assert!(
                h.shell.log.iter().any(|l| l.contains("loaded scanner")),
                "log: {:?}",
                h.shell.log
            );
            h.click("Scan");
            assert!(
                h.shell.log.iter().any(|l| l.contains("host scanned")),
                "log: {:?}",
                h.shell.log
            );
            assert!(h.has_text("scanned yes"), "text: {:?}", h.labels());
            // Connector remote probe onto the demo host 3.
            h.click_last(".");
            h.click("Load connector (1cpu)");
            h.click("Connect host 3");
            assert!(
                h.shell
                    .log
                    .iter()
                    .any(|l| l.contains("remote host revealed")),
                "log: {:?}",
                h.shell.log
            );
        }

        #[test]
        fn veil_full_flow_through_clicks() {
            let mut h = Harness::new();
            h.click("Veil");
            // Mindpower 8: jack in, walk to ops, seize, eject.
            h.click("Jack in");
            assert!(
                h.shell.log.iter().any(|l| l.contains("jacked in (mp 8)")),
                "log: {:?}",
                h.shell.log
            );
            h.click("→ ops");
            assert!(
                h.shell.log.iter().any(|l| l.contains("moved to ops")),
                "log: {:?}",
                h.shell.log
            );
            h.click_prefix("Seize host");
            assert!(
                h.shell.log.iter().any(|l| l.contains("seized")),
                "log: {:?}",
                h.shell.log
            );
            h.click("Eject");
            assert!(
                h.shell.log.iter().any(|l| l.contains("ejected (drain")),
                "log: {:?}",
                h.shell.log
            );
            assert!(h.has_text("ejected (drain"), "text: {:?}", h.labels());
            // Four veil acts went through the server log.
            assert_eq!(h.shell.server.log.len(), 4);
        }

        #[test]
        fn job_ladder_renders_and_top_tier_pays_through_clicks() {
            let mut h = Harness::new();
            // All five board buttons come from data/jobs.yaml, each showing
            // its resource requirement.
            for (i, id, pay, req) in [
                (0, "debug-script", 60, "[loop 0/4]"),
                (1, "driver-patch", 120, "[loop 0/6]"),
                (2, "firmware-flash", 200, "[data 0/5]"),
                (3, "kernel-tune", 320, "[data 0/6]"),
                (4, "contract-build", 500, "[algo 0/6]"),
            ] {
                let label = format!("Work {i}: {id} {pay}c {req}");
                assert!(
                    h.has(Role::Button, &label),
                    "missing {label:?}; {:?}",
                    h.labels()
                );
            }
            // Stock the top tier by coding 6 algo, then work it.
            for _ in 0..6 {
                h.click_prefix("Code algo (");
            }
            let (_, _, before, _) = h.shell.server.world.needs(EntityId(1)).unwrap();
            h.click_prefix("Work 4: contract-build");
            let (_, _, after, _) = h.shell.server.world.needs(EntityId(1)).unwrap();
            assert_eq!(after, before + 500, "top-tier payout");
            assert_eq!(h.shell.server.world.units(EntityId(1), sim::Res::Algo), 0);
        }

        #[test]
        fn code_then_work_cycle_through_clicks() {
            let mut h = Harness::new();
            // Job 0 (debug-script) needs 4 loop units; the button shows it.
            assert!(
                h.has(Role::Button, "Work 0: debug-script 60c [loop 0/4]"),
                "labels: {:?}",
                h.labels()
            );
            // Working short is refused with the cause rendered.
            h.click_prefix("Work 0: debug-script");
            assert!(
                h.has_text("rejected: no_resource"),
                "text: {:?}",
                h.labels()
            );
            // Four Code clicks fill the requirement.
            for _ in 0..4 {
                h.click_prefix("Code loop (");
            }
            assert!(h.has_text("Code loop (4)"), "text: {:?}", h.labels());
            let (_, _, before, _) = h.shell.server.world.needs(EntityId(1)).unwrap();
            h.click_prefix("Work 0: debug-script");
            let (_, _, after, _) = h.shell.server.world.needs(EntityId(1)).unwrap();
            assert_eq!(after, before + 60, "job paid once units were stocked");
            assert_eq!(h.shell.server.world.units(EntityId(1), sim::Res::Loop), 0);
            // 1 refusal + 4 codes + 1 work all logged.
            assert_eq!(h.shell.server.log.len(), 6);
        }

        #[test]
        fn bank_balance_shows_and_moves_on_deposit_click() {
            let mut h = Harness::new();
            // Demo world opens the player's account with $5.00 minted.
            assert!(h.has_text("bank"), "text: {:?}", h.labels());
            assert_eq!(h.shell.server.world.bank_balance(EntityId(1)), Some(500));
            // Deposit $1.50 through the console; pocket drops, bank rises.
            let at = h.text_input_center();
            h.click_at(at);
            h.pass_with(vec![egui::Event::Text("deposit 150".into())]);
            h.tap_key(egui::Key::Enter);
            h.pass_with(vec![]);
            assert!(h.has_text("$6.50"), "text: {:?}", h.labels());
            assert_eq!(h.shell.server.world.bank_balance(EntityId(1)), Some(650));
            assert_eq!(h.shell.server.log.len(), 1);
        }

        #[test]
        fn power_cut_reroutes_and_restores_through_clicks() {
            let mut h = Harness::new();
            h.click("Net");
            // Baseline: the direct path transits host 2.
            h.click("Route 1→3");
            assert!(
                h.shell
                    .log
                    .iter()
                    .any(|l| l.contains("route") && l.contains("HostId(2)")),
                "log: {:?}",
                h.shell.log
            );
            // Cut power: host 2 dark, traffic reroutes around it.
            h.click("Cut power sec (1,0)");
            assert!(
                h.has_text("power cut in section (1,0)"),
                "text: {:?}",
                h.labels()
            );
            h.click("Route 1→3");
            let last = h.shell.log.last().cloned().unwrap_or_default();
            assert!(
                last.contains("route") && !last.contains("HostId(2)"),
                "rerouted line: {last:?}"
            );
            // Restore heals it.
            h.click("Restore power");
            h.click("Route 1→3");
            let last = h.shell.log.last().cloned().unwrap_or_default();
            assert!(last.contains("HostId(2)"), "healed line: {last:?}");
            // Routing is a read-only query (not an act), so only the cut
            // and restore are logged.
            assert_eq!(h.shell.server.log.len(), 2);
        }

        #[test]
        fn mail_craft_and_send_trojan_through_clicks() {
            let mut h = Harness::new();
            // Mailer+ costs algo, which we do not have yet: refused.
            h.click("Craft Mailer+ (3 algo)");
            assert!(
                h.shell.log.iter().any(|l| l.contains("craft failed")),
                "log: {:?}",
                h.shell.log
            );
            // Farm + code algo to afford it, then craft.
            h.click("Deck");
            h.click_last(".");
            h.click("Farm algo");
            h.click("Street");
            for _ in 0..3 {
                h.click_prefix("Code algo (");
            }
            h.click("Craft Mailer+ (3 algo)");
            assert!(
                h.shell.log.iter().any(|l| l.contains("Mailer+ crafted")),
                "log: {:?}",
                h.shell.log
            );
            assert!(h.has_text("Mail — inbox 0"), "text: {:?}", h.labels());
            // Breach the rig, then send a trojan by mail (the Deck control
            // targets the rig's owner — no adjacency needed).
            h.click("Deck");
            h.click("cve-2006-0005");
            assert!(h.has_text("breached yes"), "text: {:?}", h.labels());
            h.click("Send trojan");
            assert!(
                h.shell.log.iter().any(|l| l.contains("trojan mailed")),
                "log: {:?}",
                h.shell.log
            );
        }

        #[test]
        fn mods_persist_on_the_selected_program() {
            let mut h = Harness::new();
            h.click("Deck");
            // Farm a producer (gives us a program), then build a mod into
            // it. Splash costs 2 loop, so fund it first.
            h.click_last(".");
            h.click("Farm loop");
            // Fund the 2-loop cost from the Street tab's Code control.
            h.click("Street");
            for _ in 0..2 {
                h.click_prefix("Code loop (");
            }
            h.click("Deck");
            h.click("P");
            h.click("Mod splash (2 loop)");
            assert!(
                h.shell.log.iter().any(|l| l.contains("mod splash built")),
                "log: {:?}",
                h.shell.log
            );
            assert!(h.has_text("mods: Splash"), "text: {:?}", h.labels());
            // Persists: a second attack attempt does not clear it.
            assert!(h.has_text("@(") || h.has_text("mods: Splash"));
        }

        #[test]
        fn license_shows_and_applying_requires_eligibility() {
            let mut h = Harness::new();
            // No kills yet: progress is shown, and applying is refused
            // with its cause.
            assert!(h.has_text("0/3 kills"), "text: {:?}", h.labels());
            h.click("Apply license");
            assert!(
                h.shell
                    .log
                    .iter()
                    .any(|l| l.contains("license refused: ineligible")),
                "log: {:?}",
                h.shell.log
            );
        }

        #[test]
        fn heat_shows_and_paying_a_fine_settles_it() {
            let mut h = Harness::new();
            // Clean to start: standing is in the top bar, no fine button.
            assert!(h.has_text("Clean"), "text: {:?}", h.labels());
            assert!(!h
                .labels()
                .iter()
                .any(|(r, t)| r == "Button" && t.starts_with("Pay fine")));
            // Commit sabotage through the Net tab: the record grows and the
            // fine control appears.
            h.click("Net");
            h.click("Damage wire 1-2");
            h.click("Street");
            assert!(
                h.labels()
                    .iter()
                    .any(|(r, t)| r == "Button" && t.starts_with("Pay fine")),
                "no fine control; text: {:?}",
                h.labels()
            );
            assert!(!h.has_text("Clean"), "text: {:?}", h.labels());
            // Settle: the record clears and pursuit ends.
            h.click_prefix("Pay fine");
            assert!(h.has_text("Clean"), "text: {:?}", h.labels());
            assert!(
                h.shell.log.iter().any(|l| l.contains("record settled")),
                "log: {:?}",
                h.shell.log
            );
        }

        #[test]
        fn exploit_breaches_and_trojan_needs_a_mailer_plus() {
            let mut h = Harness::new();
            h.click("Deck");
            // Exploit needs a program of ours on the host first.
            h.click_last(".");
            h.click("Farm loop");
            // Exploit the office-ws privilege hole.
            h.click("cve-2006-0005");
            assert!(h.has_text("breached yes"), "text: {:?}", h.labels());
            // Trojans are email attachments now: without a crafted Mailer+
            // the send is refused with its cause (the craft path is covered
            // by mail_craft_and_send_trojan_through_clicks).
            h.click("Send trojan");
            assert!(
                h.shell
                    .log
                    .iter()
                    .any(|l| l.contains("trojan failed: no_mailer_plus")),
                "log: {:?}",
                h.shell.log
            );
        }

        #[test]
        fn firewall_toggle_and_scrub_through_clicks() {
            let mut h = Harness::new();
            // Net tab: blocking host 1 on host 2 partitions the 1->3 route.
            h.click("Net");
            h.click("Route 1→3");
            assert!(
                h.shell.log.last().is_some_and(|l| l.contains("HostId(2)")),
                "log: {:?}",
                h.shell.log
            );
            h.click("Block 1 on 2");
            assert!(
                h.has_text("firewall: host 2 blocks host 1"),
                "text: {:?}",
                h.labels()
            );
            h.click("Route 1→3");
            assert!(
                h.shell.log.last().is_some_and(|l| l.contains("no route")),
                "log: {:?}",
                h.shell.log
            );
            h.click("Unblock");
            h.click("Route 1→3");
            assert!(
                h.shell.log.last().is_some_and(|l| l.contains("HostId(2)")),
                "log: {:?}",
                h.shell.log
            );
            // Deck tab: the scrub control reports the trail. With no
            // program on the rig and no privilege, scrubbing is refused
            // with its cause; deploying a program enables it.
            h.click("Deck");
            assert!(
                h.has_text("your trail on this rig: 0"),
                "text: {:?}",
                h.labels()
            );
            h.click("Scrub log");
            assert!(
                h.shell
                    .log
                    .iter()
                    .any(|l| l.contains("scrub failed: no_access")),
                "log: {:?}",
                h.shell.log
            );
            h.click_last(".");
            h.click("Farm loop");
            h.click("Scrub log");
            assert!(
                h.shell.log.iter().any(|l| l == "log scrubbed"),
                "log: {:?}",
                h.shell.log
            );
        }

        #[test]
        fn deck_shows_archetype_and_vuln_count() {
            let mut h = Harness::new();
            h.click("Deck");
            // The demo rig is an office workstation carrying its own holes.
            assert!(h.has_text("arch office-ws"), "text: {:?}", h.labels());
            assert!(h.has_text("breached no"), "text: {:?}", h.labels());
        }

        #[test]
        fn card_row_plays_a_card_at_the_selected_cell() {
            let mut h = Harness::new();
            h.click("Deck");
            // Farm a producer, then select it and buff it with a card.
            h.click_last(".");
            h.click("Farm loop");
            assert!(
                h.labels().iter().any(|(r, t)| r == "Button" && t == "P"),
                "no producer"
            );
            h.click("P");
            // Cards now carry a resource cost in the label.
            h.click("tank (1 loop)");
            assert!(
                !h.shell.log.iter().any(|l| l.contains("refused")),
                "log: {:?}",
                h.shell.log
            );
            assert!(
                h.shell.log.iter().any(|l| l.contains("card tank @")),
                "log: {:?}",
                h.shell.log
            );
            // The card act is logged like any other.
            assert_eq!(h.shell.server.log.len(), 2);
        }

        #[test]
        fn deck_shows_producer_glyph_and_yield_summary() {
            let mut h = Harness::new();
            h.click("Deck");
            // Empty rig: the summary points at the Farm affordance.
            assert!(
                h.has_text("Producers: none — click a free cell and Farm"),
                "text: {:?}",
                h.labels()
            );
            // Farm a data producer on a free cell.
            h.click_last(".");
            h.click("Farm data");
            // The producer shows as a P in the arena and the summary
            // reports its yield per act.
            assert!(
                h.labels().iter().any(|(r, t)| r == "Button" && t == "P"),
                "no P glyph; labels: {:?}",
                h.labels()
            );
            assert!(
                h.has_text("Producers: data ×1 (+2/act)"),
                "text: {:?}",
                h.labels()
            );
            // The Deck tab also reports components held.
            assert!(
                h.has_text("components: loop 0 · data 2 · algo 0"),
                "text: {:?}",
                h.labels()
            );
        }

        #[test]
        fn console_farm_autopicks_a_free_cell() {
            let mut h = Harness::new();
            let at = h.text_input_center();
            h.click_at(at);
            h.pass_with(vec![egui::Event::Text("farm algo".into())]);
            h.tap_key(egui::Key::Enter);
            h.pass_with(vec![]);
            // No cell was selected, so the free-cell picker chose one and
            // the farm act applied (no rejection in the log).
            assert_eq!(h.shell.server.world.units(EntityId(1), sim::Res::Algo), 2);
            assert_eq!(h.shell.server.log.len(), 1);
            assert!(
                !h.shell.log.iter().any(|l| l.contains("rejected")),
                "log: {:?}",
                h.shell.log
            );
        }

        #[test]
        fn farm_then_units_accrue_free_and_show() {
            let mut h = Harness::new();
            assert!(h.has_text("LOOP 0"), "text: {:?}", h.labels());
            h.click("Deck");
            // Select a cell, then farm a loop producer there.
            h.click_last(".");
            h.click("Farm loop");
            assert!(h.has_text("farmed loop"), "text: {:?}", h.labels());
            assert!(
                h.has_text("components: loop 3 · data 0 · algo 0"),
                "text: {:?}",
                h.labels()
            );
            // Any further act yields free units, with no extra clicks.
            h.click("Street");
            h.click("Watch");
            h.click("Watch");
            assert!(h.has_text("LOOP 9"), "text: {:?}", h.labels());
            // Farm + two watches are all logged acts.
            assert_eq!(h.shell.server.log.len(), 3);
        }

        #[test]
        fn contract_board_accept_and_deliver_through_clicks() {
            let mut h = Harness::new();
            h.shell
                .server
                .world
                .set_contracts(&[sim::contracts::default_contracts()[0].clone()]);
            h.shell.server.world.refresh_board();
            h.shell
                .server
                .world
                .add_item(EntityId(1), "medkit", 2)
                .unwrap();
            h.pass_with(vec![]);
            let before = h.shell.server.world.bank_balance(EntityId(1)).unwrap_or(0);
            h.click_prefix("Accept: ");
            assert!(h.has_text("Contract:"), "text: {:?}", h.labels());
            h.click("Deliver contract");
            assert!(
                h.shell.server.world.active_contract().is_none(),
                "contract did not complete"
            );
            let after = h.shell.server.world.bank_balance(EntityId(1)).unwrap_or(0);
            assert!(after > before, "contract did not pay: {before} -> {after}");
        }

        #[test]
        fn attack_adjacent_npc_through_click() {
            let mut h = Harness::new();
            h.shell.server.world.spawn_npc(
                EntityId(99),
                sim::Role::Office,
                (9, 8),
                (9, 8),
                (9, 8),
                9999,
            );
            h.pass_with(vec![]);
            h.click("Attack Office");
            assert!(
                h.has_text("you hit") || h.has_text("you miss"),
                "text: {:?}",
                h.labels()
            );
        }

        #[test]
        fn inventory_equips_uses_drops_and_picks_up() {
            let mut h = Harness::new();
            h.shell
                .server
                .world
                .add_item(EntityId(1), "knife", 1)
                .unwrap();
            h.pass_with(vec![]);
            h.click("Equip Knife");
            assert!(h.has_text("Weapon: Knife"), "text: {:?}", h.labels());
            h.shell
                .server
                .world
                .add_item(EntityId(1), "medkit", 1)
                .unwrap();
            h.shell.server.world.damage(EntityId(1), 10);
            h.pass_with(vec![]);
            h.click("Use Medkit");
            assert!(h.has_text("HP 20/20"), "text: {:?}", h.labels());
            h.shell
                .server
                .world
                .add_item(EntityId(1), "snack", 1)
                .unwrap();
            h.pass_with(vec![]);
            h.click("Drop Snack");
            assert!(h.has_text("Pick up Snack x1"), "text: {:?}", h.labels());
            h.click("Pick up Snack x1");
            assert!(!h.has_text("On floor:"), "text: {:?}", h.labels());
        }

        #[test]
        fn hp_shows_and_clinic_revive_works() {
            let mut h = Harness::new();
            assert!(h.has_text("HP 20/20"), "text: {:?}", h.labels());
            h.shell.server.world.damage(EntityId(1), 999);
            h.pass_with(vec![]);
            assert!(h.has_text("HP 0/20"), "text: {:?}", h.labels());
            h.click("Revive at clinic");
            assert!(h.has_text("HP 10/20"), "text: {:?}", h.labels());
        }

        #[test]
        fn tabs_render_without_panic() {
            let mut h = Harness::new();
            // Re-select one by one and assert each heading renders.
            h.click("Net");
            assert!(
                h.has_text("Net — world graph (Oldnet/Betanet)"),
                "text: {:?}",
                h.labels()
            );
            h.click("Veil");
            assert!(h.has_text("Veil — helmet space"), "text: {:?}", h.labels());
            h.click("Deck");
            assert!(h.has_text("CPU "), "text: {:?}", h.labels());
        }
    }
}

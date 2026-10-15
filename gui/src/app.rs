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
fn item_blurb(item: &str) -> &'static str {
    match item {
        "firewall" => " · slows inbound traces",
        "ration" => " · +30 food",
        "meds" => " · +25 mood, +5 food",
        _ => "",
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
    mp: u8,
    talks: BTreeMap<String, Rc<sim::Dialogue>>,
    talking: Option<sim::Conversation>,
    frames_left: Option<u32>,
    jobs: Vec<sim::JobDef>,
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
        let mut talks = BTreeMap::new();
        for d in sim::parse_dialogues(include_str!("../../data/dialogues.yaml")).unwrap_or_default()
        {
            talks.insert(d.id.clone(), Rc::new(d));
        }
        // Demo veil complex: lobby - ops - roof(exit).
        server.world.veil.add_area(1, "lobby", vec![2], false);
        server.world.veil.add_area(2, "ops", vec![1, 3], false);
        server.world.veil.add_area(3, "roof", vec![2], true);

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
        let mut shop = sim::Shop::new(till);
        shop.add_line("ration", 100, 5, 2, 5, 200);
        shop.add_line("meds", 400, 2, 1, 3, 900);
        shop.add_line("firewall", 1500, 1, 1, 1, 1200);
        shop.set_hours(8, 20);
        server.world.place_shop(10, 8, shop);
        // Night store three doors down: always open, steeper prices for
        // the graveyard shift. Same till owner, separate account.
        let night_till = server.world.open_account(EntityId(10), HostId(1)).unwrap();
        let mut night = sim::Shop::new(night_till);
        night.add_line("ration", 150, 3, 2, 3, 300);
        night.add_line("meds", 600, 2, 1, 2, 1200);
        night.set_hours(0, 24);
        server.world.place_shop(13, 8, night);
        // Rented room above the noodle shop (design/01): sleep here.
        server.world.set_home(pos(8, 8));
        let mut shell = Self {
            server,
            actor,
            tab: Tab::Street,
            input: String::new(),
            log: vec!["Welcome to FIDOM. Arrows/hjkl move. Type help + Enter.".to_string()],
            sel: None,
            mp: 8,
            talks,
            talking: None,
            frames_left: frames,
            jobs,
        };
        shell.say_status();
        shell
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
        if let Err(e) = self.server.apply_one(intent) {
            self.say(format!("rejected: {e}"));
        }
        self.say_status();
    }

    /// Like `do_verb`, but also relays any retaliation beat the act
    /// triggered (warden kills, traces) — the console code/farm paths
    /// would otherwise silently lose a producer that died on its first beat.
    fn act_narrated(&mut self, verb: Verb) {
        match self.server.apply_one(Intent {
            actor: self.actor,
            verb,
        }) {
            Ok((_, events)) => {
                for line in Self::beat_lines(&events) {
                    self.say(line);
                }
            }
            Err(e) => self.say(format!("rejected: {e}")),
        }
        self.say_status();
    }

    fn run_command(&mut self) {
        let cmd = std::mem::take(&mut self.input);
        let cmd = cmd.trim().to_lowercase();
        match cmd.as_str() {
            "help" => self.say(
                "move arrows/hjkl(vi-keys) · eat · work 0-4 · code loop/data/algo · farm loop/data/algo · watch · buy <item> · talk · deposit/withdraw <c> · sleep"
                    .to_string(),
            ),
            "eat" => self.do_verb(Verb::Eat),
            "sleep" => self.do_verb(Verb::Sleep),
            _ if cmd.starts_with("buy ") => self.do_verb(Verb::Buy(cmd[4..].trim().to_string())),
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
            Some((_, role)) => match self.dialogue_for(role) {
                Some(dlg) => {
                    let t = sim::Conversation::new(dlg, None);
                    self.say(t.text().to_string());
                    self.talking = Some(t);
                }
                None => self.say("they have nothing to say".to_string()),
            },
            None => self.say("no one in earshot".to_string()),
        }
    }

    fn move_key(&mut self, ctx: &egui::Context) {
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
        if self.tab == Tab::Street {
            let ctx = ui.ctx().clone();
            self.move_key(&ctx);
        }

        egui::Panel::top("tabs").show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.selectable_value(&mut self.tab, Tab::Street, "Street");
                ui.selectable_value(&mut self.tab, Tab::Deck, "Deck");
                ui.selectable_value(&mut self.tab, Tab::Net, "Net");
                ui.selectable_value(&mut self.tab, Tab::Veil, "Veil");
            });
        });

        egui::Panel::left("status").show(ui, |ui| {
            ui.heading("Status");
            if let Some((food, mood, money, rations)) = self.server.world.needs(self.actor) {
                ui.label(status_line(food, mood, money));
                let bank = self
                    .server
                    .world
                    .bank_balance(self.actor)
                    .map(|b| format!("bank ${:.2}", b as f64 / 100.0))
                    .unwrap_or_else(|| "bank —".to_string());
                ui.label(bank);
                let units = sim::Res::ALL
                    .iter()
                    .map(|r| format!("{} {}", r.name(), self.server.world.units(self.actor, *r)))
                    .collect::<Vec<_>>()
                    .join(" · ");
                ui.label(format!("components: {units}"));
                // Heat (design/05): alignment/standing are pure functions of
                // the record, so settling the fine clears both.
                let standing = self.server.world.standing();
                let mass = self.server.world.record_mass();
                ui.label(format!(
                    "standing {standing:?} · alignment {:?} · mass {mass:.1}",
                    self.server.world.alignment()
                ));
                if mass > 0.0 {
                    let cost = self.server.world.fine_cost();
                    if ui
                        .button(format!("Pay fine (${:.2})", cost as f64 / 100.0))
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
                ui.label(format!("rations {rations} tick {}", self.server.world.tick));
                let tick = self.server.world.tick;
                ui.label(format!(
                    "Day {} {:02}:{:02}",
                    tick / 1440 + 1,
                    (tick / 60) % 24,
                    tick % 60
                ));
                if food < 30 || mood < 30 {
                    ui.label(
                        egui::RichText::new("! needs low — Eat / Watch soon")
                            .color(egui::Color32::RED),
                    );
                }
            }
            ui.label(
                egui::RichText::new(format!(
                    "Objective: {}",
                    self.server.world.advise(self.actor)
                ))
                .strong(),
            );
            let cop = Timetable::cop_day();
            ui.label(format!(
                "cop 10:00 -> {:?}",
                cop_duty(&cop, 10, Standing::Clean)
            ));
            ui.horizontal(|ui| {
                if ui.button("Save").clicked() {
                    let msg = persist_save(&self.server);
                    self.say(msg);
                }
                if ui.button("Load").clicked() {
                    let msg = persist_load(&mut self.server);
                    self.say(msg);
                }
            });
        });

        egui::Panel::bottom("console").show(ui, |ui| {
            ui.horizontal(|ui| {
                let resp = ui.add(
                    egui::TextEdit::singleline(&mut self.input)
                        .hint_text("eat | work 0/1 | watch | help"),
                );
                if resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    self.run_command();
                }
            });
            egui::ScrollArea::vertical()
                .max_height(90.0)
                .show(ui, |ui| {
                    ui.label("Log");
                    for line in self.log.iter().rev().take(8) {
                        ui.label(line);
                    }
                });
        });

        egui::CentralPanel::default().show(ui, |ui| match self.tab {
            Tab::Street => {
                // Job buttons carry names + pay from data/jobs.yaml so the
                // choice means something; indices feed Verb::Work like before.
                // Fallback labels keep verbs available if the file is corrupt
                // (data tests pin the file, so this is belt-and-braces).
                // (id, pay, (resource, need, have)) when the job is gated.
                type JobRow = (String, i64, Option<(sim::Res, u32, u32)>);
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
                ui.horizontal(|ui| {
                    if ui.button("Eat").clicked() {
                        self.do_verb(Verb::Eat);
                    }
                    if ui.button("Watch").clicked() {
                        self.do_verb(Verb::Watch);
                    }
                    if ui.button("Sleep").clicked() {
                        self.do_verb(Verb::Sleep);
                    }
                });
                // Manual coding (design/10): one unit per click, at a mood
                // cost. Producers come later; this is the slow path.
                ui.horizontal(|ui| {
                    ui.label("Code:");
                    for res in sim::Res::ALL {
                        let have = self.server.world.units(self.actor, res);
                        if ui
                            .button(format!("Code {} ({have})", res.name()))
                            .clicked()
                        {
                            self.do_verb(Verb::Code { res });
                        }
                    }
                });
                // Job board on its own wrapped row: five long labels with
                // live counts overflow a single row and push later controls
                // outside the clip rect (their clicks are then ignored).
                ui.horizontal_wrapped(|ui| {
                    for (i, (id, pay, req)) in jobs.iter().enumerate() {
                        let label = match req {
                            Some((res, need, have)) => format!(
                                "Work {i}: {id} {pay}c [{} {have}/{need}]",
                                res.name()
                            ),
                            None => format!("Work {i}: {id} {pay}c"),
                        };
                        if ui.button(label).clicked() {
                            self.do_verb(Verb::Work(i as u8));
                        }
                    }
                });
                ui.label("@ you · n folk · $ counter (gold) · . floor · # blocked — stand by $, Buy");
                let here = self.server.world.pos_of(self.actor).unwrap_or(pos(8, 8));
                // Snapshot world reads up front: UI callbacks below need &mut self.
                // Counter shows within reach (counter serves adjacent tiles).
                let terrain = self.server.world.section_at(here.sx, here.sy).cloned();
                let counter: Vec<(String, i64, u32)> = self.server.world
                    .shop_near_at(here.x, here.y)
                    .map(|shop| shop.stock.iter().map(|(k, l)| (k.clone(), l.price_cents, l.qty)).collect())
                    .unwrap_or_default();
                let npcs = self.server.world.npc_positions();
                ui.heading(format!("Street — section ({}, {})", here.sx, here.sy));
                // Talk panel.
                if self.talking.is_none() {
                    let mut started = false;
                    ui.horizontal(|ui| {
                        for (id, role) in self.nearby_npcs() {
                            if ui.button(format!("Talk {role:?}")).clicked() {
                                if let Some(dlg) = self.dialogue_for(role) {
                                    let t = sim::Conversation::new(dlg, None);
                                    self.say(format!("[{id}] {}", t.text()));
                                    self.talking = Some(t);
                                    started = true;
                                }
                            }
                            let _ = started;
                        }
                    });
                } else {
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
                }
                // Trade panel: counter sale when standing on a shop tile.
                if !counter.is_empty() {
                    ui.separator();
                    ui.label("Counter — type buy <item> or click:");
                    for (item, price, qty) in counter {
                        ui.horizontal(|ui| {
                            ui.label(format!(
                                "{} — {}c ({} left){}",
                                item,
                                price,
                                qty,
                                item_blurb(&item)
                            ));
                            if ui.button("Buy").clicked() {
                                self.do_verb(Verb::Buy(item.clone()));
                            }
                        });
                    }
                    ui.separator();
                }
                let (ax, ay) = (here.x, here.y);
                let shop_tiles = self.server.world.shop_positions();
                // Glyph colors follow the active theme: hardcoding for one
                // theme made @ invisible on the other (light default).
                let dark = ui.visuals().dark_mode;
                let ink = |glyph: &str| match glyph {
                    "@" if dark => egui::Color32::WHITE,
                    "@" => egui::Color32::from_rgb(0, 90, 200),
                    "n" if dark => egui::Color32::LIGHT_GREEN,
                    "n" => egui::Color32::from_rgb(0, 140, 0),
                    "$" if dark => egui::Color32::from_rgb(255, 190, 40),
                    "$" => egui::Color32::from_rgb(176, 118, 0),
                    "#" => egui::Color32::DARK_GRAY,
                    _ => egui::Color32::GRAY,
                };
                egui::Grid::new("street").spacing([2.0, 2.0]).show(ui, |ui| {
                    for y in 0..16u8 {
                        for x in 0..16u8 {
                            let ch = if (x, y) == (ax, ay) {
                                "@"
                            } else if npcs.iter().any(|(_, nx, ny)| (*nx, *ny) == (x, y)) {
                                "n"
                            } else if shop_tiles.contains(&(x, y)) {
                                "$"
                            } else if terrain.as_ref().map(|s| s.walkable(x, y)).unwrap_or(true) {
                                "."
                            } else {
                                "#"
                            };
                            let color = ink(ch);
                            ui.label(egui::RichText::new(ch).monospace().color(color));
                        }
                        ui.end_row();
                    }
                });
            }
            Tab::Deck => {
                ui.heading("Deck — world host 1");
                ui.label("H killer head · o tail · P producer · click a cell, then Load/Strike/Farm · warden defends this rig");
                ui.horizontal(|ui| {
                    if ui.button("Load killer @selected").clicked() {
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
                    if ui.button("Strike selected").clicked() {
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
                // Producers (design/10): deploy a maker on the selected cell.
                // It then yields free units every act.
                ui.horizontal_wrapped(|ui| {
                    ui.label("Farm @selected:");
                    for res in sim::Res::ALL {
                        if ui.button(format!("Farm {}", res.name())).clicked() {
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
                // Card table (design/03): play a card at the selected cell.
                // Self cards target your program; attacks/debuffs the enemy.
                ui.horizontal_wrapped(|ui| {
                    ui.label("Cards @selected:");
                    for card in sim::Host::implemented_cards() {
                        if ui.button(card).clicked() {
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
                // Get out clean (design/03): scrub your trail on the rig.
                ui.horizontal(|ui| {
                    let logs = self
                        .server
                        .world
                        .host(HostId(1))
                        .map(|h| h.logs_visible(self.actor).len())
                        .unwrap_or(0);
                    ui.label(format!("your trail on this rig: {logs}"));
                    if ui.button("Scrub log").clicked() {
                        match self.server.apply_one(Intent {
                            actor: self.actor,
                            verb: Verb::ScrubLog,
                        }) {
                            Ok(_) => self.say("log scrubbed".to_string()),
                            Err(e) => self.say(format!("scrub failed: {e}")),
                        }
                    }
                });
                // Snapshot arena state up front: buttons below need &mut self.
                // Producers get their own glyph + per-resource color so the
                // economy loop is visible in the arena, not just in numbers.
                let (heads, tails, prod, cpu, total) = match self.server.world.host(HostId(1)) {
                    Some(h) => {
                        let mut heads = std::collections::BTreeMap::new();
                        let mut tails = std::collections::BTreeSet::new();
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
                            for t in &p.tail {
                                tails.insert((t.0, t.1));
                            }
                        }
                        (heads, tails, h.producer_summary(self.actor), h.cpu_load(), h.cpu_total)
                    }
                    None => (
                        std::collections::BTreeMap::new(),
                        std::collections::BTreeSet::new(),
                        vec![],
                        0,
                        0,
                    ),
                };
                egui::Grid::new("deck").spacing([2.0, 2.0]).show(ui, |ui| {
                    for y in 0..8u8 {
                        for x in 0..12u8 {
                            let (glyph, color) = if let Some(Some(res)) = heads.get(&(x, y)) {
                                // Producer head: P in the resource's color.
                                (
                                    "P",
                                    match res {
                                        sim::Res::Loop => egui::Color32::from_rgb(120, 200, 120),
                                        sim::Res::Data => egui::Color32::from_rgb(110, 190, 230),
                                        sim::Res::Algo => egui::Color32::from_rgb(220, 150, 240),
                                    },
                                )
                            } else if heads.contains_key(&(x, y)) {
                                ("H", egui::Color32::LIGHT_RED)
                            } else if tails.contains(&(x, y)) {
                                ("o", egui::Color32::GRAY)
                            } else if Some((x, y)) == self.sel {
                                ("X", egui::Color32::YELLOW)
                            } else {
                                (".", egui::Color32::DARK_GRAY)
                            };
                            if ui
                                .button(egui::RichText::new(glyph).monospace().color(color))
                                .clicked()
                            {
                                self.sel = Some((x, y));
                            }
                        }
                        ui.end_row();
                    }
                });
                ui.label(format!("CPU {cpu}/{total}"));
                if let Some(h) = self.server.world.host(HostId(1)) {
                    ui.label(format!(
                        "arch {} · vulns {} · breached {}",
                        h.arch().name(),
                        h.vuln_ids().len(),
                        if h.breached { "yes" } else { "no" }
                    ));
                }
                // Exploit the rig's holes (design/03): needs a program of
                // yours on the host as the delivery vehicle.
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
                    ui.label("Exploit:");
                    for v in &vulns {
                        if ui.button(v).clicked() {
                            match self.server.apply_one(Intent {
                                actor: self.actor,
                                verb: Verb::Exploit { vuln: v.clone() },
                            }) {
                                Ok(_) => self.say(format!("exploited {v}")),
                                Err(e) => self.say(format!("exploit failed: {e}")),
                            }
                        }
                    }
                    if breached && ui.button("Send trojan").clicked() {
                        match self.server.apply_one(Intent {
                            actor: self.actor,
                            verb: Verb::SendTrojan { host: HostId(1) },
                        }) {
                            Ok(_) => self.say("trojan planted — daily spam income".to_string()),
                            Err(e) => self.say(format!("trojan failed: {e}")),
                        }
                    }
                });
                // Producer yield summary: what the farm loop delivers.
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
            }
            Tab::Net => {
                ui.heading("Net — world graph (Oldnet/Betanet)");
                ui.label("More hops = safer trace, higher tolls. Laser is fast but tolled. Dark sections drop their hosts.");
                // Graph snapshot: nodes (with net kind + power state) and links.
                let node_ids = self.server.world.net.node_ids();
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
                if ui.button("Route 1→3").clicked() {
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
                if ui.button("Damage wire 1-2").clicked() {
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
                    if ui.button("Cut power sec (1,0)").clicked() {                        match self.server.apply_one(Intent {
                            actor: self.actor,
                            verb: Verb::SetPower { sx: 1, sy: 0, up: false },
                        }) {
                            Ok(_) => self.say("power cut in section (1,0)".to_string()),
                            Err(e) => self.say(format!("power cut failed: {e}")),
                        }
                    }
                    if ui.button("Restore power").clicked() {
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
                    });
                    if ui
                        .button(if blocked { "Unblock" } else { "Block 1 on 2" })
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
            }
            Tab::Veil => {
                ui.heading("Veil — helmet space");
                if !self.server.world.veil.present(self.actor) {
                    ui.horizontal(|ui| {
                        ui.label("mindpower");
                        ui.add(egui::DragValue::new(&mut self.mp).range(0..=20));
                        if ui.button("Jack in @lobby").clicked() {
                            let mp = self.mp;
                            let msg = match self.server.apply_one(Intent {
                                actor: self.actor,
                                verb: Verb::VeilEnter { mp },
                            }) {
                                Ok(_) => format!("jacked in (mp {mp})"),
                                Err(e) => format!("entry refused: {e}"),
                            };
                            self.say(msg);
                        }
                    });
                    ui.label("Needs 5+ mindpower. Death ejects with drain.");
                } else {
                    let area = self.server.world.veil.area_of(self.actor).unwrap_or(0);
                    let name = self.server.world.veil.area_info(area).map(|(n, _, _)| n).unwrap_or_else(|| "lobby".to_string());
                    ui.label(format!("area: {name} (mp {})", self.server.world.veil.mindpower(self.actor).unwrap_or(0)));
                    if let Some(host) = self.server.world.veil.controlled(self.actor) {
                        ui.label(format!("controlling {host:?}"));
                    }
                    let exits: Vec<u64> = self.server.world.veil.area_info(area).map(|(_, e, _)| e).unwrap_or_default();
                    ui.horizontal(|ui| {
                        for a in exits {
                            let label = self.server.world.veil.area_info(a).map(|(n, _, x)| format!("{n}{}", if x { " [exit]" } else { "" })).unwrap_or_else(|| format!("area {a}"));
                            if ui.button(format!("→ {label}")).clicked() {
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
                    ui.horizontal(|ui| {
                        if ui.button("Seize demo host (mp 3)").clicked() {
                            let msg = match self.server.apply_one(Intent {
                                actor: self.actor,
                                verb: Verb::VeilSeize,
                            }) {
                                Ok(_) => "host 70 seized".to_string(),
                                Err(e) => format!("seize failed: {e}"),
                            };
                            self.say(msg);
                        }
                        if ui.button("Eject").clicked() {
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
                }
            }
        });
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
            tree: std::collections::HashMap<NodeId, Node>,
        }

        impl Harness {
            fn new() -> Self {
                let ctx = egui::Context::default();
                ctx.enable_accesskit();
                let mut h = Self {
                    shell: Shell::new(None),
                    ctx,
                    time: 0.0,
                    tree: Default::default(),
                };
                h.pass_with(vec![]);
                h
            }

            fn input(&mut self, events: Vec<egui::Event>) -> egui::RawInput {
                self.time += 0.1;
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1280.0, 800.0),
                    )),
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
                let at = self.button_center(label);
                let mods = egui::Modifiers::default();
                self.pass_with(vec![egui::Event::PointerButton {
                    pos: at,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: mods,
                }]);
                let at = self.button_center(label);
                self.pass_with(vec![egui::Event::PointerButton {
                    pos: at,
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: mods,
                }]);
                self.pass_with(vec![]);
            }

            /// Click the bottom-right-most button with this label (for grids
            /// of identical labels like map cells).
            fn click_last(&mut self, label: &str) {
                let at = self.button_center_last(label);
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
            }

            /// Click the first button (reading order) whose label starts with
            /// `prefix`. Needed when labels carry live counts that change
            /// after each click.
            fn click_prefix(&mut self, prefix: &str) {
                let mods = egui::Modifiers::default();
                let find = |tree: &std::collections::HashMap<NodeId, Node>| -> egui::Pos2 {
                    let mut cands: Vec<(f64, f64, egui::Pos2)> = tree
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
                                egui::pos2(
                                    ((b.x0 + b.x1) / 2.0) as f32,
                                    ((b.y0 + b.y1) / 2.0) as f32,
                                ),
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
                };
                let at = find(&self.tree);
                self.pass_with(vec![egui::Event::PointerButton {
                    pos: at,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: mods,
                }]);
                let at = find(&self.tree);
                self.pass_with(vec![egui::Event::PointerButton {
                    pos: at,
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: mods,
                }]);
                self.pass_with(vec![]);
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
        fn veil_full_flow_through_clicks() {
            let mut h = Harness::new();
            h.click("Veil");
            // mp slider defaults to 8: jack in, walk to ops, seize, eject.
            h.click("Jack in @lobby");
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
            h.click("Seize demo host (mp 3)");
            assert!(
                h.shell.log.iter().any(|l| l.contains("host 70 seized")),
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
            assert!(h.has_text("bank $5.00"), "text: {:?}", h.labels());
            // Deposit $1.50 through the console; pocket drops, bank rises.
            let at = h.text_input_center();
            h.click_at(at);
            h.pass_with(vec![egui::Event::Text("deposit 150".into())]);
            h.tap_key(egui::Key::Enter);
            h.pass_with(vec![]);
            assert!(h.has_text("bank $6.50"), "text: {:?}", h.labels());
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
        fn heat_shows_and_paying_a_fine_settles_it() {
            let mut h = Harness::new();
            // Clean to start: standing shown, no fine button.
            assert!(h.has_text("standing Clean"), "text: {:?}", h.labels());
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
            assert!(!h.has_text("standing Clean"), "text: {:?}", h.labels());
            // Settle: the record clears and pursuit ends.
            h.click_prefix("Pay fine");
            assert!(h.has_text("standing Clean"), "text: {:?}", h.labels());
            assert!(
                h.shell.log.iter().any(|l| l.contains("record settled")),
                "log: {:?}",
                h.shell.log
            );
        }

        #[test]
        fn exploit_then_trojan_through_clicks() {
            let mut h = Harness::new();
            h.click("Deck");
            // Exploit needs a program of ours on the host first.
            h.click_last(".");
            h.click("Farm loop");
            // Exploit the office-ws privilege hole, then plant the trojan.
            h.click("cve-2006-0005");
            assert!(h.has_text("breached yes"), "text: {:?}", h.labels());
            h.click("Send trojan");
            assert!(
                h.shell.log.iter().any(|l| l.contains("trojan planted")),
                "log: {:?}",
                h.shell.log
            );
            // Every step was a logged act: farm, exploit, trojan.
            assert_eq!(h.shell.server.log.len(), 3);
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
            h.click("tank");
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
            assert!(
                h.has_text("components: loop 0 · data 0 · algo 0"),
                "text: {:?}",
                h.labels()
            );
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
            assert!(
                h.has_text("components: loop 9 · data 0 · algo 0"),
                "text: {:?}",
                h.labels()
            );
            // Farm + two watches are all logged acts.
            assert_eq!(h.shell.server.log.len(), 3);
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

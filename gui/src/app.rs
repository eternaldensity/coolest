//! Interactive eframe shell over the deterministic sim.
//! Street is server-backed (intent queue); Deck/Net tabs hold local demo
//! state until the multiplayer unification (design/08). `--frames N`
//! runs N frames then exits (headless smoke via xvfb).

use client_web::status_line;
use eframe::egui;
use server::Server;
use sim::{
    CellPos, Dir, EntityId, Host, HostId, Intent, LinkKind, NetGraph, NetKind, Pos,
    Section, Standing, Timetable, Verb, cop_duty, gen_section, Ports,
};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tab {
    Street,
    Deck,
    Net,
}

fn pos(x: u8, y: u8) -> Pos {
    Pos { city: 1, sx: 0, sy: 0, z: 0, x, y }
}

pub struct Shell {
    server: Server,
    actor: EntityId,
    section: Section,
    tab: Tab,
    input: String,
    log: Vec<String>,
    deck: Host,
    net: NetGraph,
    sel: Option<(u8, u8)>,
    frames_left: Option<u32>,
}

impl Shell {
    pub fn new(frames: Option<u32>) -> Self {
        let mut server = Server::new(42);
        let actor = EntityId(1);
        server.world.spawn(actor, pos(8, 8), 10);
        let section = gen_section(7, 0, 0, Ports::default());

        let mut net = NetGraph::new();
        net.add_node(HostId(1), NetKind::Oldnet, 10, false);
        net.add_node(HostId(2), NetKind::Oldnet, 100, true);
        net.add_node(HostId(3), NetKind::Betanet, 50, false);
        net.add_link(HostId(1), HostId(2), LinkKind::Wired { damaged: false }, 5, 0).ok();
        net.add_link(HostId(2), HostId(3), LinkKind::Laser, 40, 10).ok();

        let mut shell = Self {
            server,
            actor,
            section,
            tab: Tab::Street,
            input: String::new(),
            log: vec!["Welcome to FIDOM. Arrows/hjkl move. Type help + Enter.".to_string()],
            deck: Host::new(HostId(1), 12, 8, 100),
            net,
            sel: None,
            frames_left: frames,
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
            self.say(format!("{} rations={rations} tick={}", status_line(food, mood, money), self.server.world.tick));
        }
    }

    fn do_verb(&mut self, verb: Verb) {
        self.server.submit(Intent { actor: self.actor, verb });
        let out = self.server.step();
        if out.is_empty() {
            self.say("rejected (timing/lockout?)".to_string());
        }
        self.say_status();
    }

    fn run_command(&mut self) {
        let cmd = std::mem::take(&mut self.input);
        let cmd = cmd.trim().to_lowercase();
        match cmd.as_str() {
            "help" => self.say("move: arrows/hjkl | eat | work 0 | work 1 | watch".to_string()),
            "eat" => self.do_verb(Verb::Eat),
            "watch" => self.do_verb(Verb::Watch),
            "work 0" => self.do_verb(Verb::Work(0)),
            "work 1" => self.do_verb(Verb::Work(1)),
            "" => {}
            _ => self.say(format!("unknown: {cmd}")),
        }
    }

    fn move_key(&mut self, ctx: &egui::Context) {
        let dir = if ctx.input(|i| i.key_pressed(egui::Key::ArrowUp) || i.key_pressed(egui::Key::K)) {
            Some(Dir::N)
        } else if ctx.input(|i| i.key_pressed(egui::Key::ArrowDown) || i.key_pressed(egui::Key::J)) {
            Some(Dir::S)
        } else if ctx.input(|i| i.key_pressed(egui::Key::ArrowLeft) || i.key_pressed(egui::Key::H)) {
            Some(Dir::W)
        } else if ctx.input(|i| i.key_pressed(egui::Key::ArrowRight) || i.key_pressed(egui::Key::L)) {
            Some(Dir::E)
        } else {
            None
        };
        if let Some(d) = dir {
            self.do_verb(Verb::Move(d));
        }
    }

    fn street_pos(&self) -> (u8, u8) {
        // Actor position is world truth (server); section is the visible chunk.
        self.server.world.pos_of(self.actor).map(|p| (p.x, p.y)).unwrap_or((8, 8))
    }
}

impl eframe::App for Shell {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
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
            });
        });

        egui::Panel::left("status").show(ui, |ui| {
            ui.heading("Status");
            if let Some((food, mood, money, rations)) = self.server.world.needs(self.actor) {
                ui.label(status_line(food, mood, money));
                ui.label(format!("rations {rations} tick {}", self.server.world.tick));
            }
            let cop = Timetable::cop_day();
            ui.label(format!("cop 10:00 -> {:?}", cop_duty(&cop, 10, Standing::Clean)));
        });

        egui::Panel::bottom("console").show(ui, |ui| {
            ui.horizontal(|ui| {
                let resp = ui.add(egui::TextEdit::singleline(&mut self.input).hint_text("eat | work 0 | watch | help"));
                if resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    self.run_command();
                }
            });
            egui::ScrollArea::vertical().max_height(90.0).show(ui, |ui| {
                for line in self.log.iter().rev().take(8) {
                    ui.label(line);
                }
            });
        });

        egui::CentralPanel::default().show(ui, |ui| match self.tab {
            Tab::Street => {
                ui.heading("Street — M0.5 slice");
                let (ax, ay) = self.street_pos();
                egui::Grid::new("street").spacing([2.0, 2.0]).show(ui, |ui| {
                    for y in 0..16u8 {
                        for x in 0..16u8 {
                            let ch = if (x, y) == (ax, ay) {
                                "@"
                            } else if self.section.walkable(x, y) {
                                "."
                            } else {
                                "#"
                            };
                            ui.label(egui::RichText::new(ch).monospace());
                        }
                        ui.end_row();
                    }
                });
            }
            Tab::Deck => {
                ui.heading("Deck — demo host");
                ui.horizontal(|ui| {
                    if ui.button("Load killer @selected").clicked() {
                        match self.sel {
                            Some((x, y)) => {
                                let id = EntityId(100 + self.deck.progs.len() as u64);
                                match self.deck.load(id, EntityId(1), CellPos(x, y), 3, 1) {
                                    Ok(()) => self.say(format!("loaded killer {id:?}")),
                                    Err(e) => self.say(format!("load failed: {e}")),
                                }
                            }
                            None => self.say("click a cell first".to_string()),
                        }
                    }
                    if ui.button("Strike selected").clicked() {
                        match self.sel {
                            Some((x, y)) => match self.deck.strike(CellPos(x, y)) {
                                Ok(killed) => self.say(format!("strike: killed={killed}")),
                                Err(e) => self.say(format!("strike failed: {e}")),
                            },
                            None => self.say("click a cell first".to_string()),
                        }
                    }
                });
                egui::Grid::new("deck").spacing([2.0, 2.0]).show(ui, |ui| {
                    for y in 0..8u8 {
                        for x in 0..12u8 {
                            let label = if Some((x, y)) == self.sel { "[X]" } else { "[ ]" };
                            if ui.button(label).clicked() {
                                self.sel = Some((x, y));
                            }
                        }
                        ui.end_row();
                    }
                });
                ui.label(format!("CPU {}/{}", self.deck.cpu_load(), self.deck.cpu_total));
            }
            Tab::Net => {
                ui.heading("Net — Oldnet/Betanet demo");
                if ui.button("Route 1→3").clicked() {
                    match self.net.route(HostId(1), HostId(3)) {
                        Ok(r) => self.say(format!(
                            "route {:?} {}ms toll {} cap {} gw={} thr={}",
                            r.path, r.latency_ms, r.toll_cents, r.cap_mbps, r.gateway_ok,
                            r.trace_threshold()
                        )),
                        Err(e) => self.say(format!("no route: {e}")),
                    }
                }
                if ui.button("Damage wire 1-2").clicked() {
                    match self.net.damage_wire(HostId(1), HostId(2)) {
                        Ok(()) => self.say("wire 1-2 damaged".to_string()),
                        Err(e) => self.say(format!("damage failed: {e}")),
                    }
                }
            }
        });
        ui.ctx().request_repaint();
    }
}

//! Headless smoke demo: exercises M0.5 + M2 + M3 + M4 through the
//! abstract `Renderer`, proving Bevy/Godot can replace this client.
//! Run: cargo run -p client-web

use client_web::{deck_bar, net_line, status_line, WebClient};
use sim::{
    cop_duty, trace_evidence, Act, Alignment, CellPos, Crime, DeckView, Dir, EntityId, Host,
    HostId, Intent, Ledger, License, Mailbox, NetGraph, NetKind, Pos, RecordService, RenderView,
    Renderer, Standing, Timetable, Veil, Verb, World,
};

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

fn main() {
    let mut ui = WebClient::new();

    // Data files drive runtime (design/11): jobs tune payouts, vulns seed hosts.
    let jobs = sim::parse_jobs(include_str!("../../data/jobs.yaml")).unwrap();
    let vulns = sim::parse_vulns(include_str!("../../data/vulns.yaml")).unwrap();
    let cards = sim::parse_cards(include_str!("../../data/cards.yaml")).unwrap();
    let items = sim::parse_items(include_str!("../../data/items.yaml")).unwrap();
    let contracts = sim::parse_contracts(include_str!("../../data/contracts.yaml")).unwrap();
    let kinds = sim::parse_kinds(include_str!("../../data/kinds.yaml")).unwrap();

    // M0.5 survival.
    let mut w = World::new(42);
    w.set_jobs(&jobs);
    w.set_cards(&cards);
    w.set_items(&items);
    w.set_kinds(&kinds);
    w.spawn(EntityId(1), pos(8, 8), 10);
    let act = |w: &mut World, v: Verb| {
        // Jump the clock like the server scheduler would.
        w.tick = w.next_at(EntityId(1));
        w.apply(&Intent {
            actor: EntityId(1),
            verb: v,
        })
        .ok();
    };
    // M0.5 survival. Jobs consume components (design/10), so stock the
    // rig first — the manual path would be four `Code` acts.
    w.ensure_host(HostId(1))
        .add_stock(EntityId(1), sim::Res::Loop, 4);
    act(&mut w, Verb::Work(0));
    act(&mut w, Verb::Eat);
    act(&mut w, Verb::Watch);
    let (food, mood, money, rations) = w.needs(EntityId(1)).unwrap();
    ui.say(&status_line(food, mood, money));
    println!(
        "survival: {} rations={rations}",
        status_line(food, mood, money)
    );

    // M6a items: buy/equip/use through the same public API clients use.
    w.add_item(EntityId(1), "knife", 1).unwrap();
    w.equip_item(EntityId(1), "knife").unwrap();
    w.add_item(EntityId(1), "medkit", 1).unwrap();
    w.damage(EntityId(1), 6);
    let healed = w.use_item(EntityId(1), "medkit").unwrap();
    println!(
        "items: weapon={:?} hp={} healed={healed:?} weight={}g",
        w.equipped_weapon(EntityId(1)).map(|d| d.name),
        w.hp(EntityId(1)),
        w.carry_weight(EntityId(1))
    );

    // v0.6 program identity: Scanner reveals, and its data affinity pays +1.
    let scanner = w.kind_def(sim::ProgramKind::Scanner);
    w.ensure_host(HostId(1))
        .load_kind(EntityId(200), EntityId(1), CellPos(0, 0), &scanner)
        .unwrap();
    w.tick = w.next_at(EntityId(1));
    w.apply(&Intent {
        actor: EntityId(1),
        verb: Verb::DeckScan,
    })
    .unwrap();
    w.tick = w.next_at(EntityId(1));
    w.apply(&Intent {
        actor: EntityId(1),
        verb: Verb::DeckFarmKind {
            x: 1,
            y: 0,
            res: sim::Res::Data,
            kind: sim::ProgramKind::Scanner,
        },
    })
    .unwrap();
    println!(
        "programs: scanned={} data={}",
        w.host(HostId(1)).unwrap().scanned(EntityId(1)),
        w.units(EntityId(1), sim::Res::Data)
    );

    // Timetable + cop duty.
    let cop = Timetable::cop_day();
    println!(
        "cop 10:00 clean -> {:?}, wanted -> {:?}",
        cop_duty(&cop, 10, Standing::Clean),
        cop_duty(&cop, 10, Standing::Wanted)
    );

    // M2 deck fight.
    let mut h = Host::new(HostId(1), 12, 8, 100);
    // Archetype drives which holes exist (design/03 host kinds).
    let installed = h.set_arch(sim::HostArch::OfficeWS, &vulns);
    println!("vulns registered: {installed} of {}", vulns.len());
    h.load(EntityId(10), EntityId(1), CellPos(2, 2), 3, 1)
        .unwrap();
    h.step(EntityId(10), CellPos(3, 2)).unwrap();
    h.step(EntityId(10), CellPos(4, 2)).unwrap();
    ui.say(&deck_bar(h.cpu_load(), h.cpu_total));
    h.strike(CellPos(2, 2)).unwrap();
    h.strike(CellPos(3, 2)).unwrap();
    let killed = h.strike(CellPos(4, 2)).unwrap();
    println!(
        "deck: {} killed={killed}",
        deck_bar(h.cpu_load(), h.cpu_total)
    );
    if killed {
        w.mark_hack(EntityId(1));
    }
    ui.render(&RenderView::Deck(DeckView {
        host: HostId(1),
        ram_used: h.cpu_load(),
        ram_total: h.cpu_total,
    }));

    // M2d retaliation: a defended rig hunts intruders and traces them.
    let mut dh = Host::new(HostId(9), 12, 8, 100);
    dh.set_owner(Some(Host::WARDEN));
    dh.set_monitor(true);
    dh.load(EntityId(70), EntityId(1), CellPos(2, 2), 3, 1)
        .unwrap();
    let ward = dh.retaliate();
    w.mark_traced(EntityId(1));
    println!(
        "warden: guard={} traced={} foothold={}",
        ward.warden,
        ward.traced,
        dh.progs.contains_key(&70)
    );

    // M3 net route.
    let mut g = NetGraph::new();
    g.add_node(HostId(1), NetKind::Oldnet, 10, false);
    g.add_node(HostId(2), NetKind::Oldnet, 100, true);
    g.add_node(HostId(3), NetKind::Betanet, 50, false);
    g.add_link(
        HostId(1),
        HostId(2),
        sim::LinkKind::Wired { damaged: false },
        5,
        0,
    )
    .unwrap();
    g.add_link(HostId(2), HostId(3), sim::LinkKind::Laser, 40, 10)
        .unwrap();
    let r = g.route(HostId(1), HostId(3)).unwrap();
    println!(
        "net: {} cap={} gateway_ok={} trace_thr={}",
        net_line(1, 3, r.latency_ms, r.toll_cents as i64),
        r.cap_mbps,
        r.gateway_ok,
        r.trace_threshold()
    );

    // M3c ledger + mail.
    let mut l = Ledger::new();
    let a = l.open(EntityId(1), HostId(1));
    let b = l.open(EntityId(2), HostId(2));
    l.mint(a, 1000).unwrap();
    l.transfer(a, b, 300).unwrap();
    println!(
        "ledger: a={} b={} audit_ok={}",
        l.balance(a).unwrap(),
        l.balance(b).unwrap(),
        l.live_sum() == l.audit_sum()
    );
    let mut m = Mailbox::new();
    m.send(EntityId(1), EntityId(2), "initiate here").unwrap();
    println!("mail: inbox={}", m.inbox.len());

    // M4 crime + license + veil.
    let mut rec = RecordService::new();
    rec.report(Crime {
        act: Act::Theft,
        at_host: Some(HostId(1)),
        tick: 1,
        victim: None,
        evidence: trace_evidence(1.0, Alignment::Authority, true),
    });
    println!(
        "crime: align={:?} standing={:?}",
        rec.alignment(),
        rec.standing()
    );
    let mut lic = License::new();
    for _ in 0..3 {
        lic.add_kill();
    }
    println!("license eligible (dirty record) = {}", lic.eligible(&rec));
    let mut v = Veil::new();
    v.enter(EntityId(1), 8).unwrap();
    v.damage(EntityId(1), 99).unwrap();
    println!("veil: present after death = {}", v.present(EntityId(1)));
    let _ = Dir::N;

    // v0.4 embodied tour, all through the intent pipeline: fight, get
    // Wanted, surrender, then use the real mindpower Veil.
    let mut s = World::new(7);
    s.spawn(EntityId(1), pos(8, 8), 1);
    s.spawn_npc(
        EntityId(10),
        sim::Role::Shopkeeper,
        (9, 8),
        (9, 8),
        (9, 8),
        9999,
    );
    // A cop in earshot turns the kill into a full-evidence crime.
    s.spawn_npc(EntityId(20), sim::Role::Cop, (9, 9), (9, 9), (9, 9), 9999);
    s.add_item(EntityId(1), "knife", 1).unwrap();
    s.equip_item(EntityId(1), "knife").unwrap();
    let mut swings = 0;
    while s.npc_role(10).is_some() && swings < 100 {
        s.tick = s.next_at(EntityId(1));
        s.heal(EntityId(1), 999);
        let _ = s.apply(&Intent {
            actor: EntityId(1),
            verb: Verb::Attack {
                target: EntityId(10),
            },
        });
        swings += 1;
    }
    println!(
        "fight: down={} swings={swings} hp={} loot={:?}",
        s.npc_role(10).is_none(),
        s.hp(EntityId(1)),
        s.floor_items_at(pos(9, 8))
    );

    let acct = s.open_account(EntityId(1), HostId(2)).unwrap();
    s.ledger.mint(acct, 5000).unwrap();
    s.tick = s.next_at(EntityId(1));
    s.apply(&Intent {
        actor: EntityId(1),
        verb: Verb::Surrender,
    })
    .unwrap();
    println!(
        "arrest: standing={:?} bank={:?}",
        s.standing(),
        s.bank_balance(EntityId(1))
    );

    s.veil.add_area(1, "lobby", vec![2], false);
    s.veil.add_area(2, "ops", vec![1, 3], false);
    s.veil.add_area(3, "roof", vec![2], true);
    s.veil.set_host(2, EntityId(70), 3);
    s.veil.set_hazard(3, 2);
    s.veil.set_files(2, vec!["plans".into()]);
    s.tick = s.next_at(EntityId(1));
    s.apply(&Intent {
        actor: EntityId(1),
        verb: Verb::VeilEnter,
    })
    .unwrap();
    s.tick = s.next_at(EntityId(1));
    s.apply(&Intent {
        actor: EntityId(1),
        verb: Verb::VeilMove { area: 2 },
    })
    .unwrap();
    s.tick = s.next_at(EntityId(1));
    s.apply(&Intent {
        actor: EntityId(1),
        verb: Verb::VeilCarry {
            name: "plans".into(),
        },
    })
    .unwrap();
    s.tick = s.next_at(EntityId(1));
    s.apply(&Intent {
        actor: EntityId(1),
        verb: Verb::VeilSeize,
    })
    .unwrap();
    s.tick = s.next_at(EntityId(1));
    s.apply(&Intent {
        actor: EntityId(1),
        verb: Verb::SetPower {
            sx: 0,
            sy: 0,
            up: false,
        },
    })
    .unwrap();
    println!(
        "veil: present={} mp={} files={}",
        s.veil.present(EntityId(1)),
        s.mind_of(EntityId(1)).unwrap().mindpower,
        s.veil.carried_files(EntityId(1)).len()
    );

    // v0.5 local contract board: accept a delivery offer, escrow, deliver.
    let mut cw = World::new(8);
    cw.spawn(EntityId(1), pos(8, 8), 1);
    cw.spawn_npc(
        EntityId(10),
        sim::Role::Shopkeeper,
        (9, 8),
        (9, 8),
        (9, 8),
        9999,
    );
    let buyer = cw.open_account(EntityId(1), HostId(1)).unwrap();
    cw.ledger.mint(buyer, 500).unwrap();
    let till = cw.open_account(EntityId(10), HostId(1)).unwrap();
    cw.ledger.mint(till, 1000).unwrap();
    cw.set_contracts(&[contracts[0].clone()]);
    cw.refresh_board();
    cw.add_item(EntityId(1), "medkit", 2).unwrap();
    let offer = cw.offers()[0].id;
    cw.tick = cw.next_at(EntityId(1));
    cw.apply(&Intent {
        actor: EntityId(1),
        verb: Verb::AcceptOffer { offer },
    })
    .unwrap();
    cw.tick = cw.next_at(EntityId(1));
    cw.apply(&Intent {
        actor: EntityId(1),
        verb: Verb::DeliverContract,
    })
    .unwrap();
    println!(
        "contract: done={} bank={:?} rep={}",
        cw.active_contract().is_none(),
        cw.bank_balance(EntityId(1)),
        cw.employer_reputation(EntityId(10))
    );

    println!("funnel: {}", w.funnel_line());
    println!("demo ok, log lines = {}", ui.log.len());
}

//! Headless smoke demo: exercises M0.5 + M2 + M3 + M4 through the
//! abstract `Renderer`, proving Bevy/Godot can replace this client.
//! Run: cargo run -p client-web

use client_web::{deck_bar, net_line, status_line, WebClient};
use sim::{
    cop_duty, trace_evidence, Act, Alignment, CellPos, Crime, DeckView, Dir, EntityId, Host,
    HostId, Intent, Ledger, License, Mailbox, NetGraph, NetKind, Pos, Record, RenderView, Renderer,
    Standing, Timetable, Veil, Verb, World,
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

    // M0.5 survival.
    let mut w = World::new(42);
    w.set_jobs(&jobs);
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
    act(&mut w, Verb::Work(0));
    act(&mut w, Verb::Eat);
    act(&mut w, Verb::Watch);
    let (food, mood, money, rations) = w.needs(EntityId(1)).unwrap();
    ui.say(&status_line(food, mood, money));
    println!(
        "survival: {} rations={rations}",
        status_line(food, mood, money)
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
    let mut rec = Record::new();
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

    println!("funnel: {}", w.funnel_line());
    println!("demo ok, log lines = {}", ui.log.len());
}

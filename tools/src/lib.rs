//! Balance + soak tooling (design/12 gates). Deterministic sweeps with
//! committed goldens in data/golden/: TTK table, trace curve, gen stats.
//! `fidom-tools` binary reprints them; tests fail on drift.

use server::{replay, Server};
use sim::{
    gen_section, CellPos, Dir, EntityId, Host, HostId, Intent, LinkKind, NetKind, Ports, Pos, Rng,
    Verb, World,
};

/// Killer-vs-killer TTK: grow one len-N program, strike tail cells in order,
/// then the head. Returns (len, acts to clear).
pub fn ttk_table() -> Vec<(usize, u32)> {
    let mut out = Vec::new();
    for len in 1..=6usize {
        let mut h = Host::new(HostId(1), 12, 8, 1000);
        h.load(EntityId(10), EntityId(1), CellPos(0, 0), len as u8, 1)
            .unwrap();
        for i in 1..len as u8 {
            h.step(EntityId(10), CellPos(i, 0)).unwrap();
        }
        let mut acts = 0u32;
        for i in 0..len as u8 {
            h.strike(CellPos(i, 0)).unwrap();
            acts += 1;
        }
        debug_assert!(h.progs.is_empty());
        out.push((len, acts));
    }
    out
}

pub fn ttk_csv() -> String {
    let mut s = "len,acts\n".to_string();
    for (len, acts) in ttk_table() {
        s.push_str(&format!("{len},{acts}\n"));
    }
    s
}

/// Trace curve: total ticks to complete per threshold, plain vs scrambled,
/// measured through the real `Host::start_trace` / `tick_trace` interface
/// (+2/tick plain, +1/tick scrambled). Counts the completing tick.
pub fn trace_table() -> Vec<(u32, u32, u32)> {
    let mut out = Vec::new();
    for thr in [4u32, 6, 8, 10] {
        out.push((thr, measure_trace(thr, false), measure_trace(thr, true)));
    }
    out
}

fn measure_trace(threshold: u32, scrambled: bool) -> u32 {
    let mut h = Host::new(HostId(1), 12, 8, 100);
    h.load(EntityId(10), EntityId(1), CellPos(0, 0), 2, 1)
        .unwrap();
    h.start_trace(EntityId(10), threshold).unwrap();
    let mut ticks = 0u32;
    loop {
        ticks += 1;
        if h.tick_trace(scrambled) {
            break;
        }
    }
    ticks
}

pub fn trace_csv() -> String {
    let mut s = "threshold,plain_ticks,scrambled_ticks\n".to_string();
    for (thr, plain, sc) in trace_table() {
        s.push_str(&format!("{thr},{plain},{sc}\n"));
    }
    s
}

/// Generation report: per-seed district/difficulty/reachability/
/// guarantees/ports over `count` seeds. Committed golden in
/// data/golden/gen_100.json; tests fail on drift (design/12 worldgen gate).
pub fn gen_report(count: u64, seed0: u64) -> String {
    use serde_json::json;
    let mut sections = Vec::new();
    let mut reachable = 0u64;
    for i in 0..count {
        let seed = seed0 + i;
        let s = gen_section(seed, 0, 0, Ports::default());
        let ok = s.reachable((8, 8));
        if ok {
            reachable += 1;
        }
        sections.push(json!({
            "seed": seed,
            "district": format!("{:?}", s.district),
            "difficulty": s.difficulty,
            "reachable": ok,
            "guarantees": s.guarantees.iter()
                .map(|(k, x, y)| json!([format!("{k:?}"), x, y]))
                .collect::<Vec<_>>(),
            "ports": {"n": s.ports.n, "s": s.ports.s, "e": s.ports.e, "w": s.ports.w},
            "blocked": s.blocked,
        }));
    }
    let doc = json!({"seed0": seed0, "count": count, "reachable": reachable, "sections": sections});
    let mut out = serde_json::to_string_pretty(&doc).unwrap();
    out.push('\n');
    out
}

/// Generation stats: reachable sections over seeds, plus port-agreement
/// spot checks (east exit of A == west entry demanded of B).
pub fn gen_stats(count: u64, seed0: u64) -> (u64, u64) {
    let mut ok = 0u64;
    for i in 0..count {
        let s = gen_section(seed0 + i, 0, 0, Ports::default());
        if s.reachable((8, 8)) {
            ok += 1;
        }
    }
    (ok, count)
}

/// Random-walk soak: N actors, T ticks of random legal-ish moves.
/// Returns the snapshot hash. Deterministic per seed.
pub fn soak(seed: u64, actors: usize, ticks: u64) -> u64 {
    let mut rng = Rng::new(seed);
    let mut w = World::new(seed);
    for a in 0..actors {
        let x = 1 + (rng.below(14)) as u8;
        let y = 1 + (rng.below(14)) as u8;
        w.spawn(
            EntityId(1000 + a as u64),
            Pos {
                city: 1,
                sx: 0,
                sy: 0,
                z: 0,
                x,
                y,
            },
            1 + rng.below(10) as u64,
        );
    }
    let dirs = [Dir::N, Dir::S, Dir::E, Dir::W];
    for _ in 0..ticks {
        let a = rng.below(actors.max(1) as u32) as u64;
        let d = dirs[rng.below(4) as usize];
        w.tick = w.next_at(EntityId(1000 + a)).max(w.tick);
        let _ = w.apply(&Intent {
            actor: EntityId(1000 + a),
            verb: Verb::Move(d),
        });
    }
    w.snapshot_hash()
}

/// Chaos soak (design/12 nightly): random walks plus periodic power
/// kill/restore on a live tri-net, with blackout-aware routing attempted
/// throughout. Returns the snapshot hash. Deterministic per seed.
pub fn chaos_soak(seed: u64, actors: usize, ticks: u64) -> u64 {
    let mut rng = Rng::new(seed);
    let mut w = World::new(seed);
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
    w.power.anchor(HostId(1), (0, 0));
    w.power.anchor(HostId(2), (1, 0));
    w.power.anchor(HostId(3), (0, 1));
    for a in 0..actors {
        let x = 1 + (rng.below(14)) as u8;
        let y = 1 + (rng.below(14)) as u8;
        w.spawn(
            EntityId(1000 + a as u64),
            Pos {
                city: 1,
                sx: 0,
                sy: 0,
                z: 0,
                x,
                y,
            },
            1 + rng.below(10) as u64,
        );
    }
    let dirs = [Dir::N, Dir::S, Dir::E, Dir::W];
    for t in 0..ticks {
        if t % 97 == 0 {
            let s = (rng.below(2) as i32, rng.below(2) as i32);
            if rng.below(2) == 0 {
                w.power.kill(s);
            } else {
                w.power.restore(s);
            }
        }
        if t % 31 == 0 {
            let _ = w.power.powered_route(&w.net, HostId(1), HostId(3));
        }
        let a = rng.below(actors.max(1) as u32) as u64;
        let d = dirs[rng.below(4) as usize];
        w.tick = w.next_at(EntityId(1000 + a)).max(w.tick);
        let _ = w.apply(&Intent {
            actor: EntityId(1000 + a),
            verb: Verb::Move(d),
        });
    }
    w.snapshot_hash()
}
#[derive(serde::Deserialize)]
struct SpawnSpec {
    id: u64,
    x: u8,
    y: u8,
    speed: u64,
    #[serde(default = "default_city")]
    city: u32,
    #[serde(default)]
    sx: i32,
    #[serde(default)]
    sy: i32,
    #[serde(default)]
    z: i8,
}

fn default_city() -> u32 {
    1
}

/// Replay (design/12): fresh world + spawns from JSON, intents from a
/// server-saved JSONL log → snapshot hash. Drives real `server::replay`.
pub fn replay_from_files(seed: u64, spawns_json: &str, log_path: &str) -> Result<u64, String> {
    let specs: Vec<SpawnSpec> = serde_json::from_str(spawns_json).map_err(|e| e.to_string())?;
    let spawns: Vec<(EntityId, Pos, u64)> = specs
        .into_iter()
        .map(|s| {
            (
                EntityId(s.id),
                Pos {
                    city: s.city,
                    sx: s.sx,
                    sy: s.sy,
                    z: s.z,
                    x: s.x,
                    y: s.y,
                },
                s.speed,
            )
        })
        .collect();
    let intents = Server::load_log(log_path).map_err(|e| e.to_string())?;
    Ok(replay(seed, &spawns, intents).snapshot_hash())
}

/// Mapgen check (design/12): chain a 3x3 of sections through the real
/// `World::ensure_section` seam logic, then verify BFS reachability and
/// shared-edge port agreement. Returns failure lines; empty = clean.
pub fn mapgen_failures(seed: u64) -> Vec<String> {
    let mut w = World::new(seed);
    for sy in 0..3i32 {
        for sx in 0..3i32 {
            w.ensure_section(sx, sy);
        }
    }
    let mut out = Vec::new();
    for sy in 0..3i32 {
        for sx in 0..3i32 {
            let s = w.section_at(sx, sy).unwrap();
            if !s.reachable((8, 8)) {
                out.push(format!("seed {seed} section ({sx},{sy}) unreachable"));
            }
            if let Some(e) = w.section_at(sx + 1, sy) {
                if e.ports.w != s.ports.e {
                    out.push(format!(
                        "seed {seed} edge ({sx},{sy})E mismatch: {:?} vs {:?}",
                        s.ports.e, e.ports.w
                    ));
                }
            }
            if let Some(south) = w.section_at(sx, sy + 1) {
                if south.ports.n != s.ports.s {
                    out.push(format!(
                        "seed {seed} edge ({sx},{sy})S mismatch: {:?} vs {:?}",
                        s.ports.s, south.ports.n
                    ));
                }
            }
        }
    }
    out
}

/// Human report over `seeds` starting at `seed0`, including each seed's
/// center-section ports. Returns (text, failure count).
pub fn mapgen_report(seeds: u64, seed0: u64) -> (String, u64) {
    let mut fails = 0u64;
    let mut out = format!("mapgen seeds={seeds} seed0={seed0}\n");
    for i in 0..seeds {
        let seed = seed0 + i;
        let mut w = World::new(seed);
        w.ensure_section(0, 0);
        let ports = format!("{:?}", w.section_at(0, 0).unwrap().ports);
        let fs = mapgen_failures(seed);
        if fs.is_empty() {
            out.push_str(&format!("seed {seed} ports {ports} ok\n"));
        } else {
            fails += fs.len() as u64;
            for f in fs {
                out.push_str(&format!("FAIL {f}\n"));
            }
        }
    }
    out.push_str(&format!("failures {fails}\n"));
    (out, fails)
}

/// Ledger audit (design/12): recompute from the log. Loads a world save and
/// checks live == audit − outstanding IOUs. Returns (ok, live, audit, ious).
pub fn audit_world_ledger(world_json: &str) -> Result<(bool, i64, i64, i64), String> {
    let w = World::load_json(world_json).map_err(|e| e.to_string())?;
    let live = w.ledger.live_sum();
    let audit = w.ledger.audit_sum();
    let iou: i64 = w.ledger.ious().iter().map(|(_, a)| a).sum();
    Ok((live == audit - iou, live, audit, iou))
}

/// A demo-shaped world for fuzzing: shops, a rented home, a defended rig,
/// a tri-net with power, a veil, and two NPCs. Mirrors the shell's world so
/// fuzz acts exercise the same systems.
pub fn fuzz_world(seed: u64) -> World {
    let mut w = World::new(seed);
    w.spawn(
        EntityId(1),
        Pos {
            city: 1,
            sx: 0,
            sy: 0,
            z: 0,
            x: 8,
            y: 8,
        },
        10,
    );
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
        .ok();
    w.net
        .add_link(HostId(2), HostId(3), LinkKind::Laser, 40, 10)
        .ok();
    let range = w.ensure_host(HostId(1));
    range.set_owner(Some(Host::WARDEN));
    range.set_monitor(true);
    w.veil.add_area(1, "lobby", vec![2], false);
    w.veil.add_area(2, "ops", vec![1, 3], false);
    w.veil.add_area(3, "roof", vec![2], true);
    w.spawn_npc(
        EntityId(10),
        sim::Role::Shopkeeper,
        (3, 3),
        (12, 4),
        (8, 12),
        20,
    );
    w.spawn_npc(EntityId(11), sim::Role::Cop, (4, 12), (12, 12), (8, 8), 15);
    let buyer = w.open_account(EntityId(1), HostId(1)).unwrap();
    w.ledger.mint(buyer, 5000).unwrap();
    let till = w.open_account(EntityId(10), HostId(1)).unwrap();
    let mut shop = sim::Shop::new(till);
    shop.add_line("ration", 100, 5, 2, 5, 200);
    shop.add_line("meds", 400, 2, 1, 3, 900);
    shop.set_hours(8, 20);
    w.place_shop(10, 8, shop);
    w.set_home(Pos {
        city: 1,
        sx: 0,
        sy: 0,
        z: 0,
        x: 8,
        y: 8,
    });
    w
}

/// One generated act from the full verb surface (deterministic per rng).
fn fuzz_verb(rng: &mut Rng) -> Verb {
    let dirs = [Dir::N, Dir::S, Dir::E, Dir::W];
    match rng.below(148) {
        0..=24 => Verb::Move(dirs[rng.below(4) as usize]),
        25..=29 => Verb::Wait,
        30..=39 => Verb::Eat,
        40..=54 => Verb::Work(rng.below(5) as u8),
        55..=59 => Verb::Watch,
        60..=69 => Verb::Buy(["ration", "meds", "firewall"][rng.below(3) as usize].to_string()),
        70..=72 => Verb::Sleep,
        73..=78 => Verb::Deposit((rng.below(300) + 1) as i64),
        79..=84 => Verb::Withdraw((rng.below(300) + 1) as i64),
        85..=88 => Verb::DeckLoad {
            x: rng.below(12) as u8,
            y: rng.below(8) as u8,
        },
        89..=92 => Verb::DeckStrike {
            x: rng.below(12) as u8,
            y: rng.below(8) as u8,
        },
        93..=95 => Verb::DamageWire(HostId(1), HostId(2)),
        96..=97 => Verb::VeilEnter {
            mp: 5 + rng.below(8) as u8,
        },
        98 => Verb::VeilMove {
            area: 1 + rng.below(3) as u64,
        },
        99 => Verb::VeilEject,
        100..=103 => Verb::SetPower {
            sx: rng.below(3) as i32,
            sy: 0,
            up: rng.below(2) == 0,
        },
        104..=111 => Verb::Code {
            res: sim::Res::from_index(rng.below(3) as u8),
        },
        112..=117 => Verb::DeckFarm {
            x: rng.below(12) as u8,
            y: rng.below(8) as u8,
            res: sim::Res::from_index(rng.below(3) as u8),
        },
        118..=129 => Verb::DeckCard {
            card: Host::implemented_cards()[rng.below(20) as usize].to_string(),
            x: rng.below(12) as u8,
            y: rng.below(8) as u8,
        },
        130..=132 => Verb::SetFirewall {
            host: HostId(1 + rng.below(3) as u64),
            source: HostId(1 + rng.below(3) as u64),
            on: rng.below(2) == 0,
        },
        133..=136 => Verb::Exploit {
            vuln: [
                "cve-2006-0001",
                "cve-2006-0002",
                "cve-2006-0004",
                "cve-2006-0005",
            ][rng.below(4) as usize]
                .to_string(),
        },
        137..=140 => Verb::SendTrojan {
            host: HostId(1 + rng.below(2) as u64),
        },
        141 => Verb::PayFine,
        142..=146 => Verb::DeckMod {
            x: rng.below(12) as u8,
            y: rng.below(8) as u8,
            card: ["tank", "plating", "reflect", "splash", "extra"][rng.below(5) as usize]
                .to_string(),
        },
        _ => Verb::ScrubLog,
    }
}

/// Invariant fuzz: drive the demo world with generated acts and check the
/// engine's invariants after every one. Returns (applied, refused); an Err
/// names the first violation with its seed and step so it replays.
pub fn fuzz_invariants(seed: u64, acts: usize) -> Result<(usize, usize), String> {
    let mut rng = Rng::new(seed);
    let mut w = fuzz_world(seed);
    let (mut applied, mut refused) = (0usize, 0usize);
    let mut last_tick = 0u64;
    for step in 0..acts {
        let due = w.next_at(EntityId(1));
        if w.tick < due {
            w.tick = due;
        }
        let verb = fuzz_verb(&mut rng);
        let label = format!("{verb:?}");
        let pocket = w.needs(EntityId(1)).map(|n| n.2).unwrap_or(0);
        let bank = w.bank_balance(EntityId(1));
        match w.apply(&Intent {
            actor: EntityId(1),
            verb,
        }) {
            Ok(_) => {
                applied += 1;
                if label.starts_with("Buy") {
                    let after = w.needs(EntityId(1)).map(|n| n.2).unwrap_or(0);
                    let bank_after = w.bank_balance(EntityId(1)).unwrap_or(0);
                    let spent = (pocket - after) + (bank.unwrap_or(0) - bank_after);
                    if spent <= 0 {
                        return Err(format!(
                            "seed={seed} step={step}: successful {label} drew nothing"
                        ));
                    }
                }
            }
            Err(_) => refused += 1,
        }
        if w.ledger.live_sum() != w.ledger.audit_sum() {
            return Err(format!(
                "seed={seed} step={step}: ledger drift after {label}"
            ));
        }
        if w.tick < last_tick {
            return Err(format!(
                "seed={seed} step={step}: tick went backwards after {label}"
            ));
        }
        last_tick = w.tick;
        match w.pos_of(EntityId(1)) {
            Some(p) if p.x < 16 && p.y < 16 => {}
            other => {
                return Err(format!(
                    "seed={seed} step={step}: actor {other:?} after {label}"
                ))
            }
        }
        let (food, mood, _, _) = w.needs(EntityId(1)).unwrap();
        if food > 100 || mood > 100 {
            return Err(format!(
                "seed={seed} step={step}: needs over cap after {label}"
            ));
        }
        if w.next_at(EntityId(1)) < w.tick {
            return Err(format!(
                "seed={seed} step={step}: next_at behind tick after {label}"
            ));
        }
        if let Some(h) = w.host(HostId(1)) {
            if h.progs.len() > (h.w as usize) * (h.h as usize) {
                return Err(format!(
                    "seed={seed} step={step}: host overcrowded after {label}"
                ));
            }
            for p in h.progs.values() {
                if p.head.0 >= h.w || p.head.1 >= h.h {
                    return Err(format!(
                        "seed={seed} step={step}: prog out of arena after {label}"
                    ));
                }
            }
        }
        for (id, np, _) in w.npc_full() {
            if np.x >= 16 || np.y >= 16 {
                return Err(format!(
                    "seed={seed} step={step}: npc {id} out of bounds after {label}"
                ));
            }
        }
        if w.veil.controlled(EntityId(1)).is_some() && !w.veil.present(EntityId(1)) {
            return Err(format!(
                "seed={seed} step={step}: veil bookkeeping after {label}"
            ));
        }
        for res in sim::Res::ALL {
            if w.units(EntityId(1), res) > 100_000 {
                return Err(format!(
                    "seed={seed} step={step}: {} stock runaway",
                    res.name()
                ));
            }
        }
    }
    Ok((applied, refused))
}

/// Determinism fuzz: apply generated acts through the real server path, then
/// replay the accepted-intent log on a twin built identically and require an
/// identical snapshot hash. Also asserts every processed intent is logged
/// exactly once. Returns (applied, refused).
pub fn replay_fuzz(seed: u64, acts: usize) -> Result<(usize, usize), String> {
    let mut rng = Rng::new(seed);
    let mut s = Server::new(seed);
    s.world = fuzz_world(seed);
    let (mut applied, mut refused) = (0usize, 0usize);
    for step in 0..acts {
        let verb = fuzz_verb(&mut rng);
        let label = format!("{verb:?}");
        let before = s.log.len();
        match s.apply_one(Intent {
            actor: EntityId(1),
            verb,
        }) {
            Ok(_) => applied += 1,
            Err(_) => refused += 1,
        }
        if s.log.len() != before + 1 {
            return Err(format!(
                "seed={seed} step={step}: {label} not logged exactly once"
            ));
        }
        if s.world.ledger.live_sum() != s.world.ledger.audit_sum() {
            return Err(format!(
                "seed={seed} step={step}: ledger drift after {label}"
            ));
        }
    }
    let hash = s.world.snapshot_hash();
    let mut t = Server::new(seed);
    t.world = fuzz_world(seed);
    for i in s.log.clone() {
        t.submit(i);
    }
    t.step();
    if t.world.snapshot_hash() != hash {
        return Err(format!(
            "seed={seed}: replay hash {:016x} != {hash:016x}",
            t.world.snapshot_hash()
        ));
    }
    Ok((applied, refused))
}

/// Sweep both fuzzers over seeds `1..=seeds`. Returns the failing-seed
/// reports (empty when every seed passes).
pub fn fuzz_sweep(seeds: u64, acts: usize) -> Vec<String> {
    let mut fails = Vec::new();
    for seed in 1..=seeds {
        if let Err(e) = fuzz_invariants(seed, acts) {
            fails.push(format!("invariants {e}"));
        }
        if let Err(e) = replay_fuzz(seed, acts) {
            fails.push(format!("replay {e}"));
        }
    }
    fails
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ttk_matches_golden() {
        let golden = include_str!("../../data/golden/deck_ttk.csv");
        assert_eq!(ttk_csv(), golden);
    }

    #[test]
    fn trace_matches_golden() {
        let golden = include_str!("../../data/golden/trace_curve.csv");
        assert_eq!(trace_csv(), golden);
    }

    #[test]
    fn gen_100_all_reachable() {
        assert_eq!(gen_stats(100, 0), (100, 100));
    }

    #[test]
    fn gen_report_matches_golden() {
        let golden = include_str!("../../data/golden/gen_100.json");
        assert_eq!(gen_report(100, 0), golden);
    }

    #[test]
    fn soak_deterministic() {
        assert_eq!(soak(7, 20, 500), soak(7, 20, 500));
        assert_ne!(soak(7, 20, 500), soak(8, 20, 500));
    }

    #[test]
    fn fuzzers_pass_on_their_seeds_and_fail_loudly() {
        // A short sweep must be clean, and the report shape is stable.
        assert_eq!(fuzz_sweep(3, 500), Vec::<String>::new());
        let (applied, refused) = fuzz_invariants(1, 500).unwrap();
        assert!(
            applied > 0 && refused > 0,
            "applied={applied} refused={refused}"
        );
        let (applied, _) = replay_fuzz(1, 500).unwrap();
        assert!(applied > 0);
        // Deterministic per seed.
        assert_eq!(
            fuzz_invariants(7, 300).unwrap(),
            fuzz_invariants(7, 300).unwrap()
        );
        assert_eq!(replay_fuzz(7, 300).unwrap(), replay_fuzz(7, 300).unwrap());
    }

    #[test]
    fn soak_gate_budget_deterministic() {
        // design/12 gate budget: 100 bots x 10k ticks completes + replays equal.
        assert_eq!(soak(7, 100, 10000), soak(7, 100, 10000));
    }

    #[test]
    fn chaos_soak_deterministic() {
        assert_eq!(chaos_soak(7, 30, 2000), chaos_soak(7, 30, 2000));
        assert_ne!(chaos_soak(7, 30, 2000), chaos_soak(8, 30, 2000));
    }

    fn unique_tmp(name: &str) -> std::path::PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        std::env::temp_dir().join(format!("fidom-tools-{name}-{}-{nanos}", std::process::id()))
    }

    #[test]
    fn replay_cli_path_reproduces_server_hash() {
        use sim::{Dir, Verb};
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
        s.submit(Intent {
            actor: EntityId(1),
            verb: Verb::Move(Dir::S),
        });
        s.step();
        let expect = s.world.snapshot_hash();
        let dir = unique_tmp("replay");
        std::fs::create_dir_all(&dir).unwrap();
        let log = dir.join("log.jsonl");
        s.save_log(log.to_str().unwrap()).unwrap();
        let spec = r#"[{"id":1,"x":8,"y":8,"speed":1}]"#;
        let got = replay_from_files(9, spec, log.to_str().unwrap()).unwrap();
        assert_eq!(got, expect);
        assert!(replay_from_files(9, "not json", log.to_str().unwrap()).is_err());
        assert!(replay_from_files(9, spec, dir.join("missing.jsonl").to_str().unwrap()).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn mapgen_50_seeds_clean() {
        for i in 0..50u64 {
            assert!(mapgen_failures(i).is_empty(), "seed {i}");
        }
        let (report, fails) = mapgen_report(50, 0);
        assert_eq!(fails, 0);
        assert!(report.contains("failures 0"));
    }

    #[test]
    fn ledger_audit_accepts_funded_and_iou_worlds() {
        use sim::HostId;
        let mut w = World::new(3);
        w.spawn(
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
        w.spawn(
            EntityId(2),
            Pos {
                city: 1,
                sx: 0,
                sy: 0,
                z: 0,
                x: 9,
                y: 8,
            },
            1,
        );
        let a = w.open_account(EntityId(1), HostId(1)).unwrap();
        let b = w.open_account(EntityId(2), HostId(2)).unwrap();
        w.ledger.mint(a, 1000).unwrap();
        w.ledger.transfer(a, b, 300).unwrap();
        let (ok, live, audit, iou) = audit_world_ledger(&w.save_json()).unwrap();
        assert!(ok);
        assert_eq!((live, audit, iou), (1000, 1000, 0));
        // Branch-down withdrawal: live drops by the IOU, audit holds.
        w.ledger.withdraw_atm(a, 90, false, 100).unwrap();
        let (ok, live, audit, iou) = audit_world_ledger(&w.save_json()).unwrap();
        assert!(ok);
        assert_eq!((live, audit, iou), (910, 1000, 90));
        assert!(audit_world_ledger("{bad").is_err());
    }
}

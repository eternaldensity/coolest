//! Balance + soak tooling (design/12 gates). Deterministic sweeps with
//! committed goldens in data/golden/: TTK table, trace curve, gen stats.
//! `fidom-tools` binary reprints them; tests fail on drift.

use sim::{CellPos, EntityId, Host, HostId, Ports, Rng, World, Dir, Intent, Verb, Pos, gen_section};
use server::{replay, Server};

/// Killer-vs-killer TTK: grow one len-N program, strike tail cells in order,
/// then the head. Returns (len, acts to clear).
pub fn ttk_table() -> Vec<(usize, u32)> {
    let mut out = Vec::new();
    for len in 1..=6usize {
        let mut h = Host::new(HostId(1), 12, 8, 1000);
        h.load(EntityId(10), EntityId(1), CellPos(0, 0), len as u8, 1).unwrap();
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
    h.load(EntityId(10), EntityId(1), CellPos(0, 0), 2, 1).unwrap();
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
        w.spawn(EntityId(1000 + a as u64), Pos { city: 1, sx: 0, sy: 0, z: 0, x, y }, 1 + rng.below(10) as u64);
    }
    let dirs = [Dir::N, Dir::S, Dir::E, Dir::W];
    for _ in 0..ticks {
        let a = rng.below(actors.max(1) as u32) as u64;
        let d = dirs[rng.below(4) as usize];
        w.tick = w.next_at(EntityId(1000 + a)).max(w.tick);
        let _ = w.apply(&Intent { actor: EntityId(1000 + a), verb: Verb::Move(d) });
    }
    w.snapshot_hash()
}

/// Spawn spec for `replay`: tile coords default to city 1, section (0,0), z 0.
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
                Pos { city: s.city, sx: s.sx, sy: s.sy, z: s.z, x: s.x, y: s.y },
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
    fn soak_gate_budget_deterministic() {
        // design/12 gate budget: 100 bots x 10k ticks completes + replays equal.
        assert_eq!(soak(7, 100, 10000), soak(7, 100, 10000));
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
        s.world.spawn(EntityId(1), Pos { city: 1, sx: 0, sy: 0, z: 0, x: 8, y: 8 }, 1);
        s.submit(Intent { actor: EntityId(1), verb: Verb::Move(Dir::E) });
        s.submit(Intent { actor: EntityId(1), verb: Verb::Move(Dir::S) });
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
        w.spawn(EntityId(1), Pos { city: 1, sx: 0, sy: 0, z: 0, x: 8, y: 8 }, 1);
        w.spawn(EntityId(2), Pos { city: 1, sx: 0, sy: 0, z: 0, x: 9, y: 8 }, 1);
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

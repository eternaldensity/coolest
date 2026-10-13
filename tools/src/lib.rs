//! Balance + soak tooling (design/12 gates). Deterministic sweeps with
//! committed goldens in data/golden/: TTK table, trace curve, gen stats.
//! `fidom-tools` binary reprints them; tests fail on drift.

use sim::{CellPos, EntityId, Host, HostId, Ports, Rng, World, Dir, Intent, Verb, Pos, gen_section};

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
    fn soak_deterministic() {
        assert_eq!(soak(7, 20, 500), soak(7, 20, 500));
        assert_ne!(soak(7, 20, 500), soak(8, 20, 500));
    }
}

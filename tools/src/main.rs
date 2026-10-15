//! fidom-tools: balance CSVs, gen stats, soak runs (design/12).
//! Usage: fidom-tools balance | gen [n] [seed0] | gen-report [n] [seed0] | soak [actors] [ticks] [seed]
//!        fidom-tools chaos [actors] [ticks] [seed] | replay <seed> <spawns.json> <log.jsonl>
//!        fidom-tools mapgen-check [n] [seed0] | ledger-audit <world-save.json>
//!        fidom-tools fuzz [seed] [acts] | replay-fuzz [seed] [acts] | fuzz-sweep [seeds] [acts]

use std::time::Instant;
use tools::{
    audit_world_ledger, chaos_soak, fuzz_invariants, fuzz_sweep, gen_report, gen_stats,
    mapgen_report, replay_from_files, replay_fuzz, soak, trace_csv, ttk_csv,
};

fn fail(msg: String) -> ! {
    eprintln!("{msg}");
    std::process::exit(1);
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(|s| s.as_str()) {
        Some("gen") => {
            let n: u64 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(100);
            let seed0: u64 = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(0);
            let (ok, total) = gen_stats(n, seed0);
            println!("reachable {ok}/{total}");
        }
        Some("gen-report") => {
            let n: u64 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(100);
            let seed0: u64 = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(0);
            print!("{}", gen_report(n, seed0));
        }
        Some("soak") => {
            let actors: usize = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(100);
            let ticks: u64 = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(10000);
            let seed: u64 = args.get(4).and_then(|s| s.parse().ok()).unwrap_or(7);
            let t = Instant::now();
            let hash = soak(seed, actors, ticks);
            println!(
                "soak actors={actors} ticks={ticks} hash={hash:016x} elapsed_ms={}",
                t.elapsed().as_millis()
            );
        }
        Some("chaos") => {
            let actors: usize = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(100);
            let ticks: u64 = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(100000);
            let seed: u64 = args.get(4).and_then(|s| s.parse().ok()).unwrap_or(7);
            let t = Instant::now();
            let hash = chaos_soak(seed, actors, ticks);
            println!(
                "chaos actors={actors} ticks={ticks} hash={hash:016x} elapsed_ms={}",
                t.elapsed().as_millis()
            );
        }
        Some("replay") => {
            let seed: u64 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(0);
            let (spec_path, log_path) = match (args.get(3), args.get(4)) {
                (Some(a), Some(b)) => (a, b),
                _ => fail("usage: fidom-tools replay <seed> <spawns.json> <log.jsonl>".to_string()),
            };
            let spec = std::fs::read_to_string(spec_path)
                .unwrap_or_else(|e| fail(format!("replay failed: {e}")));
            match replay_from_files(seed, &spec, log_path) {
                Ok(hash) => println!("replay seed={seed} hash={hash:016x}"),
                Err(e) => fail(format!("replay failed: {e}")),
            }
        }
        Some("fuzz") => {
            let seed: u64 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(1);
            let acts: usize = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(20_000);
            let t = Instant::now();
            match fuzz_invariants(seed, acts) {
                Ok((applied, refused)) => println!(
                    "fuzz seed={seed} acts={acts} applied={applied} refused={refused} elapsed_ms={}",
                    t.elapsed().as_millis()
                ),
                Err(e) => fail(format!("fuzz FAIL {e}")),
            }
        }
        Some("replay-fuzz") => {
            let seed: u64 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(1);
            let acts: usize = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(5_000);
            let t = Instant::now();
            match replay_fuzz(seed, acts) {
                Ok((applied, refused)) => println!(
                    "replay-fuzz seed={seed} acts={acts} applied={applied} refused={refused} elapsed_ms={}",
                    t.elapsed().as_millis()
                ),
                Err(e) => fail(format!("replay-fuzz FAIL {e}")),
            }
        }
        Some("fuzz-sweep") => {
            let seeds: u64 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(25);
            let acts: usize = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(20_000);
            let t = Instant::now();
            let fails = fuzz_sweep(seeds, acts);
            if fails.is_empty() {
                println!(
                    "fuzz-sweep seeds={seeds} acts={acts} all clean elapsed_ms={}",
                    t.elapsed().as_millis()
                );
            } else {
                for f in &fails {
                    eprintln!("{f}");
                }
                fail(format!(
                    "fuzz-sweep {}/{} seeds failed",
                    fails.len(),
                    seeds * 2
                ));
            }
        }
        Some("mapgen-check") => {
            let n: u64 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(50);
            let seed0: u64 = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(0);
            let (report, fails) = mapgen_report(n, seed0);
            print!("{report}");
            if fails > 0 {
                std::process::exit(1);
            }
        }
        Some("ledger-audit") => {
            let path = match args.get(2) {
                Some(p) => p,
                None => fail("usage: fidom-tools ledger-audit <world-save.json>".to_string()),
            };
            let json = std::fs::read_to_string(path)
                .unwrap_or_else(|e| fail(format!("ledger-audit failed: {e}")));
            match audit_world_ledger(&json) {
                Ok((ok, live, audit, iou)) => {
                    println!("audit_ok={ok} live={live} audit={audit} ious={iou}");
                    if !ok {
                        std::process::exit(1);
                    }
                }
                Err(e) => fail(format!("ledger-audit failed: {e}")),
            }
        }
        _ => {
            print!("{}", ttk_csv());
            print!("{}", trace_csv());
        }
    }
}

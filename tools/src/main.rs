//! fidom-tools: balance CSVs, gen stats, soak runs (design/12).
//! Usage: fidom-tools balance | gen [n] [seed0] | soak [actors] [ticks] [seed]

use std::time::Instant;
use tools::{gen_stats, soak, trace_csv, ttk_csv};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(|s| s.as_str()) {
        Some("gen") => {
            let n: u64 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(100);
            let seed0: u64 = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(0);
            let (ok, total) = gen_stats(n, seed0);
            println!("reachable {ok}/{total}");
        }
        Some("soak") => {
            let actors: usize = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(100);
            let ticks: u64 = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(10000);
            let seed: u64 = args.get(4).and_then(|s| s.parse().ok()).unwrap_or(7);
            let t = Instant::now();
            let hash = soak(seed, actors, ticks);
            println!("soak actors={actors} ticks={ticks} hash={hash:016x} elapsed_ms={}", t.elapsed().as_millis());
        }
        _ => {
            print!("{}", ttk_csv());
            print!("{}", trace_csv());
        }
    }
}

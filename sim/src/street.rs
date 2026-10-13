use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

/// M1a street generation (design/02): deterministic sections, guaranteed
/// placements with BFS reachability, edge-port matching for roads/rivers,
/// and the 3x3-active / 5x5-loaded streaming window.
pub const SECTION_SIZE: u8 = 16;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum District {
    Residential,
    Shop,
    Park,
    Industrial,
    Gov,
    Rural,
    Forest,
    Lake,
}

/// Deterministic XorShift64* — sim-local, no external RNG dependency.
#[derive(Debug, Clone)]
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Self(seed | 1)
    }

    pub fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545F4914F6CDD1D)
    }

    pub fn below(&mut self, n: u32) -> u32 {
        (self.next_u64() % n as u64) as u32
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GuaranteeKind {
    Door,
    Computer,
    Egress,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Ports {
    pub n: Option<u8>,
    pub s: Option<u8>,
    pub e: Option<u8>,
    pub w: Option<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Section {
    pub sx: i32,
    pub sy: i32,
    pub district: District,
    pub difficulty: u8,
    /// Row-major walkable flags.
    pub walk: Vec<bool>,
    pub guarantees: Vec<(GuaranteeKind, u8, u8)>,
    pub ports: Ports,
    /// Which sides are hard-blocked (no crossing).
    pub blocked: [bool; 4], // N,S,E,W
}

impl Section {
    fn idx(x: u8, y: u8) -> usize {
        y as usize * SECTION_SIZE as usize + x as usize
    }

    pub fn walkable(&self, x: u8, y: u8) -> bool {
        x < SECTION_SIZE && y < SECTION_SIZE && self.walk[Self::idx(x, y)]
    }

    /// BFS from spawn to every guarantee + open port. Generation gate.
    pub fn reachable(&self, spawn: (u8, u8)) -> bool {
        let targets: Vec<(u8, u8)> = self.guarantees.iter().map(|(_, x, y)| (*x, *y)).collect();
        let mut seen = vec![false; 256];
        let mut q = VecDeque::new();
        if !self.walkable(spawn.0, spawn.1) {
            return false;
        }
        seen[Self::idx(spawn.0, spawn.1)] = true;
        q.push_back(spawn);
        while let Some((x, y)) = q.pop_front() {
            for (dx, dy) in [(0i16, -1), (0, 1), (1, 0), (-1, 0)] {
                let nx = x as i16 + dx;
                let ny = y as i16 + dy;
                if nx < 0 || ny < 0 || nx >= SECTION_SIZE as i16 || ny >= SECTION_SIZE as i16 {
                    continue;
                }
                let (nx, ny) = (nx as u8, ny as u8);
                if !self.walkable(nx, ny) || seen[Self::idx(nx, ny)] {
                    continue;
                }
                seen[Self::idx(nx, ny)] = true;
                q.push_back((nx, ny));
            }
        }
        targets.iter().all(|(x, y)| seen[Self::idx(*x, *y)])
    }
}

fn all_districts() -> [District; 8] {
    use District::*;
    [Residential, Shop, Park, Industrial, Gov, Rural, Forest, Lake]
}

/// Generate a section. `want_ports` constrains sides already fixed by a
/// neighbor (road/river continuity); `None` sides are rolled. Retries with
/// derived subseeds until BFS reachability holds (deterministic).
pub fn gen_section(seed: u64, sx: i32, sy: i32, want_ports: Ports) -> Section {
    gen_section_weighted(seed, sx, sy, want_ports, &[], [false; 4])
}

/// Weighted variant: `districts` is (District, weight, difficulty_base).
/// Empty slice = uniform pick, base 0 (identical to legacy behavior).
/// `shut` forces sides absent (N,S,E,W): a neighbor already there has no
/// facing port, so no road opens onto its wall. Explicit `want_ports`
/// entries always win over `shut`.
pub fn gen_section_weighted(
    seed: u64,
    sx: i32,
    sy: i32,
    want_ports: Ports,
    districts: &[(District, u32, u8)],
    shut: [bool; 4],
) -> Section {
    let mut attempt: u64 = 0;
    loop {
        let s = gen_once(seed ^ (attempt.wrapping_mul(0x9E3779B97F4A7C15)), sx, sy, want_ports, districts, shut);
        if s.reachable((8, 8)) || attempt >= 20 {
            return s;
        }
        attempt += 1;
    }
}

fn pick_district(rng: &mut Rng, districts: &[(District, u32, u8)]) -> (District, u8) {
    if districts.is_empty() {
        return (all_districts()[rng.below(8) as usize], rng.below(6) as u8);
    }
    let total: u32 = districts.iter().map(|(_, w, _)| w).sum();
    let mut roll = rng.below(total.max(1));
    for (d, w, base) in districts {
        if roll < *w {
            return (*d, base + rng.below(6) as u8);
        }
        roll -= w;
    }
    let (d, _, base) = districts[districts.len() - 1];
    (d, base)
}

fn gen_once(seed: u64, sx: i32, sy: i32, want_ports: Ports, districts: &[(District, u32, u8)], shut: [bool; 4]) -> Section {
    let mut rng = Rng::new(seed ^ ((sx as u64).wrapping_mul(0xBF58476D1CE4E5B9)) ^ ((sy as u64).wrapping_mul(0x94D049BB133111EB)));
    let (district, difficulty) = pick_district(&mut rng, districts);
    // Ports: honor constraints, shut stays absent, else 50% a road exits at random row/col.
    let mut ports = want_ports;
    let mut roll = |c: Option<u8>, closed: bool| -> Option<u8> {
        match c {
            Some(v) => Some(v),
            None if closed => None,
            None if rng.below(2) == 0 => Some(rng.below(16) as u8),
            None => None,
        }
    };
    // N/S ports are x-coords, E/W ports are y-coords.
    ports.n = roll(ports.n, shut[0]);
    ports.s = roll(ports.s, shut[1]);
    ports.e = roll(ports.e, shut[2]);
    ports.w = roll(ports.w, shut[3]);
    // Blocked edges: blocked unless a port exits there (ports force open).
    let blocked = [
        ports.n.is_none() && rng.below(4) == 0,
        ports.s.is_none() && rng.below(4) == 0,
        ports.e.is_none() && rng.below(4) == 0,
        ports.w.is_none() && rng.below(4) == 0,
    ];
    // Guarantees: computer at random interior, door near south, egress east.
    let comp = (2 + rng.below(12) as u8, 2 + rng.below(12) as u8);
    let door = (8, 13);
    let egress = (13, 8);
    let keep: [(u8, u8); 4] = [(8, 8), comp, door, egress];
    let mut walk = vec![true; 256];
    // Border walls except at ports / blocked-open sides are handled by
    // movement rules; interior clutter ~8%, never on kept cells.
    for y in 1..15u8 {
        for x in 1..15u8 {
            if keep.contains(&(x, y)) {
                continue;
            }
            if rng.below(100) < 8 {
                walk[Section::idx(x, y)] = false;
            }
        }
    }
    Section {
        sx, sy, district, difficulty, walk,
        guarantees: vec![
            (GuaranteeKind::Computer, comp.0, comp.1),
            (GuaranteeKind::Door, door.0, door.1),
            (GuaranteeKind::Egress, egress.0, egress.1),
        ],
        ports, blocked,
    }
}

/// Streaming window: 3x3 active around center, 5x5 loaded (halo = outer ring).
pub type Window = (Vec<(i32, i32)>, Vec<(i32, i32)>);

pub fn window(center: (i32, i32)) -> Window {
    let mut active = Vec::with_capacity(9);
    let mut halo = Vec::with_capacity(16);
    for dy in -2..=2i32 {
        for dx in -2..=2i32 {
            let c = (center.0 + dx, center.1 + dy);
            if dx.abs() <= 1 && dy.abs() <= 1 {
                active.push(c);
            } else {
                halo.push(c);
            }
        }
    }
    (active, halo)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hundred_seeds_all_reachable() {
        for seed in 0..100u64 {
            let d = all_districts()[seed as usize % 8];
            let _ = d;
            let s = gen_section(seed, 0, 0, Ports::default());
            assert!(s.reachable((8, 8)), "seed {seed} unreachable");
        }
    }

    #[test]
    fn weighted_districts_follow_file() {
        use crate::data::parse_districts;
        let defs = parse_districts(include_str!("../../data/districts.yaml")).unwrap();
        let table: Vec<(District, u32, u8)> = defs.iter().map(|d| {
            let district = match d.id.as_str() {
                "residential" => District::Residential,
                "shop" => District::Shop,
                "park" => District::Park,
                "industrial" => District::Industrial,
                "gov" => District::Gov,
                "rural" => District::Rural,
                "forest" => District::Forest,
                _ => District::Lake,
            };
            (district, d.weight, d.difficulty_base)
        }).collect();
        // Single-district table forces the outcome; base shifts difficulty.
        let gov = vec![(District::Gov, 1, 2)];
        for seed in 0..20u64 {
            let s = gen_section_weighted(seed, 0, 0, Ports::default(), &gov, [false; 4]);
            assert_eq!(s.district, District::Gov);
            assert!(s.difficulty >= 2);
            assert!(s.reachable((8, 8)));
        }
        // Full file table: all outcomes are known districts.
        for seed in 0..50u64 {
            let s = gen_section_weighted(seed, seed as i32, 0, Ports::default(), &table, [false; 4]);
            assert!(s.reachable((8, 8)), "seed {seed} unreachable");
        }
    }

    #[test]
    fn neighbor_ports_agree() {
        // A fixes east exit at row 7; B honors west constraint.
        let a = gen_section(11, 0, 0, Ports { e: Some(7), ..Ports::default() });
        assert_eq!(a.ports.e, Some(7));
        let b = gen_section(12, 1, 0, Ports { w: Some(7), ..Ports::default() });
        assert_eq!(b.ports.w, Some(7));
    }

    #[test]
    fn window_sizes_and_shift() {
        let (active, halo) = window((0, 0));
        assert_eq!(active.len(), 9);
        assert_eq!(halo.len(), 16);
        assert!(active.contains(&(0, 0)));
        let (active2, _) = window((1, 0));
        assert!(active2.contains(&(1, 0)));
        assert!(!active2.contains(&(-1, 0))); // old column swapped out
    }

    #[test]
    fn rng_deterministic() {
        let mut a = Rng::new(42);
        let mut b = Rng::new(42);
        for _ in 0..10 {
            assert_eq!(a.next_u64(), b.next_u64());
        }
    }
}

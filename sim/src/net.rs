use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, BinaryHeap};
use std::cmp::Reverse;
use crate::error::SimError;
use crate::ids::HostId;

/// Which network a host lives on (2006 dual-net plot).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum NetKind {
    Oldnet,  // wired, decaying, illegal/personal
    Betanet, // wireless + laser backbone, regulated
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LinkKind {
    Wired { damaged: bool },
    Wireless { range_m: u32 },
    Laser, // fast, undamageable, toll per hop
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Node {
    pub id: HostId,
    pub net: NetKind,
    pub modem_mbps: u32,
    pub gateway: bool, // cross-net HW+SW
    pub up: bool,      // power kill flips this
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Link {
    pub from: HostId,
    pub to: HostId,
    pub kind: LinkKind,
    pub latency_ms: u32,
    pub toll_cents: u32,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct NetGraph {
    nodes: BTreeMap<u64, Node>,
    links: Vec<Link>,
    known: BTreeSet<u64>, // fog-of-war: revealed nodes only
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Route {
    pub path: Vec<HostId>,
    pub latency_ms: u32,
    pub toll_cents: u32,
    pub cap_mbps: u32,
    pub gateway_ok: bool,
}

impl NetGraph {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_node(&mut self, id: HostId, net: NetKind, modem_mbps: u32, gateway: bool) {
        self.nodes.insert(id.0, Node { id, net, modem_mbps, gateway, up: true });
    }

    pub fn add_link(
        &mut self,
        from: HostId,
        to: HostId,
        kind: LinkKind,
        latency_ms: u32,
        toll_cents: u32,
    ) -> Result<(), SimError> {
        if !self.nodes.contains_key(&from.0) || !self.nodes.contains_key(&to.0) {
            return Err(SimError::new("no_node", "known hosts", "missing"));
        }
        self.links.push(Link { from, to, kind, latency_ms, toll_cents });
        Ok(())
    }

    pub fn reveal(&mut self, id: HostId) {
        if self.nodes.contains_key(&id.0) {
            self.known.insert(id.0);
        }
    }

    pub fn is_known(&self, id: HostId) -> bool {
        self.known.contains(&id.0)
    }

    pub fn set_up(&mut self, id: HostId, up: bool) -> Result<(), SimError> {
        self.nodes.get_mut(&id.0)
            .map(|n| n.up = up)
            .ok_or(SimError::new("no_node", "known host", "missing"))
    }

    /// Damage a wired link only. Laser undamageable, wireless out of scope.
    pub fn damage_wire(&mut self, from: HostId, to: HostId) -> Result<(), SimError> {
        let l = self.links.iter_mut()
            .find(|l| l.from == from && l.to == to)
            .ok_or(SimError::new("no_link", "existing link", "missing"))?;
        match &mut l.kind {
            LinkKind::Wired { damaged } => {
                *damaged = true;
                Ok(())
            }
            LinkKind::Laser => Err(SimError::new("untouchable", "wired link", "laser")),
            LinkKind::Wireless { .. } => Err(SimError::new("untouchable", "wired link", "wireless")),
        }
    }

    pub fn repair_wire(&mut self, from: HostId, to: HostId) -> Result<(), SimError> {
        let l = self.links.iter_mut()
            .find(|l| l.from == from && l.to == to)
            .ok_or(SimError::new("no_link", "existing link", "missing"))?;
        match &mut l.kind {
            LinkKind::Wired { damaged } => {
                *damaged = false;
                Ok(())
            }
            _ => Err(SimError::new("not_wired", "wired link", "other kind")),
        }
    }

    fn passable(&self, link: &Link) -> bool {
        match link.kind {
            LinkKind::Wired { damaged } => !damaged,
            _ => true,
        }
    }

    /// Dijkstra on latency over up nodes + passable links. Undirected.
    pub fn route(&self, from: HostId, to: HostId) -> Result<Route, SimError> {
        if !self.nodes.contains_key(&from.0) || !self.nodes.contains_key(&to.0) {
            return Err(SimError::new("no_node", "known hosts", "missing"));
        }
        let mut dist: BTreeMap<u64, u32> = BTreeMap::new();
        let mut prev: BTreeMap<u64, u64> = BTreeMap::new();
        let mut heap: BinaryHeap<Reverse<(u32, u64)>> = BinaryHeap::new();
        dist.insert(from.0, 0);
        heap.push(Reverse((0, from.0)));
        while let Some(Reverse((d, u))) = heap.pop() {
            if u == to.0 {
                break;
            }
            if dist.get(&u).copied().unwrap_or(u32::MAX) < d {
                continue;
            }
            // Down transit nodes are skipped (endpoints must be up too).
            if !self.nodes[&u].up {
                continue;
            }
            for l in &self.links {
                if !self.passable(l) {
                    continue;
                }
                let v = if l.from.0 == u { l.to.0 } else if l.to.0 == u { l.from.0 } else { continue };
                if !self.nodes.get(&v).map(|n| n.up).unwrap_or(false) {
                    continue;
                }
                let nd = d.saturating_add(l.latency_ms);
                if nd < dist.get(&v).copied().unwrap_or(u32::MAX) {
                    dist.insert(v, nd);
                    prev.insert(v, u);
                    heap.push(Reverse((nd, v)));
                }
            }
        }
        if !dist.contains_key(&to.0) {
            return Err(SimError::new("no_route", "connected path", "partitioned"));
        }
        // Reconstruct.
        let mut path = vec![to];
        let mut cur = to.0;
        while cur != from.0 {
            cur = *prev.get(&cur).ok_or(SimError::new("no_route", "connected path", "gap"))?;
            path.push(HostId(cur));
        }
        path.reverse();
        // Toll, cap, gateway over the chosen path.
        let mut toll = 0u32;
        for w in path.windows(2) {
            let l = self.links.iter().find(|l|
                (l.from == w[0] && l.to == w[1]) || (l.from == w[1] && l.to == w[0])).unwrap();
            toll = toll.saturating_add(l.toll_cents);
        }
        let cap = path.iter()
            .map(|h| self.nodes[&h.0].modem_mbps)
            .min().unwrap_or(0);
        let nets: BTreeSet<NetKind> = path.iter()
            .map(|h| self.nodes[&h.0].net).collect();
        let gateway_ok = nets.len() < 2
            || path.iter().any(|h| self.nodes[&h.0].gateway);
        Ok(Route { path, latency_ms: dist[&to.0], toll_cents: toll, cap_mbps: cap, gateway_ok })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tri() -> NetGraph {
        // A(old,10) -5ms- B(old,100) -5ms- C(bet,50, Laser 40ms toll 10)
        // + direct A-C wired 100ms.
        let mut g = NetGraph::new();
        g.add_node(HostId(1), NetKind::Oldnet, 10, false);
        g.add_node(HostId(2), NetKind::Oldnet, 100, true);
        g.add_node(HostId(3), NetKind::Betanet, 50, false);
        g.add_link(HostId(1), HostId(2), LinkKind::Wired { damaged: false }, 5, 0).unwrap();
        g.add_link(HostId(2), HostId(3), LinkKind::Laser, 40, 10).unwrap();
        g.add_link(HostId(1), HostId(3), LinkKind::Wired { damaged: false }, 100, 0).unwrap();
        g
    }

    #[test]
    fn picks_lowest_latency_and_reports_cap_toll() {
        let g = tri();
        let r = g.route(HostId(1), HostId(3)).unwrap();
        assert_eq!(r.path, vec![HostId(1), HostId(2), HostId(3)]);
        assert_eq!(r.latency_ms, 45);
        assert_eq!(r.toll_cents, 10);
        assert_eq!(r.cap_mbps, 10); // min modem along path
        assert!(r.gateway_ok); // B is a gateway, cross-net legal
    }

    #[test]
    fn damaged_wire_and_power_force_reroute() {
        let mut g = tri();
        g.damage_wire(HostId(1), HostId(2)).unwrap();
        let r = g.route(HostId(1), HostId(3)).unwrap();
        assert_eq!(r.path, vec![HostId(1), HostId(3)]); // direct, slower
        g.repair_wire(HostId(1), HostId(2)).unwrap();
        g.set_up(HostId(2), false).unwrap(); // power kill
        let r = g.route(HostId(1), HostId(3)).unwrap();
        assert_eq!(r.path, vec![HostId(1), HostId(3)]);
        // Laser undamageable, wireless out of scope.
        assert!(g.damage_wire(HostId(2), HostId(3)).is_err());
    }

    #[test]
    fn cross_net_without_gateway_flagged() {
        let mut g = NetGraph::new();
        g.add_node(HostId(1), NetKind::Oldnet, 10, false);
        g.add_node(HostId(2), NetKind::Betanet, 10, false);
        g.add_link(HostId(1), HostId(2), LinkKind::Wireless { range_m: 500 }, 20, 0).unwrap();
        let r = g.route(HostId(1), HostId(2)).unwrap();
        assert!(!r.gateway_ok);
    }

    #[test]
    fn fog_of_war_tracks_reveal() {
        let mut g = tri();
        assert!(!g.is_known(HostId(3)));
        g.reveal(HostId(3));
        assert!(g.is_known(HostId(3)));
        assert!(g.route(HostId(9), HostId(3)).is_err()); // unknown endpoint
    }
}

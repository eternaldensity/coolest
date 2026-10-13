use crate::ids::HostId;
use crate::net::{NetGraph, Route};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// Section power grid (design/02 + 2006): each street section powers its
/// anchored hosts. Killing power drops those hosts from Net routing —
/// traffic re-routes around the dark section. Physical access still works.

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PowerGrid {
    /// Sections explicitly killed. Absent = powered.
    dark: BTreeSet<(i32, i32)>,
    anchors: BTreeMap<u64, (i32, i32)>,
}

impl PowerGrid {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn anchor(&mut self, host: HostId, section: (i32, i32)) {
        self.anchors.insert(host.0, section);
    }

    pub fn kill(&mut self, section: (i32, i32)) {
        self.dark.insert(section);
    }

    pub fn restore(&mut self, section: (i32, i32)) {
        self.dark.remove(&section);
    }

    pub fn is_up(&self, section: (i32, i32)) -> bool {
        !self.dark.contains(&section)
    }

    fn dark_hosts(&self) -> BTreeSet<u64> {
        self.anchors
            .iter()
            .filter(|(_, s)| self.dark.contains(s))
            .map(|(h, _)| *h)
            .collect()
    }

    pub fn powered_route(
        &self,
        graph: &NetGraph,
        from: HostId,
        to: HostId,
    ) -> Result<Route, crate::error::SimError> {
        graph.route_avoiding(from, to, &self.dark_hosts())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::net::{LinkKind, NetKind};

    fn grid3() -> (NetGraph, PowerGrid) {
        let mut g = NetGraph::new();
        g.add_node(HostId(1), NetKind::Oldnet, 10, false);
        g.add_node(HostId(2), NetKind::Oldnet, 100, true);
        g.add_node(HostId(3), NetKind::Betanet, 50, false);
        g.add_link(
            HostId(1),
            HostId(2),
            LinkKind::Wired { damaged: false },
            5,
            0,
        )
        .unwrap();
        g.add_link(HostId(2), HostId(3), LinkKind::Laser, 40, 10)
            .unwrap();
        g.add_link(
            HostId(1),
            HostId(3),
            LinkKind::Wired { damaged: false },
            100,
            0,
        )
        .unwrap();
        let mut p = PowerGrid::new();
        p.anchor(HostId(1), (0, 0));
        p.anchor(HostId(2), (1, 0));
        p.anchor(HostId(3), (2, 0));
        (g, p)
    }

    #[test]
    fn kill_forces_reroute_restore_heals() {
        let (g, mut p) = grid3();
        assert_eq!(
            p.powered_route(&g, HostId(1), HostId(3))
                .unwrap()
                .latency_ms,
            45
        );
        p.kill((1, 0)); // transit host 2 goes dark
        let r = p.powered_route(&g, HostId(1), HostId(3)).unwrap();
        assert_eq!(r.path, vec![HostId(1), HostId(3)]);
        assert!(!p.is_up((1, 0)));
        p.restore((1, 0));
        assert_eq!(
            p.powered_route(&g, HostId(1), HostId(3))
                .unwrap()
                .latency_ms,
            45
        );
    }

    #[test]
    fn dark_endpoint_fails() {
        let (g, mut p) = grid3();
        p.kill((2, 0)); // target host 3 dark
        assert!(p.powered_route(&g, HostId(1), HostId(3)).is_err());
    }
}

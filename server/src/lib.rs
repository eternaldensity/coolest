//! Authoritative server shell (design/08 + 11): single incoming queue,
//! tick loop, snapshots, intent log for replay. Async runtime and sockets
//! land later; the ordering and determinism rules live here from day one.

use std::collections::VecDeque;
use sim::{Delta, Event, Intent, World};

/// Server owns time. Clients send Intents; step() applies them in arrival
/// order, jumping the clock to each actor's next due tick (never rewinding).
pub struct Server {
    pub world: World,
    queue: VecDeque<Intent>,
    pub log: Vec<Intent>,
}

impl Server {
    pub fn new(seed: u64) -> Self {
        Self { world: World::new(seed), queue: VecDeque::new(), log: Vec::new() }
    }

    pub fn submit(&mut self, intent: Intent) {
        self.queue.push_back(intent);
    }

    pub fn pending(&self) -> usize {
        self.queue.len()
    }

    /// Apply everything queued. Returns one (Delta, Events) per intent.
    /// Failed intents (bad timing/actor) are dropped and NOT logged —
    /// replay feeds only applied intents, so hashes stay comparable.
    pub fn step(&mut self) -> Vec<(Delta, Vec<Event>)> {
        let mut out = Vec::with_capacity(self.queue.len());
        while let Some(intent) = self.queue.pop_front() {
            let due = self.world.next_at(intent.actor);
            if self.world.tick < due {
                self.world.tick = due;
            }
            match self.world.apply(&intent) {
                Ok(res) => {
                    self.log.push(intent);
                    out.push(res);
                }
                Err(_) => continue,
            }
        }
        out
    }

    pub fn snapshot(&self) -> String {
        self.world.save_json()
    }

    pub fn restore(&mut self, json: &str) -> Result<(), sim::SimError> {
        self.world = World::load_json(json)?;
        self.queue.clear();
        Ok(())
    }

    /// Disk persistence ( evacuate/stop/restart + file backup dump, design/08).
    pub fn save_snapshot(&self, path: &str) -> std::io::Result<()> {
        std::fs::write(path, self.snapshot())
    }

    pub fn load_snapshot(&mut self, path: &str) -> std::io::Result<()> {
        let s = std::fs::read_to_string(path)?;
        self.world = World::load_json(&s)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        self.queue.clear();
        Ok(())
    }

    pub fn save_log(&self, path: &str) -> std::io::Result<()> {
        let lines: Vec<String> = self.log.iter()
            .map(|i| serde_json::to_string(i).unwrap_or_default())
            .collect();
        std::fs::write(path, lines.join("\n"))
    }

    pub fn load_log(path: &str) -> std::io::Result<Vec<Intent>> {
        let s = std::fs::read_to_string(path)?;
        let mut out = Vec::new();
        for line in s.lines() {
            if line.trim().is_empty() {
                continue;
            }
            let i: Intent = serde_json::from_str(line)
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
            out.push(i);
        }
        Ok(out)
    }
}

/// File replay (design/12): fresh world + spawns, feed logged intents.
/// Returns the resulting world for hash comparison.
pub fn replay(
    seed: u64,
    spawns: &[(sim::EntityId, sim::Pos, u64)],
    intents: Vec<Intent>,
) -> World {
    let mut s = Server::new(seed);
    for (id, pos, speed) in spawns {
        s.world.spawn(*id, *pos, *speed);
    }
    for i in intents {
        s.submit(i);
    }
    s.step();
    s.world
}

#[cfg(test)]
mod tests {
    use super::*;
    use sim::{Dir, EntityId, Pos, Verb};

    fn pos(x: u8, y: u8) -> Pos {
        Pos { city: 1, sx: 0, sy: 0, z: 0, x, y }
    }

    fn mover(d: Dir) -> Intent {
        Intent { actor: EntityId(1), verb: Verb::Move(d) }
    }

    #[test]
    fn queue_applies_in_arrival_order() {
        let mut s = Server::new(9);
        s.world.spawn(EntityId(1), pos(8, 8), 1);
        s.submit(mover(Dir::E));
        s.submit(mover(Dir::E));
        s.submit(mover(Dir::S));
        assert_eq!(s.pending(), 3);
        let out = s.step();
        assert_eq!(out.len(), 3);
        assert_eq!(s.pending(), 0);
        assert_eq!(s.log.len(), 3);
    }

    #[test]
    fn bad_intents_dropped_not_logged() {
        let mut s = Server::new(9);
        s.world.spawn(EntityId(1), pos(8, 8), 1);
        s.submit(mover(Dir::E));
        s.submit(Intent { actor: EntityId(99), verb: Verb::Wait }); // unknown
        let out = s.step();
        assert_eq!(out.len(), 1);
        assert_eq!(s.log.len(), 1);
    }

    #[test]
    fn snapshot_restore_and_replay_match() {
        let mut s = Server::new(9);
        s.world.spawn(EntityId(1), pos(8, 8), 1);
        s.submit(mover(Dir::E));
        s.submit(mover(Dir::S));
        s.step();
        let snap = s.snapshot();
        let hash = s.world.snapshot_hash();

        let mut s2 = Server::new(9);
        s2.world.spawn(EntityId(1), pos(8, 8), 1);
        s2.restore(&snap).unwrap();
        assert_eq!(s2.world.snapshot_hash(), hash);

        // Replay the intent log on a fresh server: same hash (design/12 gate).
        let logged = s.log.clone();
        let mut s3 = Server::new(9);
        s3.world.spawn(EntityId(1), pos(8, 8), 1);
        for i in logged {
            s3.submit(i);
        }
        s3.step();
        assert_eq!(s3.world.snapshot_hash(), hash);
    }

    #[test]
    fn disk_snapshot_and_log_replay() {
        let dir = "/tmp/opencode/fidom-test";
        let _ = std::fs::create_dir_all(dir);
        let snap_path = format!("{dir}/snap.json");
        let log_path = format!("{dir}/log.jsonl");

        let mut s = Server::new(9);
        s.world.spawn(EntityId(1), pos(8, 8), 1);
        s.submit(mover(Dir::E));
        s.submit(mover(Dir::S));
        s.step();
        let hash = s.world.snapshot_hash();
        s.save_snapshot(&snap_path).unwrap();
        s.save_log(&log_path).unwrap();

        let mut s2 = Server::new(1);
        s2.load_snapshot(&snap_path).unwrap();
        assert_eq!(s2.world.snapshot_hash(), hash);

        let intents = Server::load_log(&log_path).unwrap();
        assert_eq!(intents.len(), 2);
        let w = super::replay(9, &[(EntityId(1), pos(8, 8), 1)], intents);
        assert_eq!(w.snapshot_hash(), hash);

        assert!(Server::load_log(&format!("{dir}/missing.jsonl")).is_err());
    }
}

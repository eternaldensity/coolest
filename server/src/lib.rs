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
}

use crate::ids::EntityId;
use std::cmp::Reverse;
use std::collections::BinaryHeap;

/// Energy scheduler: 1 tick = 1s. Entities act at `next_act_at`.
#[derive(Debug, Default)]
pub struct Scheduler {
    heap: BinaryHeap<Reverse<(u64, u64)>>,
}

impl Scheduler {
    pub fn new() -> Self {
        Self {
            heap: BinaryHeap::new(),
        }
    }

    pub fn schedule(&mut self, id: EntityId, at: u64) {
        self.heap.push(Reverse((at, id.0)));
    }

    pub fn pop_due(&mut self, now: u64) -> Vec<EntityId> {
        let mut out = Vec::new();
        while let Some(Reverse((at, raw))) = self.heap.peek().copied() {
            if at > now {
                break;
            }
            self.heap.pop();
            out.push(EntityId(raw));
        }
        out
    }

    pub fn peek_next(&self) -> Option<u64> {
        self.heap.peek().map(|r| r.0 .0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn order_is_time_ascending() {
        let mut s = Scheduler::new();
        s.schedule(EntityId(2), 10);
        s.schedule(EntityId(1), 5);
        s.schedule(EntityId(3), 5);
        let a = s.pop_due(5);
        assert_eq!(a.len(), 2);
        let b = s.pop_due(10);
        assert_eq!(b, vec![EntityId(2)]);
    }
}

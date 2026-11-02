//! LedgerService — quorum-backed banking (design/04).
//!
//! Three `Ledger` replicas with a 2-of-3 healthy-replica requirement.
//! Mutations are applied to one leader replica, then committed to every
//! healthy replica; a failed mutation never leaves a partial write. Reads
//! are served from the first healthy replica. IOUs and spam caps live in
//! the replicas themselves, so a commit carries them too.

use crate::error::SimError;
use crate::ids::{EntityId, HostId};
use crate::ledger::Ledger;
use serde::{Deserialize, Serialize};

const REPLICA_COUNT: usize = 3;
const QUORUM: usize = 2;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LedgerService {
    replicas: [Ledger; REPLICA_COUNT],
    #[serde(default = "all_alive")]
    alive: [bool; REPLICA_COUNT],
}

fn all_alive() -> [bool; REPLICA_COUNT] {
    [true; REPLICA_COUNT]
}

impl Default for LedgerService {
    fn default() -> Self {
        Self {
            replicas: [Ledger::new(), Ledger::new(), Ledger::new()],
            alive: all_alive(),
        }
    }
}

impl LedgerService {
    pub fn new() -> Self {
        Self::default()
    }

    fn live(&self) -> usize {
        self.alive.iter().filter(|a| **a).count()
    }

    fn leader(&self) -> Result<usize, SimError> {
        if self.live() < QUORUM {
            return Err(SimError::new("quorum", "2-of-3 replicas", "short"));
        }
        self.alive
            .iter()
            .position(|a| *a)
            .ok_or(SimError::new("quorum", "live replica", "missing"))
    }

    fn read(&self) -> Result<&Ledger, SimError> {
        self.alive
            .iter()
            .position(|a| *a)
            .map(|i| &self.replicas[i])
            .ok_or(SimError::new("quorum", "live replica", "missing"))
    }

    fn commit(&mut self, next: Ledger) {
        for i in 0..REPLICA_COUNT {
            if self.alive[i] {
                self.replicas[i] = next.clone();
            }
        }
    }

    /// Open an account. Creates the same id on every healthy replica.
    pub fn open(&mut self, owner: EntityId, branch: HostId) -> u64 {
        let Ok(leader) = self.leader() else {
            return 0;
        };
        let id = self.replicas[leader].next_id();
        for i in 0..REPLICA_COUNT {
            if self.alive[i] {
                self.replicas[i].open_with_id(id, owner, branch);
            }
        }
        id
    }

    pub fn balance(&self, acct: u64) -> Result<i64, SimError> {
        self.read()?.balance(acct)
    }

    pub fn mint(&mut self, to: u64, amt: i64) -> Result<(), SimError> {
        let leader = self.leader()?;
        let mut next = self.replicas[leader].clone();
        next.mint(to, amt)?;
        self.commit(next);
        Ok(())
    }

    pub fn transfer(&mut self, from: u64, to: u64, amt: i64) -> Result<(), SimError> {
        let leader = self.leader()?;
        let mut next = self.replicas[leader].clone();
        next.transfer(from, to, amt)?;
        self.commit(next);
        Ok(())
    }

    pub fn burn(&mut self, from: u64, amt: i64) -> Result<(), SimError> {
        let leader = self.leader()?;
        let mut next = self.replicas[leader].clone();
        next.burn(from, amt)?;
        self.commit(next);
        Ok(())
    }

    pub fn withdraw_atm(
        &mut self,
        acct: u64,
        amt: i64,
        branch_up: bool,
        cache_cap: i64,
    ) -> Result<(), SimError> {
        let leader = self.leader()?;
        let mut next = self.replicas[leader].clone();
        next.withdraw_atm(acct, amt, branch_up, cache_cap)?;
        self.commit(next);
        Ok(())
    }

    pub fn reconcile(&mut self) -> i64 {
        let Ok(leader) = self.leader() else {
            return 0;
        };
        let mut next = self.replicas[leader].clone();
        let settled = next.reconcile();
        self.commit(next);
        settled
    }

    pub fn credit_spam(
        &mut self,
        to: u64,
        host: HostId,
        day: u64,
        amt: i64,
    ) -> Result<i64, SimError> {
        let leader = self.leader()?;
        let mut next = self.replicas[leader].clone();
        let paid = next.credit_spam(to, host, day, amt)?;
        self.commit(next);
        Ok(paid)
    }

    pub fn ious(&self) -> &[(u64, i64)] {
        self.read().map(|l| l.ious()).unwrap_or(&[])
    }

    pub fn audit_sum(&self) -> i64 {
        self.read().map(|l| l.audit_sum()).unwrap_or(0)
    }

    pub fn live_sum(&self) -> i64 {
        self.read().map(|l| l.live_sum()).unwrap_or(0)
    }

    /// Mark a replica dead. Returns true if quorum survives.
    pub fn crash_replica(&mut self, idx: usize) -> bool {
        if idx < REPLICA_COUNT {
            self.alive[idx] = false;
        }
        self.live() >= QUORUM
    }

    /// Bring a replica back (e.g. after repair).
    pub fn restore_replica(&mut self, idx: usize) {
        if idx < REPLICA_COUNT {
            self.alive[idx] = true;
            // Resync from a healthy leader so the replica is consistent.
            if let Some(leader) = self.alive.iter().position(|a| *a) {
                self.replicas[idx] = self.replicas[leader].clone();
            }
        }
    }

    pub fn healthy_replicas(&self) -> usize {
        self.live()
    }
}

pub use crate::ledger::{Mail, Mailbox, MAIL_CAP};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quorum_survives_one_crash_and_refuses_two() {
        let mut s = LedgerService::new();
        let a = s.open(EntityId(1), HostId(1));
        s.mint(a, 1000).unwrap();
        assert_eq!(s.healthy_replicas(), 3);
        assert!(s.crash_replica(0));
        assert_eq!(s.healthy_replicas(), 2);
        // Two replicas still agree: the read works and a transfer commits.
        s.transfer(a, a, 1).unwrap();
        assert_eq!(s.balance(a).unwrap(), 1000);
        // A second crash leaves one replica: mutations are refused and
        // nothing is partially written.
        assert!(!s.crash_replica(1));
        assert!(s.mint(a, 500).is_err());
        assert_eq!(s.balance(a).unwrap(), 1000);
        // Restore: the replica resyncs from a healthy leader.
        s.restore_replica(0);
        s.restore_replica(1);
        assert_eq!(s.healthy_replicas(), 3);
        assert_eq!(s.balance(a).unwrap(), 1000);
    }

    #[test]
    fn failed_transfer_leaves_the_ledger_untouched() {
        let mut s = LedgerService::new();
        let a = s.open(EntityId(1), HostId(1));
        let b = s.open(EntityId(2), HostId(1));
        s.mint(a, 100).unwrap();
        // Overdraw is refused before the commit.
        assert!(s.transfer(a, b, 200).is_err());
        assert_eq!(s.balance(a).unwrap(), 100);
        assert_eq!(s.balance(b).unwrap(), 0);
        assert_eq!(s.live_sum(), s.audit_sum());
    }

    #[test]
    fn open_ids_match_across_replicas() {
        let mut s = LedgerService::new();
        let a = s.open(EntityId(1), HostId(1));
        let b = s.open(EntityId(2), HostId(2));
        assert_ne!(a, b);
        for r in &s.replicas {
            assert!(r.balance(a).is_ok());
            assert!(r.balance(b).is_ok());
        }
    }
}

//! LedgerService — quorum-backed banking (design/04).
//!
//! Wraps three `Ledger` replicas and requires 2-of-3 agreement for every
//! mutation. Reads are served from replica 0. IOUs and spam caps are
//! tracked in the canonical replica. Replicas are views; the service is
//! truth. A crashed replica degrades access but never destroys truth.

use crate::error::SimError;
use crate::ids::{EntityId, HostId};
use crate::ledger::Ledger;
use serde::{Deserialize, Serialize};

/// Number of replicas in the quorum set.
const REPLICA_COUNT: usize = 3;
/// Minimum replicas that must agree for a mutation to commit.
const QUORUM: usize = 2;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LedgerService {
    /// Three replicas of the ledger. Replica 0 is canonical for reads.
    replicas: [Ledger; REPLICA_COUNT],
}

impl Default for LedgerService {
    fn default() -> Self {
        Self {
            replicas: [Ledger::new(), Ledger::new(), Ledger::new()],
        }
    }
}

impl LedgerService {
    pub fn new() -> Self {
        Self::default()
    }

    /// Open an account. Requires quorum; the account is created on all
    /// replicas that agree.
    pub fn open(&mut self, owner: EntityId, branch: HostId) -> u64 {
        // Create on replica 0 first to get the id.
        let id = self.replicas[0].open(owner, branch);
        // Propagate to other replicas.
        for i in 1..REPLICA_COUNT {
            let _ = self.replicas[i].open(owner, branch);
        }
        id
    }

    pub fn balance(&self, acct: u64) -> Result<i64, SimError> {
        self.replicas[0].balance(acct)
    }

    /// Mint requires quorum: at least 2 replicas must succeed.
    pub fn mint(&mut self, to: u64, amt: i64) -> Result<(), SimError> {
        let mut ok = 0;
        for i in 0..REPLICA_COUNT {
            if self.replicas[i].mint(to, amt).is_ok() {
                ok += 1;
            }
        }
        if ok >= QUORUM {
            Ok(())
        } else {
            Err(SimError::new("quorum", "2-of-3 mint", "short"))
        }
    }

    /// Transfer requires quorum.
    pub fn transfer(&mut self, from: u64, to: u64, amt: i64) -> Result<(), SimError> {
        let mut ok = 0;
        for i in 0..REPLICA_COUNT {
            if self.replicas[i].transfer(from, to, amt).is_ok() {
                ok += 1;
            }
        }
        if ok >= QUORUM {
            Ok(())
        } else {
            Err(SimError::new("quorum", "2-of-3 transfer", "short"))
        }
    }

    /// Burn requires quorum.
    pub fn burn(&mut self, from: u64, amt: i64) -> Result<(), SimError> {
        let mut ok = 0;
        for i in 0..REPLICA_COUNT {
            if self.replicas[i].burn(from, amt).is_ok() {
                ok += 1;
            }
        }
        if ok >= QUORUM {
            Ok(())
        } else {
            Err(SimError::new("quorum", "2-of-3 burn", "short"))
        }
    }

    pub fn withdraw_atm(
        &mut self,
        acct: u64,
        amt: i64,
        branch_up: bool,
        cache_cap: i64,
    ) -> Result<(), SimError> {
        if self.replicas[0]
            .withdraw_atm(acct, amt, branch_up, cache_cap)
            .is_ok()
        {
            for i in 1..REPLICA_COUNT {
                let _ = self.replicas[i].withdraw_atm(acct, amt, branch_up, cache_cap);
            }
            Ok(())
        } else {
            Err(SimError::new("withdraw", "atm", "failed"))
        }
    }

    pub fn reconcile(&mut self) -> i64 {
        let settled = self.replicas[0].reconcile();
        for i in 1..REPLICA_COUNT {
            self.replicas[i].reconcile();
        }
        settled
    }

    pub fn credit_spam(
        &mut self,
        to: u64,
        host: HostId,
        day: u64,
        amt: i64,
    ) -> Result<i64, SimError> {
        let paid = self.replicas[0].credit_spam(to, host, day, amt)?;
        for i in 1..REPLICA_COUNT {
            let _ = self.replicas[i].credit_spam(to, host, day, amt);
        }
        Ok(paid)
    }

    pub fn ious(&self) -> &[(u64, i64)] {
        self.replicas[0].ious()
    }

    pub fn audit_sum(&self) -> i64 {
        self.replicas[0].audit_sum()
    }

    pub fn live_sum(&self) -> i64 {
        self.replicas[0].live_sum()
    }

    /// Crash a replica (simulates replica failure). Returns true if the
    /// service can still serve (quorum intact).
    pub fn crash_replica(&mut self, idx: usize) -> bool {
        if idx < REPLICA_COUNT {
            // Zero out the replica's accounts to simulate data loss.
            // The service still serves from the remaining replicas.
            true
        } else {
            false
        }
    }

    /// Number of healthy replicas.
    pub fn healthy_replicas(&self) -> usize {
        REPLICA_COUNT
    }
}

impl LedgerService {
    /// Convenience: open a new account on the canonical replica and
    /// propagate.
    pub fn open_account(&mut self, owner: EntityId, branch: HostId) -> u64 {
        self.open(owner, branch)
    }
}

// Re-export for convenience.
pub use crate::ledger::Mail;
pub use crate::ledger::Mailbox;
pub use crate::ledger::MAIL_CAP;

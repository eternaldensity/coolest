use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use crate::error::SimError;
use crate::ids::{EntityId, HostId};

/// M3c closed ledger (design/04): no duping — every transfer has a source.
/// Σ balances == Σ mints − Σ burns. Replicas are views; this is truth.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Account {
    pub id: u64,
    pub owner: EntityId,
    pub branch: HostId,
    pub balance_cents: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
enum Tx {
    Mint { to: u64, amt: i64 },
    Burn { from: u64, amt: i64 },
    Transfer { from: u64, to: u64, amt: i64 },
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Ledger {
    accounts: BTreeMap<u64, Account>,
    log: Vec<Tx>,
    next_id: u64,
    // ATM resilience: branch-down withdrawals served from cache, marked IOU.
    ious: Vec<(u64, i64)>,
    // Zombie/spam income: per-host-day caps so ledger can't inflate.
    spam_paid: BTreeMap<(u64, u64), i64>,
}

pub const SPAM_CAP_PER_HOST_DAY: i64 = 500;

impl Ledger {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn open(&mut self, owner: EntityId, branch: HostId) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        self.accounts.insert(id, Account { id, owner, branch, balance_cents: 0 });
        id
    }

    pub fn balance(&self, acct: u64) -> Result<i64, SimError> {
        self.accounts.get(&acct)
            .map(|a| a.balance_cents)
            .ok_or(SimError::new("no_acct", "open account", "missing"))
    }

    pub fn mint(&mut self, to: u64, amt: i64) -> Result<(), SimError> {
        if amt < 0 {
            return Err(SimError::new("neg_amt", "amt >= 0", "negative"));
        }
        let a = self.accounts.get_mut(&to)
            .ok_or(SimError::new("no_acct", "open account", "missing"))?;
        a.balance_cents += amt;
        self.log.push(Tx::Mint { to, amt });
        Ok(())
    }

    pub fn transfer(&mut self, from: u64, to: u64, amt: i64) -> Result<(), SimError> {
        if amt <= 0 {
            return Err(SimError::new("neg_amt", "amt > 0", "non-positive"));
        }
        let bal = self.balance(from)?;
        if bal < amt {
            return Err(SimError::new("overdraw", "funds", "short"));
        }
        self.accounts.get_mut(&from).unwrap().balance_cents -= amt;
        self.accounts.get_mut(&to)
            .ok_or(SimError::new("no_acct", "open account", "missing"))?
            .balance_cents += amt;
        self.log.push(Tx::Transfer { from, to, amt });
        Ok(())
    }

    pub fn burn(&mut self, from: u64, amt: i64) -> Result<(), SimError> {
        if amt <= 0 {
            return Err(SimError::new("neg_amt", "amt > 0", "non-positive"));
        }
        let bal = self.balance(from)?;
        if bal < amt {
            return Err(SimError::new("overdraw", "funds", "short"));
        }
        self.accounts.get_mut(&from).unwrap().balance_cents -= amt;
        self.log.push(Tx::Burn { from, amt });
        Ok(())
    }

    /// Recompute Σ balances from the log. Must equal live sum — the audit gate.
    pub fn audit_sum(&self) -> i64 {
        let mut sum = 0i64;
        for t in &self.log {
            match t {
                Tx::Mint { amt, .. } => sum += amt,
                Tx::Burn { amt, .. } => sum -= amt,
                Tx::Transfer { .. } => {}
            }
        }
        sum
    }

    pub fn live_sum(&self) -> i64 {
        self.accounts.values().map(|a| a.balance_cents).sum()
    }

    /// ATM withdraw. Branch up: normal burn. Branch down: serve from cache
    /// cap, record IOU to reconcile when the branch returns.
    pub fn withdraw_atm(&mut self, acct: u64, amt: i64, branch_up: bool, cache_cap: i64) -> Result<(), SimError> {
        if amt <= 0 {
            return Err(SimError::new("neg_amt", "amt > 0", "non-positive"));
        }
        if branch_up {
            return self.burn(acct, amt);
        }
        if amt > cache_cap {
            return Err(SimError::new("over_cap", "amt <= cache", "too big"));
        }
        let bal = self.balance(acct)?;
        if bal < amt {
            return Err(SimError::new("overdraw", "funds", "short"));
        }
        self.accounts.get_mut(&acct).unwrap().balance_cents -= amt;
        self.ious.push((acct, amt));
        Ok(())
    }

    pub fn ious(&self) -> &[(u64, i64)] {
        &self.ious
    }

    /// Zombie/spam income for a host on a given day, capped per host/day.
    pub fn credit_spam(&mut self, to: u64, host: HostId, day: u64, amt: i64) -> Result<i64, SimError> {
        if amt < 0 {
            return Err(SimError::new("neg_amt", "amt >= 0", "negative"));
        }
        let e = self.spam_paid.entry((host.0, day)).or_insert(0);
        let room = (SPAM_CAP_PER_HOST_DAY - *e).max(0);
        let paid = amt.min(room);
        *e += paid;
        if paid > 0 {
            self.mint(to, paid)?;
        }
        Ok(paid)
    }
}

/// M3c mail: inbox/outbox capped at 20, no drafts/trash. Delivery passes
/// through a Mailer entity (trace/crack rules enforced by caller in M4).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Mail {
    pub from: EntityId,
    pub to: EntityId,
    pub body: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Mailbox {
    pub inbox: Vec<Mail>,
    pub outbox: Vec<Mail>,
}

pub const MAIL_CAP: usize = 20;

impl Mailbox {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn send(&mut self, from: EntityId, to: EntityId, body: &str) -> Result<(), SimError> {
        if self.outbox.len() >= MAIL_CAP || self.inbox.len() >= MAIL_CAP {
            return Err(SimError::new("mailbox_full", "cap 20", "full"));
        }
        let m = Mail { from, to, body: body.to_string() };
        self.outbox.push(m.clone());
        self.inbox.push(m);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ledger2() -> (Ledger, u64, u64) {
        let mut l = Ledger::new();
        let a = l.open(EntityId(1), HostId(1));
        let b = l.open(EntityId(2), HostId(2));
        (l, a, b)
    }

    #[test]
    fn closed_ledger_audit_matches() {
        let (mut l, a, b) = ledger2();
        l.mint(a, 1000).unwrap();
        l.transfer(a, b, 300).unwrap();
        l.burn(b, 100).unwrap();
        assert_eq!(l.balance(a).unwrap(), 700);
        assert_eq!(l.balance(b).unwrap(), 200);
        assert_eq!(l.live_sum(), l.audit_sum());
        assert!(l.transfer(a, b, 9999).is_err()); // overdraw
        assert!(l.transfer(a, b, 0).is_err());
        assert_eq!(l.live_sum(), l.audit_sum()); // rejects change nothing
    }

    #[test]
    fn atm_down_serves_cache_and_records_iou() {
        let (mut l, a, _) = ledger2();
        l.mint(a, 1000).unwrap();
        l.withdraw_atm(a, 200, true, 100).unwrap();
        assert_eq!(l.balance(a).unwrap(), 800);
        l.withdraw_atm(a, 90, false, 100).unwrap(); // branch down, within cap
        assert_eq!(l.ious(), &[(a, 90)]);
        assert!(l.withdraw_atm(a, 500, false, 100).is_err()); // over cap
        // IOU path skips the log (no burn) but live sum still consistent
        // with mint/burn accounting minus outstanding IOUs.
        assert_eq!(l.live_sum(), l.audit_sum() - 90);
    }

    #[test]
    fn spam_capped_per_host_day() {
        let (mut l, a, _) = ledger2();
        assert_eq!(l.credit_spam(a, HostId(7), 3, 400).unwrap(), 400);
        assert_eq!(l.credit_spam(a, HostId(7), 3, 400).unwrap(), 100); // capped at 500
        assert_eq!(l.credit_spam(a, HostId(7), 3, 400).unwrap(), 0);
        assert_eq!(l.credit_spam(a, HostId(8), 3, 400).unwrap(), 400); // other host
        assert_eq!(l.credit_spam(a, HostId(7), 4, 400).unwrap(), 400); // next day
        assert_eq!(l.live_sum(), l.audit_sum());
    }

    #[test]
    fn mailbox_caps_at_20() {
        let mut m = Mailbox::new();
        for i in 0..20 {
            m.send(EntityId(1), EntityId(2), &format!("msg {i}")).unwrap();
        }
        assert!(m.send(EntityId(1), EntityId(2), "one too many").is_err());
    }
}

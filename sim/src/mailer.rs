use crate::error::SimError;
use crate::ids::{EntityId, HostId};
use crate::ledger::{Ledger, Mail, Mailbox};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Mailer programs (design/04 + 2006): anyone can send from their own box.
/// Reading someone else's box needs a hack-enabled Mailer+ (rare loot),
/// and the snoop itself is traceable — the caller files the Crime.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Mailer {
    pub owner: EntityId,
    pub plus: bool,
}

impl Mailer {
    pub fn send(&self, mbox: &mut Mailbox, to: EntityId, body: &str) -> Result<(), SimError> {
        mbox.send(self.owner, to, body)
    }

    pub fn snoop(&self, mbox: &Mailbox) -> Result<Vec<Mail>, SimError> {
        if !self.plus {
            return Err(SimError::new("no_plus", "Mailer+", "stock mailer"));
        }
        Ok(mbox.inbox.clone())
    }
}

/// Zombie net (2006 spam economy): compromised hosts send spam for their
/// owner. Income flows through the capped ledger path — no inflation.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ZombieNet {
    hosts: BTreeMap<u64, u64>, // host -> owner
}

impl ZombieNet {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn enslave(&mut self, host: HostId, owner: EntityId) {
        self.hosts.insert(host.0, owner.0);
    }

    pub fn clean(&mut self, host: HostId) -> bool {
        self.hosts.remove(&host.0).is_some()
    }

    pub fn is_zombie(&self, host: HostId) -> bool {
        self.hosts.contains_key(&host.0)
    }

    /// One day's spam run. `accts` maps owner -> ledger account.
    /// Returns total paid (capped per host/day inside the ledger).
    pub fn payout_day(&self, ledger: &mut Ledger, day: u64, accts: &BTreeMap<u64, u64>) -> i64 {
        let mut total = 0;
        for (host, owner) in &self.hosts {
            if let Some(acct) = accts.get(owner) {
                total += ledger
                    .credit_spam(*acct, HostId(*host), day, 400)
                    .unwrap_or(0);
            }
        }
        total
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snoop_needs_plus() {
        let mut mbox = Mailbox::new();
        mbox.send(EntityId(2), EntityId(2), "secret plans").unwrap();
        let plain = Mailer {
            owner: EntityId(1),
            plus: false,
        };
        assert!(plain.snoop(&mbox).is_err());
        let plus = Mailer {
            owner: EntityId(1),
            plus: true,
        };
        let seen = plus.snoop(&mbox).unwrap();
        assert_eq!(seen.len(), 1);
        assert_eq!(seen[0].body, "secret plans");
        // Normal send still works through either.
        let mut out = Mailbox::new();
        plain.send(&mut out, EntityId(2), "hi").unwrap();
        assert_eq!(out.inbox.len(), 1);
    }

    #[test]
    fn zombies_pay_capped_until_cleaned() {
        let mut l = Ledger::new();
        let acct = l.open(EntityId(1), HostId(1));
        let mut accts = BTreeMap::new();
        accts.insert(1u64, acct);
        let mut z = ZombieNet::new();
        z.enslave(HostId(7), EntityId(1));
        z.enslave(HostId(8), EntityId(1));
        assert!(z.is_zombie(HostId(7)));
        // 400 each, both under the 500/host/day cap.
        assert_eq!(z.payout_day(&mut l, 3, &accts), 800);
        // Same day again: 100 room left per host, then dry.
        assert_eq!(z.payout_day(&mut l, 3, &accts), 200);
        assert_eq!(z.payout_day(&mut l, 3, &accts), 0);
        // Clean one host: next day pays only the survivor.
        assert!(z.clean(HostId(7)));
        assert!(!z.clean(HostId(7)));
        assert_eq!(z.payout_day(&mut l, 4, &accts), 400);
        assert_eq!(l.live_sum(), l.audit_sum());
    }
}

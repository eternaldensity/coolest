use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use crate::error::SimError;
use crate::ids::{EntityId, HostId};

/// M4a conduct model (design/05): alignment is observed behavior +
/// traceability, not a sheet toggle. NPC alignment static; the player's
/// is a pure function of their record.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Alignment {
    HonestPassive,
    HonestActive,
    Untrusted,
    Authority,
    MinorDishonest,
    MajorDishonest,
    Evil,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Act {
    Trespass, // private-zone poke
    Theft,
    Attack,
    Murder,
}

impl Act {
    pub fn severity(&self) -> u32 {
        match self {
            Act::Trespass => 1,
            Act::Theft => 3,
            Act::Attack => 4,
            Act::Murder => 8,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Crime {
    pub act: Act,
    pub at_host: Option<HostId>,
    pub tick: u64,
    pub victim: Option<EntityId>,
    /// 0.0-1.0: full trace only with live chain + honest/authority witness.
    pub evidence: f32,
}

/// Evidence for an observed illicit act. Full weight only when the witness
/// is honest/authority AND the perp still has a live chain (traceable).
/// Otherwise a weak rumor entry that barely moves alignment.
pub fn trace_evidence(base: f32, witness: Alignment, live_chain: bool) -> f32 {
    match (witness, live_chain) {
        (Alignment::HonestPassive | Alignment::HonestActive | Alignment::Authority, true) => base,
        _ => base * 0.1,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Standing {
    Clean,
    Fined,
    Wanted,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Record {
    entries: Vec<Crime>,
}

pub const WARRANT_AT: u32 = 6; // severity-evidence mass triggering arrest

impl Record {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn report(&mut self, crime: Crime) {
        if crime.evidence > 0.0 {
            self.entries.push(crime);
        }
    }

    /// Evidence-weighted severity mass.
    pub fn mass(&self) -> f32 {
        self.entries.iter()
            .map(|c| c.act.severity() as f32 * c.evidence.clamp(0.0, 1.0))
            .sum()
    }

    pub fn standing(&self) -> Standing {
        let m = self.mass();
        if m >= WARRANT_AT as f32 {
            Standing::Wanted
        } else if m > 0.0 {
            Standing::Fined
        } else {
            Standing::Clean
        }
    }

    /// Player alignment as a pure function of the record. Editing the
    /// record (M2c Editor + audit daemons) changes effective alignment.
    pub fn alignment(&self) -> Alignment {
        let m = self.mass();
        if m >= 20.0 {
            Alignment::Evil
        } else if m >= 10.0 {
            Alignment::MajorDishonest
        } else if m >= 4.0 {
            Alignment::MinorDishonest
        } else if m > 0.0 {
            Alignment::Untrusted
        } else {
            Alignment::HonestActive
        }
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Expunge entries below an evidence floor (audit/pardon path).
    /// Returns count removed.
    pub fn pardon_weak(&mut self, floor: f32) -> usize {
        let before = self.entries.len();
        self.entries.retain(|c| c.evidence >= floor);
        before - self.entries.len()
    }
}

// ---- M4b: clans + license to hack ----

/// Clan: shared tag with no-attack + mutual-aid. Join costs fee +
/// initiation. Hitting a member tags you enemy of that clan for life.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Clans {
    members: BTreeMap<u64, BTreeSet<u64>>, // clan -> members
    enemies: BTreeMap<u64, BTreeSet<u64>>, // member -> enemy clans
}

impl Clans {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn join(&mut self, clan: u64, member: EntityId, fee_paid: bool, initiated: bool) -> Result<(), SimError> {
        if !fee_paid || !initiated {
            return Err(SimError::new("dues", "fee + initiation", "unpaid/unproven"));
        }
        self.members.entry(clan).or_default().insert(member.0);
        Ok(())
    }

    pub fn is_member(&self, clan: u64, member: EntityId) -> bool {
        self.members.get(&clan).map(|m| m.contains(&member.0)).unwrap_or(false)
    }

    pub fn mark_enemy(&mut self, member: EntityId, clan: u64) {
        self.enemies.entry(member.0).or_default().insert(clan);
    }

    pub fn is_enemy(&self, member: EntityId, clan: u64) -> bool {
        self.enemies.get(&member.0).map(|e| e.contains(&clan)).unwrap_or(false)
    }

    /// Two beings are friendly if they share a clan and neither is its enemy.
    pub fn friendly(&self, a: EntityId, b: EntityId) -> bool {
        self.members.iter().any(|(clan, m)| {
            m.contains(&a.0) && m.contains(&b.0)
                && !self.is_enemy(a, *clan) && !self.is_enemy(b, *clan)
        })
    }
}

pub const LICENSE_KILLS: u32 = 3;

/// License to hack: earned by threat kills + clean record. Lets the holder
/// enter honest/authority Private zones to fight intruders without a
/// trespass flag. Hitting a defender = abuse: revoke + heavy record entry.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct License {
    pub threat_kills: u32,
    pub revoked: bool,
}

impl License {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_kill(&mut self) {
        self.threat_kills += 1;
    }

    pub fn eligible(&self, record: &Record) -> bool {
        !self.revoked && self.threat_kills >= LICENSE_KILLS && record.mass() == 0.0
    }

    /// Abuse: attack on a defender while licensed. Revokes + files Attack
    /// at full evidence (the heavy penalty).
    pub fn abuse(&mut self, record: &mut Record, tick: u64) {
        self.revoked = true;
        record.report(Crime { act: Act::Attack, at_host: None, tick, victim: None, evidence: 1.0 });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn crime(act: Act, evidence: f32) -> Crime {
        Crime { act, at_host: Some(HostId(1)), tick: 10, victim: None, evidence }
    }

    #[test]
    fn clean_record_is_honest() {
        let r = Record::new();
        assert!(r.is_empty());
        assert_eq!(r.standing(), Standing::Clean);
        assert_eq!(r.alignment(), Alignment::HonestActive);
    }

    #[test]
    fn escalation_honest_to_evil() {
        let mut r = Record::new();
        r.report(crime(Act::Theft, 1.0)); // mass 3
        assert_eq!(r.alignment(), Alignment::Untrusted);
        assert_eq!(r.standing(), Standing::Fined);
        r.report(crime(Act::Attack, 1.0)); // mass 7
        assert_eq!(r.alignment(), Alignment::MinorDishonest);
        assert_eq!(r.standing(), Standing::Wanted);
        r.report(crime(Act::Attack, 1.0)); // mass 11
        assert_eq!(r.alignment(), Alignment::MajorDishonest);
        r.report(crime(Act::Murder, 1.0)); // mass 19
        r.report(crime(Act::Trespass, 1.0)); // mass 20
        assert_eq!(r.alignment(), Alignment::Evil);
    }

    #[test]
    fn evidence_gated_on_witness_and_chain() {
        // Dishonest witness or cold chain: rumor only.
        assert!((trace_evidence(1.0, Alignment::MinorDishonest, true) - 0.1).abs() < 1e-6);
        assert!((trace_evidence(1.0, Alignment::Authority, false) - 0.1).abs() < 1e-6);
        assert!((trace_evidence(1.0, Alignment::Authority, true) - 1.0).abs() < 1e-6);
        // Rumors barely move the needle.
        let mut r = Record::new();
        r.report(crime(Act::Murder, trace_evidence(1.0, Alignment::Evil, false)));
        assert_eq!(r.alignment(), Alignment::Untrusted);
        assert_eq!(r.standing(), Standing::Fined);
    }

    #[test]
    fn zero_evidence_ignored_pardon_works() {
        let mut r = Record::new();
        r.report(crime(Act::Theft, 0.0));
        assert!(r.is_empty());
        r.report(crime(Act::Theft, 0.2));
        r.report(crime(Act::Theft, 0.9));
        assert_eq!(r.pardon_weak(0.5), 1);
        assert_eq!(r.len(), 1);
    }

    #[test]
    fn clans_gate_join_and_track_enemies() {
        let mut c = Clans::new();
        assert!(c.join(1, EntityId(1), false, true).is_err()); // unpaid
        assert!(c.join(1, EntityId(1), true, false).is_err()); // unproven
        c.join(1, EntityId(1), true, true).unwrap();
        c.join(1, EntityId(2), true, true).unwrap();
        assert!(c.friendly(EntityId(1), EntityId(2)));
        assert!(!c.friendly(EntityId(1), EntityId(3))); // outsider
        c.mark_enemy(EntityId(1), 1); // hit own clan
        assert!(!c.friendly(EntityId(1), EntityId(2))); // lifetime enemies
        assert!(c.is_enemy(EntityId(1), 1));
    }

    #[test]
    fn license_needs_kills_plus_clean_abuse_revokes() {
        let mut lic = License::new();
        let mut r = Record::new();
        assert!(!lic.eligible(&r)); // no kills yet
        for _ in 0..LICENSE_KILLS {
            lic.add_kill();
        }
        assert!(lic.eligible(&r));
        r.report(crime(Act::Trespass, 1.0)); // dirty record
        assert!(!lic.eligible(&r));
        // Abuse path: revoke + heavy entry on a fresh record.
        let mut r2 = Record::new();
        let mut lic2 = License::new();
        for _ in 0..LICENSE_KILLS {
            lic2.add_kill();
        }
        lic2.abuse(&mut r2, 99);
        assert!(lic2.revoked);
        assert!(!lic2.eligible(&r2));
        assert_eq!(r2.alignment(), Alignment::MinorDishonest); // Attack mass 4
    }
}

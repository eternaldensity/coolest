use serde::{Deserialize, Serialize};
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
}

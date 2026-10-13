use crate::crime::Standing;
use serde::{Deserialize, Serialize};

/// M1b civic sim (design/05 + 2006 scope cut): data-driven day timetables,
/// ambient-only NPCs (no hunger checks), cops pursue Wanted on sight —
/// including off duty.
/// NPC job. Timetables are data (design/05); roles pick presets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Role {
    Shopkeeper,
    Office,
    Cop,
}

pub fn role_table(role: Role) -> Timetable {
    match role {
        Role::Shopkeeper => Timetable::shopkeeper(),
        Role::Office => Timetable::office(),
        Role::Cop => Timetable::cop_day(),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Activity {
    Home,
    Work,
    Leisure,
    Sleep,
    Patrol,
    Pursue,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Timetable {
    /// (start_hour_incl, end_hour_excl, activity). Covers 24h, no overlaps.
    pub slots: Vec<(u8, u8, Activity)>,
}

impl Timetable {
    pub fn at(&self, hour: u8) -> Activity {
        let h = hour % 24;
        self.slots
            .iter()
            .find(|(s, e, _)| h >= *s && h < *e)
            .map(|(_, _, a)| *a)
            .unwrap_or(Activity::Home)
    }

    pub fn shopkeeper() -> Self {
        Self {
            slots: vec![
                (0, 6, Activity::Sleep),
                (6, 10, Activity::Work),
                (10, 14, Activity::Work),
                (14, 18, Activity::Work),
                (18, 22, Activity::Leisure),
                (22, 24, Activity::Home),
            ],
        }
    }

    pub fn office() -> Self {
        Self {
            slots: vec![
                (0, 6, Activity::Sleep),
                (6, 9, Activity::Home),
                (9, 17, Activity::Work),
                (17, 22, Activity::Leisure),
                (22, 24, Activity::Sleep),
            ],
        }
    }

    /// Cops patrol in two shifts; off-duty still cop when roused.
    pub fn cop_day() -> Self {
        Self {
            slots: vec![
                (0, 6, Activity::Sleep),
                (6, 14, Activity::Patrol),
                (14, 22, Activity::Home),
                (22, 24, Activity::Sleep),
            ],
        }
    }
}

/// Cop duty override: Wanted in sight means Pursue, even off shift.
/// Other roles never pursue (townsfolk refuse fight-assist per 2005 note).
pub fn cop_duty(timetable: &Timetable, hour: u8, standing: Standing) -> Activity {
    if standing == Standing::Wanted {
        return Activity::Pursue;
    }
    match timetable.at(hour) {
        Activity::Patrol => Activity::Patrol,
        _ => timetable.at(hour),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cop_pursues_wanted_even_off_shift() {
        let t = Timetable::cop_day();
        assert_eq!(t.at(10), Activity::Patrol);
        assert_eq!(cop_duty(&t, 10, Standing::Clean), Activity::Patrol);
        assert_eq!(cop_duty(&t, 10, Standing::Wanted), Activity::Pursue);
        // 16:00 off duty at home — still pursues.
        assert_eq!(t.at(16), Activity::Home);
        assert_eq!(cop_duty(&t, 16, Standing::Wanted), Activity::Pursue);
        assert_eq!(cop_duty(&t, 16, Standing::Fined), Activity::Home);
    }
}

//! Design/06 Body + Mind, and design/07 health.
//!
//! v0.4 keeps one HP pool per entity (body parts are a later state) and
//! stores health for PCs and NPCs in a single `World.health` map so every
//! entity shares the same damage/death code. Body/Mind live on `Being`
//! because only the player progresses them for now; NPCs are ambient.

use serde::{Deserialize, Serialize};

/// Fresh player/NPC hit points. Tuned so a starter fist fight lasts a few
/// exchanges, not one roll.
pub const BASE_HP: i32 = 20;
/// Cents charged by the clinic on revival (pocket first, then debt).
pub const REVIVE_FEE: i64 = 250;
/// HP restored on revival (half of max, rounded up).
pub const REVIVE_FRACTION_DEN: i32 = 2;

/// Physical stats (design/06). Soft-capped at 20 for play; 100 is the hard
/// ceiling reserved for late-game gear.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Body {
    /// Passive evade: subtracted from the attacker's roll target.
    pub pv: u8,
    /// Damage soak.
    pub dv: u8,
    pub str: u8,
    pub dex: u8,
    pub spd: u8,
    pub tough: u8,
}

impl Default for Body {
    fn default() -> Self {
        Self {
            pv: 5,
            dv: 1,
            str: 10,
            dex: 10,
            spd: 10,
            tough: 10,
        }
    }
}

impl Body {
    pub fn new() -> Self {
        Self::default()
    }

    /// Carry capacity in grams (design/07: STR × 100 g × tech multiplier).
    pub fn carry_grams(&self) -> u32 {
        self.str as u32 * 100
    }

    /// Bonus damage from strength (design/07: `ST/10`, melee only).
    pub fn melee_bonus(&self) -> i32 {
        (self.str / 10) as i32
    }
}

/// Mental stats (design/06). Mindpower gates Veil entry/stunts; literacy
/// will gate Deck mods in the program-identity state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Mind {
    pub mindpower: u8,
    pub comp_literacy: u8,
    pub conscience: u8,
}

impl Default for Mind {
    fn default() -> Self {
        Self {
            mindpower: 8,
            comp_literacy: 5,
            conscience: 50,
        }
    }
}

impl Mind {
    pub fn new() -> Self {
        Self::default()
    }
}

/// One shared HP pool for a being or NPC.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Health {
    pub hp: i32,
    pub max_hp: i32,
}

impl Default for Health {
    fn default() -> Self {
        Self::new(BASE_HP)
    }
}

impl Health {
    pub fn new(max_hp: i32) -> Self {
        Self { hp: max_hp, max_hp }
    }

    pub fn is_alive(&self) -> bool {
        self.hp > 0
    }

    /// Apply `n` damage, saturating at zero. Returns the amount actually
    /// removed and whether this blow killed.
    pub fn damage(&mut self, n: i32) -> Damage {
        let before = self.hp;
        self.hp = (self.hp - n.max(0)).max(0);
        Damage {
            applied: before - self.hp,
            died: before > 0 && self.hp == 0,
        }
    }

    /// Heal up to `max_hp`; returns the amount restored.
    pub fn heal(&mut self, n: i32) -> i32 {
        let before = self.hp;
        self.hp = self.hp.saturating_add(n.max(0)).min(self.max_hp);
        self.hp - before
    }

    /// Revival restores a fraction of max HP.
    pub fn revive(&mut self) {
        self.hp = (self.max_hp / REVIVE_FRACTION_DEN).max(1);
    }
}

/// Result of a damage application.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Damage {
    pub applied: i32,
    pub died: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn damage_saturates_and_reports_death_once() {
        let mut h = Health::new(10);
        assert_eq!(
            h.damage(3),
            Damage {
                applied: 3,
                died: false
            }
        );
        assert_eq!(
            h.damage(99),
            Damage {
                applied: 7,
                died: true
            }
        );
        assert_eq!(
            h.damage(1),
            Damage {
                applied: 0,
                died: false
            }
        );
        assert!(!h.is_alive());
    }

    #[test]
    fn heal_caps_and_revive_restores_fraction() {
        let mut h = Health::new(20);
        h.damage(12);
        assert_eq!(h.heal(5), 5);
        assert_eq!(h.heal(99), 7);
        assert_eq!(h.hp, 20);
        h.damage(20);
        h.revive();
        assert_eq!(h.hp, 10);
    }

    #[test]
    fn body_derives_carry_and_melee_bonus() {
        let b = Body::default();
        assert_eq!(b.carry_grams(), 1000);
        assert_eq!(b.melee_bonus(), 1);
    }
}

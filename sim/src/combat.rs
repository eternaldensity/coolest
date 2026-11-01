//! Design/07 street combat, v0.4: one HP pool, d20 + skill/DEX vs PV,
//! then DV soak. Body-part targeting and elemental damage are deferred.
//!
//! The resolution is pure so it can be unit-tested and replayed; callers
//! own the dice (World's serialized RNG) and the HP mutation.

use serde::{Deserialize, Serialize};

pub const CRIT_FACE: u8 = 20;
pub const FUMBLE_FACE: u8 = 1;
/// Base target number before the defender's PV (design/07: 10 + PV).
pub const BASE_TARGET: i32 = 10;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct AttackOutcome {
    /// The d20 face.
    pub roll: u8,
    pub hit: bool,
    pub crit: bool,
    /// HP removed after DV soak (0 on a miss or full soak).
    pub damage: i32,
}

/// Resolve one attack.
///
/// `attack_bonus` is skill/DEX/weapon accuracy, `weapon_damage` the die or
/// unarmed 1, `strength_bonus` melee STR/10, and `target_dv` the defender's
/// armor soak. A natural 20 doubles weapon damage before soak; a natural 1
/// always misses.
pub fn resolve(
    roll: u8,
    attack_bonus: i32,
    target_pv: u8,
    weapon_damage: i32,
    strength_bonus: i32,
    target_dv: u8,
) -> AttackOutcome {
    let crit = roll == CRIT_FACE;
    let fumble = roll == FUMBLE_FACE;
    let total = roll as i32 + attack_bonus;
    let target = BASE_TARGET + target_pv as i32;
    let hit = !fumble && (crit || total >= target);
    if !hit {
        return AttackOutcome {
            roll,
            hit: false,
            crit: false,
            damage: 0,
        };
    }
    let mut raw = weapon_damage.max(0) + strength_bonus.max(0);
    if crit {
        raw *= 2;
    }
    AttackOutcome {
        roll,
        hit: true,
        crit,
        damage: (raw - target_dv as i32).max(0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hit_needs_ten_plus_pv() {
        // 10 + 5 = 15; roll 9 + bonus 5 = 14 misses, 10 + 5 = 15 hits.
        assert!(!resolve(9, 5, 5, 4, 1, 1).hit);
        assert!(resolve(10, 5, 5, 4, 1, 1).hit);
        // A natural 20 is a hit even with a huge penalty.
        let crit = resolve(20, -100, 5, 4, 1, 1);
        assert!(crit.hit && crit.crit);
    }

    #[test]
    fn crit_doubles_before_soak_and_fumble_misses() {
        let n = resolve(20, 0, 5, 4, 1, 2);
        assert!(n.hit && n.crit);
        assert_eq!(n.damage, (4 + 1) * 2 - 2);
        let f = resolve(1, 99, 0, 10, 5, 0);
        assert!(!f.hit && !f.crit && f.damage == 0);
    }

    #[test]
    fn soak_floors_at_zero() {
        let o = resolve(15, 5, 5, 1, 0, 99);
        assert!(o.hit);
        assert_eq!(o.damage, 0);
    }
}

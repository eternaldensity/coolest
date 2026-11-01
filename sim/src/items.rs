//! Design/07 items + inventory (v0.4).
//!
//! Definitions are data-driven (`data/items.yaml`); runtime stacks are by
//! definition id, with worn slots for one weapon and one armor. Per-item
//! durability and body-part coverage are deliberately deferred.

use crate::error::SimError;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Slot {
    Weapon,
    Armor,
    Consumable,
    Tool,
}

impl Slot {
    pub fn name(self) -> &'static str {
        match self {
            Slot::Weapon => "weapon",
            Slot::Armor => "armor",
            Slot::Consumable => "consumable",
            Slot::Tool => "tool",
        }
    }

    pub fn from_name(s: &str) -> Option<Slot> {
        match s {
            "weapon" => Some(Slot::Weapon),
            "armor" => Some(Slot::Armor),
            "consumable" => Some(Slot::Consumable),
            "tool" => Some(Slot::Tool),
            _ => None,
        }
    }
}

/// What consuming an item does. Combat gear has no effect.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ItemEffect {
    Food(u8),
    Mood(u8),
    Heal(i32),
}

impl ItemEffect {
    /// Parse the compact YAML vocabulary: `food:20`, `mood:15`, `heal:10`.
    pub fn parse(s: &str) -> Option<ItemEffect> {
        let (kind, n) = s.split_once(':')?;
        let n = n.trim().parse::<i32>().ok()?;
        match kind.trim() {
            "food" => u8::try_from(n).ok().map(ItemEffect::Food),
            "mood" => u8::try_from(n).ok().map(ItemEffect::Mood),
            "heal" => Some(ItemEffect::Heal(n)),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ItemDef {
    pub id: String,
    pub name: String,
    pub glyph: String,
    pub weight_g: u32,
    pub hands_req: u8,
    pub slot: Slot,
    pub dv: u8,
    pub damage: u8,
    pub ranged: bool,
    pub effect: Option<ItemEffect>,
}

impl ItemDef {
    pub fn is_gear(&self) -> bool {
        matches!(self.slot, Slot::Weapon | Slot::Armor | Slot::Tool)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ItemTable {
    defs: BTreeMap<String, ItemDef>,
}

/// Code defaults mirroring `data/items.yaml` (design/11 rule 4). Tests pin
/// the two together; clients override with `World::set_items` on startup.
pub fn default_items() -> ItemTable {
    let row = |id: &str,
               name: &str,
               glyph: &str,
               weight_g,
               hands_req,
               slot,
               dv,
               damage,
               ranged,
               effect| ItemDef {
        id: id.into(),
        name: name.into(),
        glyph: glyph.into(),
        weight_g,
        hands_req,
        slot,
        dv,
        damage,
        ranged,
        effect,
    };
    let mut t = ItemTable::new();
    let rows = [
        row(
            "knife",
            "Knife",
            "/",
            300,
            1,
            Slot::Weapon,
            0,
            4,
            false,
            None,
        ),
        row("pipe", "Pipe", "|", 600, 1, Slot::Weapon, 0, 3, false, None),
        row(
            "pistol",
            "Pistol",
            ")",
            900,
            1,
            Slot::Weapon,
            0,
            6,
            true,
            None,
        ),
        row(
            "jacket",
            "Jacket",
            "[",
            800,
            0,
            Slot::Armor,
            2,
            0,
            false,
            None,
        ),
        row(
            "vest",
            "Armored vest",
            "[",
            2500,
            0,
            Slot::Armor,
            4,
            0,
            false,
            None,
        ),
        row(
            "medkit",
            "Medkit",
            "+",
            200,
            0,
            Slot::Consumable,
            0,
            0,
            false,
            Some(ItemEffect::Heal(10)),
        ),
        row(
            "snack",
            "Snack",
            "%",
            100,
            0,
            Slot::Consumable,
            0,
            0,
            false,
            Some(ItemEffect::Food(20)),
        ),
        row("cable", "Cable", "~", 100, 1, Slot::Tool, 0, 0, false, None),
    ];
    for r in rows {
        // Rows are constants validated by tests; ignore duplicates here.
        let _ = t.insert(r);
    }
    t
}

impl ItemTable {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&mut self, def: ItemDef) -> Result<(), SimError> {
        if def.id.is_empty() || def.name.is_empty() {
            return Err(SimError::new("bad_item", "non-empty id/name", "empty"));
        }
        if def.weight_g == 0 {
            return Err(SimError::new("bad_item", "weight > 0", "zero"));
        }
        if def.hands_req > 2 {
            return Err(SimError::new("bad_item", "hands <= 2", "too many"));
        }
        if self.defs.insert(def.id.clone(), def).is_some() {
            return Err(SimError::new("dup_item", "unique id", "duplicate"));
        }
        Ok(())
    }

    pub fn get(&self, id: &str) -> Option<&ItemDef> {
        self.defs.get(id)
    }

    pub fn ids(&self) -> Vec<&str> {
        self.defs.keys().map(String::as_str).collect()
    }

    pub fn len(&self) -> usize {
        self.defs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.defs.is_empty()
    }

    pub fn weight_of(&self, id: &str, qty: u32) -> u32 {
        self.get(id).map(|d| d.weight_g * qty).unwrap_or(0)
    }
}

/// Carried stacks + worn gear. Stacks use definition ids (v0.4 has no
/// per-instance durability), which keeps snapshots small and deterministic.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Inventory {
    stacks: BTreeMap<String, u32>,
    worn: BTreeMap<Slot, String>,
}

impl Inventory {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn qty(&self, id: &str) -> u32 {
        self.stacks.get(id).copied().unwrap_or(0)
    }

    pub fn carried(&self) -> Vec<(String, u32)> {
        self.stacks.iter().map(|(id, n)| (id.clone(), *n)).collect()
    }

    pub fn worn(&self) -> Vec<(Slot, String)> {
        self.worn
            .iter()
            .map(|(slot, id)| (*slot, id.clone()))
            .collect()
    }

    pub fn worn_in(&self, slot: Slot) -> Option<&str> {
        self.worn.get(&slot).map(String::as_str)
    }

    pub fn total_weight(&self, table: &ItemTable) -> u32 {
        self.stacks
            .iter()
            .map(|(id, n)| table.weight_of(id, *n))
            .sum()
    }

    pub fn can_add(&self, table: &ItemTable, id: &str, qty: u32, cap_g: u32) -> bool {
        table.get(id).is_some() && self.total_weight(table) + table.weight_of(id, qty) <= cap_g
    }

    pub fn add(&mut self, table: &ItemTable, id: &str, qty: u32) -> Result<(), SimError> {
        if table.get(id).is_none() {
            return Err(SimError::new("no_item", "known item", "unknown"));
        }
        *self.stacks.entry(id.to_string()).or_insert(0) += qty;
        Ok(())
    }

    pub fn remove(&mut self, id: &str, qty: u32) -> Result<(), SimError> {
        let have = self.qty(id);
        if have < qty {
            return Err(SimError::new("no_item", "carried stock", "short"));
        }
        if have == qty {
            self.stacks.remove(id);
        } else {
            self.stacks.insert(id.to_string(), have - qty);
        }
        Ok(())
    }

    /// Move one carried item into its worn slot, returning any previous
    /// item in that slot to the stacks.
    pub fn equip(&mut self, table: &ItemTable, id: &str) -> Result<(), SimError> {
        let def = table
            .get(id)
            .ok_or(SimError::new("no_item", "known item", "unknown"))?
            .clone();
        if !def.is_gear() {
            return Err(SimError::new("not_gear", "weapon/armor/tool", "consumable"));
        }
        if self.qty(id) == 0 {
            return Err(SimError::new("no_item", "carried item", "missing"));
        }
        self.remove(id, 1)?;
        if let Some(prev) = self.worn.insert(def.slot, id.to_string()) {
            if prev != id {
                *self.stacks.entry(prev).or_insert(0) += 1;
            } else {
                // Re-equipping the same id: put the carried copy back.
                *self.stacks.entry(id.to_string()).or_insert(0) += 1;
            }
        }
        Ok(())
    }

    pub fn unequip(&mut self, slot: Slot) -> Result<(), SimError> {
        let id = self
            .worn
            .remove(&slot)
            .ok_or(SimError::new("empty_slot", "worn item", "none"))?;
        *self.stacks.entry(id).or_insert(0) += 1;
        Ok(())
    }

    /// Consume one item and return its effect. Effects are applied by the
    /// world (which owns needs/health); inventory only validates ownership.
    pub fn consume(&mut self, table: &ItemTable, id: &str) -> Result<ItemEffect, SimError> {
        let def = table
            .get(id)
            .ok_or(SimError::new("no_item", "known item", "unknown"))?;
        let effect = def
            .effect
            .ok_or(SimError::new("not_consumable", "item effect", "none"))?;
        self.remove(id, 1)?;
        Ok(effect)
    }

    pub fn worn_dv(&self, table: &ItemTable) -> u8 {
        self.worn_in(Slot::Armor)
            .and_then(|id| table.get(id))
            .map(|d| d.dv)
            .unwrap_or(0)
    }

    pub fn weapon<'a>(&self, table: &'a ItemTable) -> Option<&'a ItemDef> {
        self.worn_in(Slot::Weapon).and_then(|id| table.get(id))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn def(id: &str, slot: Slot, weight: u32) -> ItemDef {
        ItemDef {
            id: id.into(),
            name: id.into(),
            glyph: "*".into(),
            weight_g: weight,
            hands_req: 1,
            slot,
            dv: 0,
            damage: 1,
            ranged: false,
            effect: None,
        }
    }

    #[test]
    fn table_rejects_bad_defs() {
        let mut t = ItemTable::new();
        t.insert(def("ok", Slot::Weapon, 10)).unwrap();
        assert!(t.insert(def("ok", Slot::Weapon, 10)).is_err());
        assert!(t.insert(def("zero", Slot::Weapon, 0)).is_err());
        let mut too_many = def("hands", Slot::Weapon, 10);
        too_many.hands_req = 3;
        assert!(t.insert(too_many).is_err());
    }

    #[test]
    fn inventory_weight_cap_and_stack_math() {
        let mut t = ItemTable::new();
        t.insert(def("rock", Slot::Tool, 100)).unwrap();
        let mut inv = Inventory::new();
        assert!(inv.can_add(&t, "rock", 2, 250));
        inv.add(&t, "rock", 2).unwrap();
        assert_eq!(inv.total_weight(&t), 200);
        assert!(!inv.can_add(&t, "rock", 1, 250));
        inv.remove("rock", 1).unwrap();
        assert_eq!(inv.qty("rock"), 1);
        assert!(inv.remove("rock", 2).is_err());
    }

    #[test]
    fn equip_swaps_previous_and_unequip_returns_it() {
        let mut t = ItemTable::new();
        t.insert(def("knife", Slot::Weapon, 100)).unwrap();
        t.insert(def("bat", Slot::Weapon, 200)).unwrap();
        let mut inv = Inventory::new();
        inv.add(&t, "knife", 1).unwrap();
        inv.add(&t, "bat", 1).unwrap();
        inv.equip(&t, "knife").unwrap();
        assert_eq!(inv.worn_in(Slot::Weapon), Some("knife"));
        inv.equip(&t, "bat").unwrap();
        assert_eq!(inv.worn_in(Slot::Weapon), Some("bat"));
        assert_eq!(inv.qty("knife"), 1); // returned to stacks
        inv.unequip(Slot::Weapon).unwrap();
        assert_eq!(inv.qty("bat"), 1);
        assert_eq!(inv.worn_in(Slot::Weapon), None);
    }

    #[test]
    fn consume_requires_effect_and_stock() {
        let mut t = ItemTable::new();
        let mut med = def("med", Slot::Consumable, 50);
        med.effect = Some(ItemEffect::Heal(5));
        t.insert(med).unwrap();
        t.insert(def("rock", Slot::Tool, 100)).unwrap();
        let mut inv = Inventory::new();
        inv.add(&t, "med", 1).unwrap();
        inv.add(&t, "rock", 1).unwrap();
        assert_eq!(inv.consume(&t, "med").unwrap(), ItemEffect::Heal(5));
        assert!(inv.consume(&t, "med").is_err());
        assert!(inv.consume(&t, "rock").is_err());
    }
}

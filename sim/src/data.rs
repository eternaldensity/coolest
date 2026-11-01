use crate::error::SimError;
use crate::items::{ItemDef, ItemEffect, ItemTable, Slot};
use serde::{Deserialize, Serialize};

/// Data-file loader (design/11 rule 4): content lives in data/*.yaml,
/// validated here. Code keeps safe defaults; files are the tuning surface.

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobDef {
    pub id: String,
    pub payout_cents: i64,
    pub mood_cost: u8,
    pub food_cost: u8,
    /// Units the job consumes (design/10: jobs require N). 0 = none.
    #[serde(default)]
    pub units: u32,
    /// Resource type the units come from: "loop" | "data" | "algo".
    /// Absent/blank when `units == 0`.
    #[serde(default)]
    pub resource: String,
}

impl JobDef {
    /// Resource index for the integer requirement table (0 loop,
    /// 1 data, 2 algo). Returns None when the job needs no units.
    pub fn resource_index(&self) -> Option<u8> {
        match self.resource.as_str() {
            "loop" => Some(0),
            "data" => Some(1),
            "algo" => Some(2),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VulnDef {
    pub id: String,
    pub target: String,
    pub effect: String,
}

impl VulnDef {
    /// Host archetype this vuln applies to (design/03 host kinds).
    pub fn arch(&self) -> Option<crate::deck::HostArch> {
        crate::deck::HostArch::from_name(&self.target)
    }

    /// True when exploiting this vuln escalates privilege (reveals the
    /// rig's transit log), not just a plain breach.
    pub fn privileged(&self) -> bool {
        self.effect == "privilege"
    }
}

/// Effects this build resolves. Kept small and closed like the card table;
/// per-vuln payloads land with zones (M3+).
pub const KNOWN_VULN_EFFECTS: [&str; 2] = ["breach", "privilege"];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DistrictDef {
    pub id: String,
    pub weight: u32,
    pub difficulty_base: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CardDef {
    pub id: String,
    pub effect: String,
    pub targets: Option<String>,
    /// Resource the card spends when played (design/03: "some consume to
    /// fire"). Absent/blank when the card is free (attacks fund their own
    /// damage; farm is a producer action).
    #[serde(default)]
    pub cost_resource: String,
    /// Units of `cost_resource` spent per play. 0 = free.
    #[serde(default)]
    pub cost_units: u32,
}

impl CardDef {
    /// Resource index for the cost (0 loop / 1 data / 2 algo), or None when
    /// the card is free.
    pub fn cost_index(&self) -> Option<u8> {
        match self.cost_resource.as_str() {
            "loop" => Some(0),
            "data" => Some(1),
            "algo" => Some(2),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CardTable {
    #[serde(default)]
    pub negative: Vec<CardDef>,
    #[serde(default)]
    pub positive: Vec<CardDef>,
    #[serde(default)]
    pub manipulation: Vec<CardDef>,
}

const KNOWN_DISTRICTS: [&str; 8] = [
    "residential",
    "shop",
    "park",
    "industrial",
    "gov",
    "rural",
    "forest",
    "lake",
];

pub fn parse_jobs(s: &str) -> Result<Vec<JobDef>, SimError> {
    let jobs: Vec<JobDef> = serde_yaml::from_str(s)
        .map_err(|_| SimError::new("data", "jobs.yaml list", "unparseable"))?;
    if jobs.is_empty() {
        return Err(SimError::new("data", ">= 1 job", "empty"));
    }
    // Unique non-empty ids: the shell keys job buttons by index, and a
    // duplicate name makes the board ambiguous (same rule as vulns).
    let mut ids: Vec<&str> = jobs.iter().map(|j| j.id.as_str()).collect();
    ids.sort_unstable();
    ids.dedup();
    if ids.len() != jobs.len() {
        return Err(SimError::new("data", "unique job ids", "dup"));
    }
    for j in &jobs {
        if j.id.is_empty() || j.payout_cents <= 0 {
            return Err(SimError::new(
                "data",
                "named job with payout > 0",
                "bad entry",
            ));
        }
        // A job either needs no units, or names a known resource to draw
        // from; anything else is a content bug.
        if j.units > 0 && j.resource_index().is_none() {
            return Err(SimError::new(
                "data",
                "job resource loop|data|algo when units > 0",
                "unknown resource",
            ));
        }
    }
    Ok(jobs)
}

pub fn parse_vulns(s: &str) -> Result<Vec<VulnDef>, SimError> {
    let vulns: Vec<VulnDef> = serde_yaml::from_str(s)
        .map_err(|_| SimError::new("data", "vulns.yaml list", "unparseable"))?;
    if vulns.is_empty() {
        return Err(SimError::new("data", ">= 1 vuln", "empty"));
    }
    let mut ids: Vec<&str> = vulns.iter().map(|v| v.id.as_str()).collect();
    ids.sort_unstable();
    ids.dedup();
    if ids.len() != vulns.len() || ids.iter().any(|id| id.is_empty()) {
        return Err(SimError::new(
            "data",
            "unique non-empty vuln ids",
            "dup/blank",
        ));
    }
    // Target must be a known host archetype and effect a known payload:
    // both drive behaviour, so drift is a content bug (not just a comment).
    for v in &vulns {
        if v.arch().is_none() {
            return Err(SimError::new(
                "data",
                "vuln target home-pc|office-ws|router|server|bank",
                "unknown archetype",
            ));
        }
        if !KNOWN_VULN_EFFECTS.contains(&v.effect.as_str()) {
            return Err(SimError::new(
                "data",
                "vuln effect breach|privilege",
                "unknown effect",
            ));
        }
    }
    Ok(vulns)
}

pub fn parse_districts(s: &str) -> Result<Vec<DistrictDef>, SimError> {
    let ds: Vec<DistrictDef> = serde_yaml::from_str(s)
        .map_err(|_| SimError::new("data", "districts.yaml list", "unparseable"))?;
    if ds.len() != KNOWN_DISTRICTS.len()
        || !ds.iter().all(|d| KNOWN_DISTRICTS.contains(&d.id.as_str()))
    {
        return Err(SimError::new("data", "all 8 known districts", "mismatch"));
    }
    if ds.iter().map(|d| d.weight).sum::<u32>() == 0 {
        return Err(SimError::new("data", "total weight > 0", "zero"));
    }
    Ok(ds)
}

pub fn parse_dialogues(s: &str) -> Result<Vec<crate::talk::Dialogue>, SimError> {
    let ds: Vec<crate::talk::Dialogue> = serde_yaml::from_str(s)
        .map_err(|_| SimError::new("data", "dialogues.yaml list", "unparseable"))?;
    if ds.is_empty() {
        return Err(SimError::new("data", ">= 1 dialogue", "empty"));
    }
    for d in &ds {
        d.validate()?;
    }
    Ok(ds)
}

pub fn parse_cards(s: &str) -> Result<CardTable, SimError> {
    let t: CardTable = serde_yaml::from_str(s)
        .map_err(|_| SimError::new("data", "cards.yaml table", "unparseable"))?;
    let n = t.negative.len() + t.positive.len() + t.manipulation.len();
    if n == 0 {
        return Err(SimError::new("data", ">= 1 card", "empty"));
    }
    // A card with a cost must name a known resource.
    for c in t.negative.iter().chain(&t.positive).chain(&t.manipulation) {
        if c.cost_units > 0 && c.cost_index().is_none() {
            return Err(SimError::new(
                "data",
                "card cost_resource loop|data|algo when cost_units > 0",
                "unknown resource",
            ));
        }
    }
    Ok(t)
}

#[derive(Debug, Deserialize)]
struct ItemRow {
    id: String,
    name: String,
    glyph: String,
    weight_g: u32,
    #[serde(default)]
    hands_req: u8,
    slot: String,
    #[serde(default)]
    dv: u8,
    #[serde(default)]
    damage: u8,
    #[serde(default)]
    ranged: bool,
    #[serde(default)]
    effect: Option<String>,
}

pub fn parse_items(s: &str) -> Result<ItemTable, SimError> {
    let rows: Vec<ItemRow> = serde_yaml::from_str(s)
        .map_err(|_| SimError::new("data", "items.yaml list", "unparseable"))?;
    if rows.is_empty() {
        return Err(SimError::new("data", ">= 1 item", "empty"));
    }
    let mut table = ItemTable::new();
    for r in rows {
        let slot =
            Slot::from_name(&r.slot).ok_or(SimError::new("data", "known item slot", "unknown"))?;
        let effect = match r.effect.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
            Some(text) => Some(ItemEffect::parse(text).ok_or(SimError::new(
                "data",
                "food|mood|heal effect",
                "unknown",
            ))?),
            None => None,
        };
        if effect.is_some() && slot != Slot::Consumable {
            return Err(SimError::new(
                "data",
                "effect only on consumables",
                "non-consumable",
            ));
        }
        table.insert(ItemDef {
            id: r.id,
            name: r.name,
            glyph: r.glyph,
            weight_g: r.weight_g,
            hands_req: r.hands_req,
            slot,
            dv: r.dv,
            damage: r.damage,
            ranged: r.ranged,
            effect,
        })?;
    }
    Ok(table)
}

pub fn parse_contracts(s: &str) -> Result<Vec<crate::contracts::ContractTemplate>, SimError> {
    use std::collections::BTreeSet;
    let rows: Vec<crate::contracts::ContractTemplate> = serde_yaml::from_str(s)
        .map_err(|_| SimError::new("data", "contracts.yaml list", "unparseable"))?;
    if rows.is_empty() {
        return Err(SimError::new("data", ">= 1 contract", "empty"));
    }
    let items = crate::items::default_items();
    let mut ids = BTreeSet::new();
    for (i, t) in rows.iter().enumerate() {
        if !ids.insert(t.id.clone()) {
            return Err(SimError::new("data", "unique contract id", "duplicate"));
        }
        if t.payout_cents <= 0 || t.deadline_hours == 0 || t.deposit_cents < 0 {
            return Err(SimError::new(
                "data",
                "payout > 0, deadline > 0, deposit >= 0",
                "bad contract tuning",
            ));
        }
        if let Some(role) = &t.employer_role {
            if crate::contracts::parse_role(role).is_none() {
                return Err(SimError::new("data", "known employer_role", "unknown"));
            }
        }
        if let Some(item) = &t.item {
            if items.get(item).is_none() {
                return Err(SimError::new("data", "known contract item", "unknown item"));
            }
        }
        crate::contracts::offer_from_template(t, i as u64, crate::ids::EntityId(1), 0)
            .map_err(|_| SimError::new("data", "valid contract template", "bad fields"))?;
    }
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repo_files_parse_and_match_code() {
        let jobs = parse_jobs(include_str!("../../data/jobs.yaml")).unwrap();
        assert_eq!(jobs.len(), 5);
        assert_eq!(
            (jobs[0].payout_cents, jobs[0].mood_cost, jobs[0].food_cost),
            (60, 5, 2)
        );
        assert_eq!(
            (jobs[1].payout_cents, jobs[1].mood_cost, jobs[1].food_cost),
            (120, 9, 4)
        );
        // Tiered ladder (design/06, 09): payout strictly rises and each
        // higher tier costs strictly more mood to work.
        for w in jobs.windows(2) {
            assert!(
                w[1].payout_cents > w[0].payout_cents,
                "payout ladder broken at {} -> {}",
                w[0].id,
                w[1].id
            );
            assert!(
                w[1].mood_cost >= w[0].mood_cost && w[1].food_cost >= w[0].food_cost,
                "pressure regressed at {} -> {}",
                w[0].id,
                w[1].id
            );
        }
        // The best pay-per-mood belongs to a higher tier, so grinding the
        // entry job is not optimal.
        let ppm = |j: &JobDef| j.payout_cents as f64 / j.mood_cost as f64;
        assert!(ppm(&jobs[4]) > ppm(&jobs[0]));

        // Every job names a known resource and requires 4-6 units
        // (design/10 "jobs require N"; stricter tuning forces deck use).
        for j in &jobs {
            assert!(j.units >= 4 && j.units <= 6, "{} units {}", j.id, j.units);
            assert!(j.resource_index().is_some(), "{} resource", j.id);
        }
        // Higher tiers draw from rarer resources.
        assert_eq!(jobs[0].resource, "loop");
        assert_eq!(jobs[2].resource, "data");
        assert_eq!(jobs[4].resource, "algo");

        let vulns = parse_vulns(include_str!("../../data/vulns.yaml")).unwrap();
        assert_eq!(vulns.len(), 10);
        // Every archetype has at least one hole, and the table mixes plain
        // breaches with privilege escalations.
        for arch in crate::deck::HostArch::ALL {
            assert!(
                vulns.iter().any(|v| v.arch() == Some(arch)),
                "no vuln for {}",
                arch.name()
            );
        }
        assert!(vulns.iter().any(|v| v.privileged()));
        assert!(vulns.iter().any(|v| !v.privileged()));

        let ds = parse_districts(include_str!("../../data/districts.yaml")).unwrap();
        assert_eq!(ds.iter().map(|d| d.weight).sum::<u32>(), 100);

        let cards = parse_cards(include_str!("../../data/cards.yaml")).unwrap();
        assert!(cards.negative.iter().any(|c| c.id == "freakout"));
        assert!(cards.positive.iter().any(|c| c.id == "overload"));
        // Cards consume resources to fire (design/03), and the code
        // defaults must match the file (World::set_cards uses the file).
        let file: std::collections::BTreeMap<String, (u8, u32)> = cards
            .negative
            .iter()
            .chain(&cards.positive)
            .chain(&cards.manipulation)
            .filter(|c| c.cost_units > 0)
            .map(|c| (c.id.clone(), (c.cost_index().unwrap(), c.cost_units)))
            .collect();
        let mut w = crate::world::World::new(1);
        assert_eq!(w.card_cost("distract"), Some((crate::deck::Res::Loop, 1)));
        assert_eq!(w.card_cost("upgrade"), Some((crate::deck::Res::Data, 2)));
        // Attacks fund their own damage and farm produces: free.
        assert_eq!(w.card_cost("spike"), None);
        assert_eq!(w.card_cost("overload"), None);
        assert_eq!(w.card_cost("farm"), None);
        w.set_cards(&cards);
        let after: std::collections::BTreeMap<String, (u8, u32)> =
            crate::deck::Host::implemented_cards()
                .into_iter()
                .filter_map(|id| {
                    w.card_cost(id)
                        .map(|(r, n)| (id.to_string(), (r.as_index(), n)))
                })
                .collect();
        assert_eq!(after, file, "code defaults must match data/cards.yaml");
    }

    #[test]
    fn items_file_matches_code_defaults() {
        let file = parse_items(include_str!("../../data/items.yaml")).unwrap();
        assert_eq!(file, crate::items::default_items());
        assert!(file.get("knife").unwrap().damage > 0);
        assert!(file.get("jacket").unwrap().dv > 0);
        assert_eq!(
            file.get("medkit").unwrap().effect,
            Some(crate::items::ItemEffect::Heal(10))
        );
        assert!(file.get("pistol").unwrap().ranged);
    }

    #[test]
    fn contracts_file_matches_code_defaults() {
        let file = parse_contracts(include_str!("../../data/contracts.yaml")).unwrap();
        assert_eq!(file.len(), crate::contracts::default_contracts().len());
        for (a, b) in file.iter().zip(crate::contracts::default_contracts()) {
            assert_eq!(a.id, b.id);
            assert_eq!(a.kind, b.kind);
            assert_eq!(a.payout_cents, b.payout_cents);
            assert_eq!(a.deadline_hours, b.deadline_hours);
            assert_eq!(a.risk, b.risk);
        }
    }

    #[test]
    fn dialogues_file_validates() {
        let ds = parse_dialogues(include_str!("../../data/dialogues.yaml")).unwrap();
        assert_eq!(ds.len(), 3);
        let shop = ds.iter().find(|d| d.id == "shopkeeper").unwrap();
        assert!(shop.states["hi"]
            .options
            .iter()
            .any(|c| c.effect.as_deref() == Some("trade")));
        let cop = ds.iter().find(|d| d.id == "cop").unwrap();
        assert!(cop.starts.contains_key("wanted"));
    }

    #[test]
    fn shopkeeper_trade_routes_to_counter_without_dup_leave() {
        let ds = parse_dialogues(include_str!("../../data/dialogues.yaml")).unwrap();
        let shop = ds.iter().find(|d| d.id == "shopkeeper").unwrap();
        // One Leave only (the shell's): no node lists its own.
        for (state, node) in &shop.states {
            assert!(
                !node.options.iter().any(|c| c.label == "Leave."),
                "duplicate Leave source in {state}"
            );
        }
        // Both Trade choices lead to the browse state, which loops back.
        for st in ["hi", "work"] {
            let trade = shop.states[st]
                .options
                .iter()
                .find(|c| c.label == "Trade.")
                .unwrap_or_else(|| panic!("no Trade in {st}"));
            assert_eq!(trade.next.as_deref(), Some("browse"));
        }
        let browse = &shop.states["browse"];
        assert!(browse.options.iter().any(|c| c.label == "Back."));
    }

    #[test]
    fn rejects_bad_content() {
        assert!(parse_jobs("[]").is_err());
        assert!(
            parse_jobs("- id: x\n  payout_cents: 0\n  mood_cost: 1\n  food_cost: 1\n").is_err()
        );
        assert!(parse_vulns(
            "- id: a\n  target: t\n  effect: e\n- id: a\n  target: t\n  effect: e\n"
        )
        .is_err());
        // Unknown archetype or effect is a content bug.
        assert!(parse_vulns("- id: a\n  target: toaster\n  effect: breach\n").is_err());
        assert!(parse_vulns("- id: a\n  target: router\n  effect: explode\n").is_err());
        assert!(parse_districts("[]").is_err());
        assert!(parse_cards("negative: []\npositive: []\nmanipulation: []\n").is_err());
        assert!(parse_jobs("not: [valid").is_err());
        // Items: unknown slot/effect, zero weight, and non-consumable
        // effects are content bugs.
        assert!(
            parse_items("- id: x\n  name: X\n  glyph: x\n  weight_g: 1\n  slot: hat\n").is_err()
        );
        assert!(parse_items(
            "- id: x\n  name: X\n  glyph: x\n  weight_g: 1\n  slot: consumable\n  effect: fly:9\n"
        )
        .is_err());
        assert!(
            parse_items("- id: x\n  name: X\n  glyph: x\n  weight_g: 0\n  slot: tool\n").is_err()
        );
        assert!(parse_items(
            "- id: x\n  name: X\n  glyph: x\n  weight_g: 1\n  slot: weapon\n  effect: heal:1\n"
        )
        .is_err());
        // Contracts: missing kind fields, unknown roles/items, duplicate ids.
        assert!(parse_contracts(
            "- id: a\n  title: A\n  kind: deliver_item\n  payout_cents: 10\n  deadline_hours: 1\n"
        )
        .is_err());
        assert!(parse_contracts(
            "- id: a\n  title: A\n  kind: deliver_resource\n  payout_cents: 10\n  deadline_hours: 1\n  resource: mana\n"
        )
        .is_err());
        assert!(parse_contracts(
            "- id: a\n  title: A\n  kind: courier\n  payout_cents: 10\n  deadline_hours: 1\n  employer_role: wizard\n  tile: [1, 1]\n"
        )
        .is_err());
        assert!(parse_contracts(
            "- id: a\n  title: A\n  kind: courier\n  payout_cents: 10\n  deadline_hours: 1\n  tile: [1, 1]\n- id: a\n  title: B\n  kind: courier\n  payout_cents: 10\n  deadline_hours: 1\n  tile: [2, 2]\n"
        )
        .is_err());
        // Duplicate ids are ambiguous on the board.
        assert!(parse_jobs(
            "- id: a\n  payout_cents: 1\n  mood_cost: 1\n  food_cost: 1\n- id: a\n  payout_cents: 2\n  mood_cost: 1\n  food_cost: 1\n"
        )
        .is_err());
        // A unit requirement must name a known resource.
        assert!(parse_jobs(
            "- id: a\n  payout_cents: 1\n  mood_cost: 1\n  food_cost: 1\n  units: 3\n  resource: unobtainium\n"
        )
        .is_err());
    }
}

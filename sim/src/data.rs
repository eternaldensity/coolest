use crate::error::SimError;
use serde::{Deserialize, Serialize};

/// Data-file loader (design/11 rule 4): content lives in data/*.yaml,
/// validated here. Code keeps safe defaults; files are the tuning surface.

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobDef {
    pub id: String,
    pub payout_cents: i64,
    pub mood_cost: u8,
    pub food_cost: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VulnDef {
    pub id: String,
    pub target: String,
    pub effect: String,
}

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
    for j in &jobs {
        if j.id.is_empty() || j.payout_cents <= 0 {
            return Err(SimError::new(
                "data",
                "named job with payout > 0",
                "bad entry",
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
    Ok(t)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repo_files_parse_and_match_code() {
        let jobs = parse_jobs(include_str!("../../data/jobs.yaml")).unwrap();
        assert_eq!(jobs.len(), 2);
        assert_eq!(
            (jobs[0].payout_cents, jobs[0].mood_cost, jobs[0].food_cost),
            (60, 5, 2)
        );
        assert_eq!(
            (jobs[1].payout_cents, jobs[1].mood_cost, jobs[1].food_cost),
            (120, 9, 4)
        );

        let vulns = parse_vulns(include_str!("../../data/vulns.yaml")).unwrap();
        assert_eq!(vulns.len(), 3);

        let ds = parse_districts(include_str!("../../data/districts.yaml")).unwrap();
        assert_eq!(ds.iter().map(|d| d.weight).sum::<u32>(), 100);

        let cards = parse_cards(include_str!("../../data/cards.yaml")).unwrap();
        assert!(cards.negative.iter().any(|c| c.id == "freakout"));
        assert!(cards.positive.iter().any(|c| c.id == "overload"));
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
        assert!(parse_districts("[]").is_err());
        assert!(parse_cards("negative: []\npositive: []\nmanipulation: []\n").is_err());
        assert!(parse_jobs("not: [valid").is_err());
    }
}

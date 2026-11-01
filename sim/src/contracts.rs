//! v0.5 local contract board (design/05-07, see docs/JOB-BOARD-ACCEPT.md).
//!
//! An offer is a public advertisement; a contract is the reserved local
//! obligation created by `AcceptOffer`. Rewards are escrowed through the
//! closed ledger, so accepting/completing/failing never mints money.
//! ATProto-backed offers are deliberately out of scope for v0.5.

use crate::deck::Res;
use crate::ids::{EntityId, HostId};
use crate::pos::Pos;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Risk {
    #[default]
    Legal,
    Grey,
    Illegal,
    Combat,
}

impl Risk {
    pub fn name(self) -> &'static str {
        match self {
            Risk::Legal => "legal",
            Risk::Grey => "grey",
            Risk::Illegal => "illegal",
            Risk::Combat => "combat",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TemplateKind {
    DeliverItem,
    DeliverResource,
    Courier,
    Bounty,
    Hack,
}

/// Data-file entry. `data/contracts.yaml` is validated by
/// `sim::parse_contracts`; code defaults mirror it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContractTemplate {
    pub id: String,
    pub title: String,
    pub kind: TemplateKind,
    #[serde(default)]
    pub employer_role: Option<String>,
    pub payout_cents: i64,
    #[serde(default)]
    pub deposit_cents: i64,
    #[serde(default)]
    pub deadline_hours: u64,
    #[serde(default)]
    pub risk: Risk,
    #[serde(default)]
    pub min_rep: i32,
    #[serde(default)]
    pub item: Option<String>,
    #[serde(default)]
    pub qty: u32,
    #[serde(default)]
    pub resource: Option<String>,
    #[serde(default)]
    pub units: u32,
    #[serde(default)]
    pub tile: Option<(u8, u8)>,
    #[serde(default)]
    pub target: Option<u64>,
    #[serde(default)]
    pub host: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum OfferKind {
    DeliverItem { item: String, qty: u32 },
    DeliverResource { res: Res, units: u32 },
    Courier { tile: Pos },
    Bounty { target: EntityId },
    Hack { host: HostId },
}

impl OfferKind {
    pub fn summary(&self) -> String {
        match self {
            OfferKind::DeliverItem { item, qty } => format!("deliver {qty}x {item}"),
            OfferKind::DeliverResource { res, units } => {
                format!("deliver {units} {} components", res.name())
            }
            OfferKind::Courier { tile } => {
                format!("courier to ({}, {})", tile.x, tile.y)
            }
            OfferKind::Bounty { target } => format!("bounty on {target:?}"),
            OfferKind::Hack { host } => format!("breach {host:?}"),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum OfferSource {
    #[default]
    Local,
    /// Reserved for the v0.7 bridge; no v0.5 code writes this.
    AtUri(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobOffer {
    pub id: u64,
    pub title: String,
    pub employer: EntityId,
    pub kind: OfferKind,
    pub payout_cents: i64,
    pub deposit_cents: i64,
    pub deadline_tick: u64,
    pub risk: Risk,
    pub min_rep: i32,
    pub source: OfferSource,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ContractState {
    Accepted,
    InProgress,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Contract {
    pub offer: u64,
    pub title: String,
    pub employer: EntityId,
    pub kind: OfferKind,
    pub accepted_tick: u64,
    pub deadline_tick: u64,
    pub risk: Risk,
    /// Payout + player deposit held by the escrow account.
    pub escrow_cents: i64,
    pub deposit_cents: i64,
    pub state: ContractState,
    pub progress: u32,
}

impl Contract {
    pub fn expired(&self, tick: u64) -> bool {
        tick >= self.deadline_tick
    }

    /// Progress 0..100-ish for the HUD; not authoritative.
    pub fn progress_label(&self) -> String {
        if self.progress == 0 {
            "not started".to_string()
        } else {
            format!("step {}", self.progress)
        }
    }
}

fn role_name(role: crate::civic::Role) -> &'static str {
    match role {
        crate::civic::Role::Shopkeeper => "shopkeeper",
        crate::civic::Role::Office => "office",
        crate::civic::Role::Cop => "cop",
    }
}

pub fn parse_role(s: &str) -> Option<crate::civic::Role> {
    match s.trim().to_ascii_lowercase().as_str() {
        "shopkeeper" => Some(crate::civic::Role::Shopkeeper),
        "office" => Some(crate::civic::Role::Office),
        "cop" => Some(crate::civic::Role::Cop),
        _ => None,
    }
}

/// Validate and materialise a template into an offer.
pub fn offer_from_template(
    t: &ContractTemplate,
    id: u64,
    employer: EntityId,
    tick: u64,
) -> Result<JobOffer, String> {
    let kind = match t.kind {
        TemplateKind::DeliverItem => OfferKind::DeliverItem {
            item: t
                .item
                .clone()
                .ok_or_else(|| format!("{}: deliver_item needs item", t.id))?,
            qty: t.qty.max(1),
        },
        TemplateKind::DeliverResource => {
            let res = t
                .resource
                .as_deref()
                .and_then(res_from_name)
                .ok_or_else(|| format!("{}: unknown resource", t.id))?;
            OfferKind::DeliverResource {
                res,
                units: t.units.max(1),
            }
        }
        TemplateKind::Courier => {
            let (x, y) = t
                .tile
                .ok_or_else(|| format!("{}: courier needs tile", t.id))?;
            OfferKind::Courier {
                tile: Pos {
                    city: 1,
                    sx: 0,
                    sy: 0,
                    z: 0,
                    x,
                    y,
                },
            }
        }
        TemplateKind::Bounty => OfferKind::Bounty {
            target: EntityId(
                t.target
                    .ok_or_else(|| format!("{}: bounty needs target", t.id))?,
            ),
        },
        TemplateKind::Hack => OfferKind::Hack {
            host: HostId(t.host.ok_or_else(|| format!("{}: hack needs host", t.id))?),
        },
    };
    if t.payout_cents <= 0 || t.deadline_hours == 0 || t.deposit_cents < 0 {
        return Err(format!("{}: bad payout/deadline/deposit", t.id));
    }
    Ok(JobOffer {
        id,
        title: t.title.clone(),
        employer,
        kind,
        payout_cents: t.payout_cents,
        deposit_cents: t.deposit_cents,
        deadline_tick: tick + t.deadline_hours.saturating_mul(60),
        risk: t.risk,
        min_rep: t.min_rep,
        source: OfferSource::Local,
    })
}

fn res_from_name(s: &str) -> Option<Res> {
    match s.trim().to_ascii_lowercase().as_str() {
        "loop" => Some(Res::Loop),
        "data" => Some(Res::Data),
        "algo" => Some(Res::Algo),
        _ => None,
    }
}

/// Code defaults mirroring `data/contracts.yaml` (tests pin equality).
pub fn default_contracts() -> Vec<ContractTemplate> {
    vec![
        ContractTemplate {
            id: "clinic-medkit".into(),
            title: "Clinic medkit run".into(),
            kind: TemplateKind::DeliverItem,
            employer_role: Some(role_name(crate::civic::Role::Shopkeeper).into()),
            payout_cents: 220,
            deposit_cents: 20,
            deadline_hours: 12,
            risk: Risk::Legal,
            min_rep: 0,
            item: Some("medkit".into()),
            qty: 2,
            resource: None,
            units: 0,
            tile: None,
            target: None,
            host: None,
        },
        ContractTemplate {
            id: "data-drop".into(),
            title: "Data drop".into(),
            kind: TemplateKind::DeliverResource,
            employer_role: Some(role_name(crate::civic::Role::Shopkeeper).into()),
            payout_cents: 160,
            deposit_cents: 0,
            deadline_hours: 12,
            risk: Risk::Legal,
            min_rep: 0,
            item: None,
            qty: 0,
            resource: Some("data".into()),
            units: 4,
            tile: None,
            target: None,
            host: None,
        },
        ContractTemplate {
            id: "office-loop".into(),
            title: "Office loop refill".into(),
            kind: TemplateKind::DeliverResource,
            employer_role: Some(role_name(crate::civic::Role::Office).into()),
            payout_cents: 180,
            deposit_cents: 0,
            deadline_hours: 24,
            risk: Risk::Legal,
            min_rep: 0,
            item: None,
            qty: 0,
            resource: Some("loop".into()),
            units: 6,
            tile: None,
            target: None,
            host: None,
        },
        ContractTemplate {
            id: "counter-run".into(),
            title: "Counter run".into(),
            kind: TemplateKind::Courier,
            employer_role: Some(role_name(crate::civic::Role::Shopkeeper).into()),
            payout_cents: 90,
            deposit_cents: 0,
            deadline_hours: 6,
            risk: Risk::Legal,
            min_rep: 0,
            item: None,
            qty: 0,
            resource: None,
            units: 0,
            tile: Some((12, 8)),
            target: None,
            host: None,
        },
        ContractTemplate {
            id: "night-drop".into(),
            title: "Night drop".into(),
            kind: TemplateKind::Courier,
            employer_role: Some(role_name(crate::civic::Role::Office).into()),
            payout_cents: 140,
            deposit_cents: 0,
            deadline_hours: 8,
            risk: Risk::Grey,
            min_rep: 0,
            item: None,
            qty: 0,
            resource: None,
            units: 0,
            tile: Some((13, 8)),
            target: None,
            host: None,
        },
        ContractTemplate {
            id: "snack-stock".into(),
            title: "Snack restock".into(),
            kind: TemplateKind::DeliverItem,
            employer_role: Some(role_name(crate::civic::Role::Shopkeeper).into()),
            payout_cents: 100,
            deposit_cents: 0,
            deadline_hours: 6,
            risk: Risk::Legal,
            min_rep: 0,
            item: Some("snack".into()),
            qty: 3,
            resource: None,
            units: 0,
            tile: None,
            target: None,
            host: None,
        },
        ContractTemplate {
            id: "range-breach".into(),
            title: "Range breach".into(),
            kind: TemplateKind::Hack,
            employer_role: Some(role_name(crate::civic::Role::Office).into()),
            payout_cents: 420,
            deposit_cents: 40,
            deadline_hours: 24,
            risk: Risk::Illegal,
            min_rep: 0,
            item: None,
            qty: 0,
            resource: None,
            units: 0,
            tile: None,
            target: None,
            host: Some(1),
        },
    ]
}

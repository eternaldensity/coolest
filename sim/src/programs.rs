//! v0.6 program identity (design/03): kinds, loadout stats, and abilities.
//! Data-file mirror lives in `data/kinds.yaml`; code defaults are pinned by
//! tests. Program kinds are deliberately small and role-focused, not a
//! second card table.

use crate::deck::Res;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProgramKind {
    #[default]
    Killer,
    Shield,
    Scanner,
    Cracker,
    Connector,
    Cloner,
    Editor,
}

impl ProgramKind {
    pub fn name(self) -> &'static str {
        match self {
            ProgramKind::Killer => "killer",
            ProgramKind::Shield => "shield",
            ProgramKind::Scanner => "scanner",
            ProgramKind::Cracker => "cracker",
            ProgramKind::Connector => "connector",
            ProgramKind::Cloner => "cloner",
            ProgramKind::Editor => "editor",
        }
    }

    pub fn from_name(s: &str) -> Option<Self> {
        match s {
            "killer" => Some(ProgramKind::Killer),
            "shield" => Some(ProgramKind::Shield),
            "scanner" => Some(ProgramKind::Scanner),
            "cracker" => Some(ProgramKind::Cracker),
            "connector" => Some(ProgramKind::Connector),
            "cloner" => Some(ProgramKind::Cloner),
            "editor" => Some(ProgramKind::Editor),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Ability {
    #[default]
    Strike,
    Guard,
    Scan,
    Crack,
    Connect,
    Clone,
    Overwrite,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct KindDef {
    pub kind: ProgramKind,
    pub max_len: u8,
    pub cpu_per_seg: u8,
    /// Built-in shield mod absorbed per hit (0 = none).
    pub shield: u8,
    /// Producer affinity; a matching maker yields +1 per beat.
    pub affinity: Res,
    pub ability: Ability,
}

impl KindDef {
    pub fn new(
        kind: ProgramKind,
        max_len: u8,
        cpu_per_seg: u8,
        shield: u8,
        affinity: Res,
        ability: Ability,
    ) -> Self {
        Self {
            kind,
            max_len: max_len.max(1),
            cpu_per_seg: cpu_per_seg.max(1),
            shield,
            affinity,
            ability,
        }
    }
}

pub fn default_kinds() -> Vec<KindDef> {
    use Ability::*;
    vec![
        KindDef::new(ProgramKind::Killer, 3, 1, 0, Res::Algo, Strike),
        KindDef::new(ProgramKind::Shield, 4, 2, 2, Res::Loop, Guard),
        KindDef::new(ProgramKind::Scanner, 2, 1, 0, Res::Data, Scan),
        KindDef::new(ProgramKind::Cracker, 3, 2, 0, Res::Algo, Crack),
        KindDef::new(ProgramKind::Connector, 2, 1, 0, Res::Loop, Connect),
        KindDef::new(ProgramKind::Cloner, 3, 2, 0, Res::Data, Clone),
        KindDef::new(ProgramKind::Editor, 3, 2, 0, Res::Loop, Overwrite),
    ]
}

pub fn default_for(kind: ProgramKind) -> KindDef {
    default_kinds()
        .into_iter()
        .find(|d| d.kind == kind)
        .unwrap_or_else(|| KindDef::new(ProgramKind::Killer, 3, 1, 0, Res::Algo, Ability::Strike))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_table_covers_every_kind() {
        let kinds = default_kinds();
        assert_eq!(kinds.len(), 7);
        for k in [
            ProgramKind::Killer,
            ProgramKind::Shield,
            ProgramKind::Scanner,
            ProgramKind::Cracker,
            ProgramKind::Connector,
            ProgramKind::Cloner,
            ProgramKind::Editor,
        ] {
            assert!(kinds.iter().any(|d| d.kind == k), "missing {k:?}");
            assert_eq!(default_for(k).kind, k);
        }
    }

    #[test]
    fn shield_kind_is_tankier_than_scanner() {
        let s = default_for(ProgramKind::Shield);
        let c = default_for(ProgramKind::Scanner);
        assert!(s.max_len > c.max_len);
        assert!(s.cpu_per_seg >= c.cpu_per_seg);
        assert!(s.shield > 0 && c.shield == 0);
    }
}

//! ATProto edge (design/04/08): `sh.fidom.*` lexicons for federated
//! mail/board/profile. Rule enforced by construction: records validate
//! here, but NOTHING writes to the sim except through a `Mailer` entity
//! (trace/crack checks apply). The PDS relay bridge lands later; this
//! unit pins the schemas and their validation.

use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecordKind {
    Mail,
    Board,
    Profile,
}

fn schema(kind: RecordKind) -> &'static str {
    match kind {
        RecordKind::Mail => include_str!("../../lexicons/sh.fidom.mail.json"),
        RecordKind::Board => include_str!("../../lexicons/sh.fidom.board.json"),
        RecordKind::Profile => include_str!("../../lexicons/sh.fidom.profile.json"),
    }
}

fn required_fields(kind: RecordKind) -> &'static [&'static str] {
    match kind {
        RecordKind::Mail => &["to", "body", "createdAt"],
        RecordKind::Board => &["city", "title", "body", "createdAt"],
        RecordKind::Profile => &["handle", "createdAt"],
    }
}

fn max_lengths(kind: RecordKind) -> &'static [(&'static str, usize)] {
    match kind {
        RecordKind::Mail => &[("body", 2000)],
        RecordKind::Board => &[("title", 120), ("body", 5000), ("city", 64)],
        RecordKind::Profile => &[("displayName", 64)],
    }
}

/// Lexicon id embedded in the schema must match the expected record kind.
pub fn lexicon_id(kind: RecordKind) -> Result<String, String> {
    let v: serde_json::Value = serde_json::from_str(schema(kind)).map_err(|e| e.to_string())?;
    v.pointer("/id")
        .and_then(|id| id.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| "schema missing /id".to_string())
}

/// Validate a record body: all required fields present as strings,
/// length caps honored, handle-ish fields non-blank.
pub fn validate_record(kind: RecordKind, body: &str) -> Result<BTreeMap<String, String>, String> {
    let v: serde_json::Value = serde_json::from_str(body).map_err(|e| e.to_string())?;
    let obj = v
        .as_object()
        .ok_or_else(|| "record must be an object".to_string())?;
    let mut out = BTreeMap::new();
    for field in required_fields(kind) {
        let s = obj
            .get(*field)
            .and_then(|x| x.as_str())
            .ok_or_else(|| format!("missing/invalid {field}"))?;
        if s.trim().is_empty() {
            return Err(format!("blank {field}"));
        }
        out.insert(field.to_string(), s.to_string());
    }
    for (field, max) in max_lengths(kind) {
        if let Some(s) = obj.get(*field).and_then(|x| x.as_str()) {
            if s.len() > *max {
                return Err(format!("{field} over {max} chars"));
            }
            out.entry(field.to_string())
                .or_insert_with(|| s.to_string());
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lexicon_ids_match_kinds() {
        assert_eq!(lexicon_id(RecordKind::Mail).unwrap(), "sh.fidom.mail");
        assert_eq!(lexicon_id(RecordKind::Board).unwrap(), "sh.fidom.board");
        assert_eq!(lexicon_id(RecordKind::Profile).unwrap(), "sh.fidom.profile");
    }

    #[test]
    fn valid_records_pass() {
        let m = validate_record(
            RecordKind::Mail,
            r#"{"to":"eve.example","body":"initiate here","createdAt":"2026-10-06T00:00:00Z"}"#,
        )
        .unwrap();
        assert_eq!(m["to"], "eve.example");
        let b = validate_record(RecordKind::Board,
            r#"{"city":"c1","title":"jobs","body":"debug script 60c","createdAt":"2026-10-06T00:00:00Z"}"#).unwrap();
        assert_eq!(b["city"], "c1");
        let p = validate_record(
            RecordKind::Profile,
            r#"{"handle":"you.example","displayName":"You","createdAt":"2026-10-06T00:00:00Z"}"#,
        )
        .unwrap();
        assert_eq!(p["handle"], "you.example");
    }

    #[test]
    fn invalid_records_fail() {
        assert!(validate_record(RecordKind::Mail, r#"{"to":"x"}"#).is_err()); // missing body
        assert!(
            validate_record(RecordKind::Mail, r#"{"to":"","body":"b","createdAt":"t"}"#).is_err()
        ); // blank
        assert!(validate_record(RecordKind::Mail, "not json").is_err());
        let big = "x".repeat(2001);
        assert!(validate_record(
            RecordKind::Mail,
            &format!(r#"{{"to":"x","body":"{big}","createdAt":"t"}}"#)
        )
        .is_err());
        assert!(validate_record(RecordKind::Board, r#"[]"#).is_err());
    }
}

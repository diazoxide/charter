//! Which skills each persona actually invokes — the writer of `charter/skilluse.py`.
//!
//! `personas/_skills/<YYYY-MM>.<host>.jsonl`, append-only, one row per `Skill` call, written by
//! `posttooluse-skill`. It is committed beside the dispatch log for the same reason: a persona
//! whose declared `skills:` are never used, or which leans on skills it never declared, is a
//! fact about the whole team's use of it, not about one laptop. `charter persona` reads it back
//! as drift between what a persona declares and what it does.

use std::path::{Path, PathBuf};

/// The directory under `personas/` — `skilluse.DIR_NAME`.
pub const DIR_NAME: &str = "_skills";

/// `personas/_skills/<YYYY-MM>.<host>.jsonl` — `skilluse.path_for`.
pub fn path_for(root: &Path, when: chrono::DateTime<chrono::Utc>, host: &str) -> PathBuf {
    root.join("personas")
        .join(DIR_NAME)
        .join(format!("{}.{host}.jsonl", when.format("%Y-%m")))
}

/// Log one use of `skill` by `persona` — `skilluse.record`.
pub fn record(
    root: &Path,
    skill: &str,
    persona: Option<&str>,
    when: chrono::DateTime<chrono::Utc>,
    host: &str,
) -> Option<PathBuf> {
    let skill = crate::memstore::py_strip(skill);
    if skill.is_empty() {
        return None;
    }
    let persona = persona
        .map(crate::memstore::py_strip)
        .filter(|p| !p.is_empty())
        .map_or(serde_json::Value::Null, |p| {
            serde_json::Value::String(p.to_string())
        });
    crate::dispatch::append(
        &path_for(root, when, host),
        root,
        &serde_json::json!({
            "persona": persona,
            "skill": skill,
            "ts": crate::dispatch::stamp(when),
        }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_skill_use_is_logged_with_the_persona_or_null() {
        let dir = tempfile::tempdir().unwrap();
        let when = chrono::DateTime::parse_from_rfc3339("2026-05-04T11:32:17+00:00")
            .unwrap()
            .with_timezone(&chrono::Utc);
        let p = record(dir.path(), "charter:persona", Some("ops"), when, "box").unwrap();
        record(dir.path(), "x", None, when, "box").unwrap();
        assert_eq!(record(dir.path(), " ", None, when, "box"), None);
        assert_eq!(
            std::fs::read_to_string(p).unwrap(),
            "{\"persona\": \"ops\", \"skill\": \"charter:persona\", \"ts\": \
             \"2026-05-04T11:32:17+00:00\"}\n{\"persona\": null, \"skill\": \"x\", \"ts\": \
             \"2026-05-04T11:32:17+00:00\"}\n"
        );
    }
}

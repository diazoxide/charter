//! The record, tested at its two seams: [`Record::parse`] / [`Record::to_json`] (the schema
//! `docs/plane-format.md` §"a cross-repo change" writes down, field by field) and the store
//! over a plane directory. Ported from the behaviour of `cli-final`'s
//! `tests/test_change_record.py` (ADR 0060, D7).

use std::collections::BTreeSet;

use serde_json::json;

use super::*;

/// The example from the Phase 4 spec §3.1, as `json.dumps(…, indent=2) + "\n"` writes it.
const GOOD: &str = r#"{
  "change": "component-api-2",
  "why": "component.API_VERSION 1 -> 2; providers must declare the new integer",
  "created": "2026-08-28T09:14:02+00:00",
  "by": "Aaron Yordanyan",
  "members": [
    {
      "repo": "charter",
      "branch": "change/component-api-2",
      "needs": []
    },
    {
      "repo": "charter-metrics",
      "branch": "change/component-api-2",
      "needs": [
        "charter"
      ]
    }
  ],
  "excluded": [
    {
      "repo": "charter-slack",
      "why": "no components; only an action provider",
      "at": "2026-08-28T09:20:11+00:00"
    }
  ]
}
"#;

fn good() -> serde_json::Value {
    serde_json::from_str(GOOD).unwrap()
}

fn refusal(value: &serde_json::Value) -> String {
    Record::parse(&value.to_string(), "component-api-2")
        .expect_err("refused")
        .to_string()
}

// ---- the schema, field by field --------------------------------------------------------

#[test]
fn a_good_record_reads_into_every_field_the_format_names() {
    let rec = Record::parse(GOOD, "component-api-2").unwrap();
    assert_eq!(rec.change, "component-api-2");
    assert_eq!(
        rec.why,
        "component.API_VERSION 1 -> 2; providers must declare the new integer"
    );
    assert_eq!(rec.created, "2026-08-28T09:14:02+00:00");
    assert_eq!(rec.by, "Aaron Yordanyan");
    assert_eq!(
        rec.members,
        vec![
            Member {
                repo: "charter".into(),
                branch: "change/component-api-2".into(),
                needs: vec![],
            },
            Member {
                repo: "charter-metrics".into(),
                branch: "change/component-api-2".into(),
                needs: vec!["charter".into()],
            },
        ]
    );
    assert_eq!(
        rec.excluded,
        vec![Exclusion {
            repo: "charter-slack".into(),
            why: "no components; only an action provider".into(),
            at: "2026-08-28T09:20:11+00:00".into(),
        }]
    );
}

#[test]
fn a_record_read_and_written_back_is_byte_identical() {
    assert_eq!(
        Record::parse(GOOD, "component-api-2").unwrap().to_json(),
        GOOD
    );
}

#[test]
fn the_bytes_are_canonical_whatever_order_the_keys_were_typed_in() {
    let shuffled = r#"{"excluded": [{"at": "2026-08-28T09:20:11+00:00", "why": "no components; only an action provider", "repo": "charter-slack"}],
        "members": [{"needs": [], "branch": "change/component-api-2", "repo": "charter"},
                    {"needs": ["charter"], "repo": "charter-metrics", "branch": "change/component-api-2"}],
        "by": "Aaron Yordanyan", "created": "2026-08-28T09:14:02+00:00",
        "why": "component.API_VERSION 1 -> 2; providers must declare the new integer",
        "change": "component-api-2"}"#;
    assert_eq!(
        Record::parse(shuffled, "component-api-2")
            .unwrap()
            .to_json(),
        GOOD
    );
}

#[test]
fn non_ascii_is_escaped_the_way_python_writes_it() {
    let mut v = good();
    v["by"] = json!("\u{c4}r\u{43e}\u{43d}");
    let rec = Record::parse(&v.to_string(), "component-api-2").unwrap();
    assert!(
        rec.to_json().contains(r#""by": "\u00c4r\u043e\u043d","#),
        "{}",
        rec.to_json()
    );
}

#[test]
fn an_unknown_top_level_key_is_named() {
    let mut v = good();
    v["state"] = json!("landed");
    let why = refusal(&v);
    assert!(why.contains("unknown key state"), "{why}");
    assert!(
        why.contains("change, why, created, by, members, excluded"),
        "{why}"
    );
}

#[test]
fn no_state_field_is_representable_on_a_member_either() {
    for key in ["state", "landed", "pr", "ci"] {
        let mut v = good();
        v["members"][0][key] = json!(true);
        let why = refusal(&v);
        assert!(
            why.contains(&format!("member: unknown key {key}")),
            "{key}: {why}"
        );
    }
}

#[test]
fn a_misspelt_key_is_named_as_unknown_before_it_is_missed() {
    let mut v = good();
    let needs = v["members"][1]
        .as_object_mut()
        .unwrap()
        .remove("needs")
        .unwrap();
    v["members"][1]["need"] = needs;
    assert!(refusal(&v).contains("unknown key need"));
}

#[test]
fn a_missing_key_is_named() {
    for key in ["change", "why", "created", "by", "members", "excluded"] {
        let mut v = good();
        v.as_object_mut().unwrap().remove(key);
        let why = refusal(&v);
        assert!(why.contains(&format!("missing key {key}")), "{key}: {why}");
    }
    let mut v = good();
    v["excluded"][0].as_object_mut().unwrap().remove("at");
    assert!(refusal(&v).contains("exclusion: missing key at"));
}

#[test]
fn every_container_must_have_its_shape() {
    let cases = [
        (
            json!(["not", "an", "object"]),
            "the record is not an object",
        ),
        (
            {
                let mut v = good();
                v["members"] = json!({});
                v
            },
            "'members' is not a list",
        ),
        (
            {
                let mut v = good();
                v["members"] = json!(["charter"]);
                v
            },
            "a member is not an object",
        ),
        (
            {
                let mut v = good();
                v["members"][0]["needs"] = json!("charter");
                v
            },
            "member 'charter': 'needs' is not a list",
        ),
        (
            {
                let mut v = good();
                v["excluded"] = json!("none");
                v
            },
            "'excluded' is not a list",
        ),
        (
            {
                let mut v = good();
                v["excluded"] = json!([1]);
                v
            },
            "an exclusion is not an object",
        ),
    ];
    for (value, expected) in cases {
        let why = refusal(&value);
        assert!(why.contains(expected), "{expected}: {why}");
    }
}

#[test]
fn text_that_is_not_json_is_refused_rather_than_read_as_empty() {
    let why = Record::parse("{\"change\": ", "component-api-2")
        .unwrap_err()
        .to_string();
    assert!(why.contains("the record is not JSON"), "{why}");
}

#[test]
fn a_record_that_disagrees_with_its_filename_is_refused() {
    let mut v = good();
    v["change"] = json!("other");
    let why = refusal(&v);
    assert!(why.contains("the record calls itself other"), "{why}");
}

#[test]
fn the_slug_is_asked_about_before_anything_else() {
    let why = Record::parse("not even json", "../escape")
        .unwrap_err()
        .to_string();
    assert!(why.contains("is not a change name"), "{why}");
}

#[test]
fn the_name_rule_is_letters_digits_dot_underscore_dash_not_leading_with_a_dot_or_dash() {
    for ok in ["a", "A1", "component-api-2", "v1.2_x", "9"] {
        assert!(name_ok(ok), "{ok}");
    }
    for bad in ["", ".hidden", "-flag", "a/b", "..", "a b", "a\nb", "é"] {
        assert!(!name_ok(bad), "{bad:?}");
    }
}

// ---- strings: one contained line ---------------------------------------------------------

#[test]
fn an_empty_or_blank_why_is_refused() {
    for blank in ["", "   "] {
        let mut v = good();
        v["why"] = json!(blank);
        assert!(refusal(&v).contains("why: expected a non-empty string"));
    }
}

#[test]
fn a_why_that_is_not_a_string_is_refused_and_still_named() {
    let mut v = good();
    v["why"] = json!(["a", "b"]);
    let why = refusal(&v);
    assert!(
        why.contains("expected a non-empty string, got ['a', 'b']"),
        "{why}"
    );
}

#[test]
fn a_why_that_cannot_be_one_line_is_refused() {
    let mut v = good();
    v["why"] = json!("first\nsecond");
    let why = refusal(&v);
    assert!(why.contains("is not one plain line"), "{why}");
    assert!(!why.contains('\n'), "the refusal forged a line: {why:?}");
}

#[test]
fn a_why_a_little_longer_than_a_report_row_is_kept_whole() {
    let mut v = good();
    let long = "w".repeat(200);
    v["why"] = json!(long);
    let rec = Record::parse(&v.to_string(), "component-api-2").unwrap();
    assert_eq!(rec.why, long);
}

#[test]
fn a_why_longer_than_a_committed_value_may_be_is_refused() {
    let mut v = good();
    v["why"] = json!("w".repeat(TEXT_LIMIT + 1));
    assert!(refusal(&v).contains("is not one plain line"));
}

#[test]
fn an_exclusion_needs_its_reason_too() {
    let mut v = good();
    v["excluded"][0]["why"] = json!("");
    assert!(refusal(&v).contains("exclusion 'charter-slack': why"));
}

// ---- names and branches ------------------------------------------------------------------

#[test]
fn dot_github_is_a_real_repository_and_is_accepted() {
    let mut v = good();
    v["members"][0]["repo"] = json!(".github");
    v["members"][1]["needs"] = json!([".github"]);
    assert!(Record::parse(&v.to_string(), "component-api-2").is_ok());
}

#[test]
fn a_member_naming_a_path_is_refused() {
    for path in ["../elsewhere", "a/b", "/etc", ".."] {
        let mut v = good();
        v["members"][0]["repo"] = json!(path);
        v["members"][1]["needs"] = json!([]);
        let why = refusal(&v);
        assert!(
            why.contains("change 'component-api-2': member:"),
            "{path}: {why}"
        );
    }
}

#[test]
fn the_same_repo_twice_is_refused() {
    let mut v = good();
    v["members"][1]["repo"] = json!("charter");
    v["members"][1]["needs"] = json!([]);
    assert!(refusal(&v).contains("'charter' is a member twice"));
}

#[test]
fn a_member_that_is_also_excluded_is_refused() {
    let mut v = good();
    v["excluded"][0]["repo"] = json!("charter");
    assert!(refusal(&v).contains("'charter' is both a member and excluded"));
}

#[test]
fn a_branch_that_reaches_git_as_a_flag_is_refused() {
    let mut v = good();
    v["members"][0]["branch"] = json!("-b");
    let why = refusal(&v);
    assert!(why.contains("member 'charter'"), "{why}");
    assert!(why.contains("begins with '-'"), "{why}");
}

#[test]
fn a_branch_that_is_empty_not_a_string_or_not_one_line_is_refused() {
    for (branch, expected) in [
        (json!(""), "is not a branch name"),
        (json!(7), "7 is not a branch name"),
        (json!("a\u{2028}b"), "is not one plain line"),
    ] {
        let mut v = good();
        v["members"][0]["branch"] = branch;
        let why = refusal(&v);
        assert!(why.contains(expected), "{expected}: {why}");
        assert!(!why.contains('\u{2028}'), "{why:?}");
    }
}

#[test]
fn an_ordinary_branch_is_accepted() {
    assert_eq!(branch_refusal("change/component-api-2"), None);
    assert_eq!(branch_refusal(&"b".repeat(200)), None);
}

// ---- ordering: declared, refused when it cannot be true -----------------------------------

fn ordered(needs: &[(&str, &[&str])]) -> serde_json::Value {
    let mut v = good();
    v["excluded"] = json!([]);
    v["members"] = needs
        .iter()
        .map(|(repo, n)| json!({"repo": repo, "branch": "change/x", "needs": n}))
        .collect();
    v
}

#[test]
fn a_cycle_is_refused_naming_both_members() {
    let why = refusal(&ordered(&[("a", &["b"]), ("b", &["a"])]));
    assert!(why.contains("ordering cycle: 'a' → 'b' → 'a'"), "{why}");
}

#[test]
fn a_longer_cycle_names_every_member_in_it() {
    let why = refusal(&ordered(&[("a", &["b"]), ("b", &["c"]), ("c", &["a"])]));
    assert!(why.contains("'a' → 'b' → 'c' → 'a'"), "{why}");
}

#[test]
fn a_member_cannot_block_itself() {
    let why = refusal(&ordered(&[("a", &["a"])]));
    assert!(
        why.contains("member 'a' declares itself as its own blocker"),
        "{why}"
    );
}

#[test]
fn needs_may_only_name_a_member_of_this_change() {
    let why = refusal(&ordered(&[("a", &["ghost"])]));
    assert!(
        why.contains("member 'a' needs 'ghost', which is not a member of this change"),
        "{why}"
    );
}

#[test]
fn a_diamond_is_not_a_cycle() {
    let v = ordered(&[("a", &[]), ("b", &["a"]), ("c", &["a"]), ("d", &["b", "c"])]);
    assert!(Record::parse(&v.to_string(), "component-api-2").is_ok());
}

#[test]
fn blocked_is_derived_from_the_declaration_and_what_has_landed_and_names_the_blocker() {
    let v = ordered(&[("a", &[]), ("b", &["a"]), ("c", &["a", "b"])]);
    let rec = Record::parse(&v.to_string(), "component-api-2").unwrap();
    let nothing = BTreeSet::new();
    let blocked = rec.blocked(&nothing);
    assert_eq!(blocked.get("b"), Some(&vec!["a".to_string()]));
    assert_eq!(
        blocked.get("c"),
        Some(&vec!["a".to_string(), "b".to_string()])
    );
    assert!(!blocked.contains_key("a"));
    let a_landed: BTreeSet<String> = ["a".to_string()].into();
    let blocked = rec.blocked(&a_landed);
    assert!(!blocked.contains_key("b"));
    assert_eq!(blocked.get("c"), Some(&vec!["b".to_string()]));
}

#[test]
fn dependents_are_the_members_that_need_a_repo() {
    let v = ordered(&[("a", &[]), ("b", &["a"]), ("c", &["a"])]);
    let rec = Record::parse(&v.to_string(), "component-api-2").unwrap();
    assert_eq!(rec.dependents("a"), vec!["b", "c"]);
    assert!(rec.dependents("b").is_empty());
}

// ---- no refusal forges a line ------------------------------------------------------------

#[test]
fn no_refusal_can_forge_a_second_line() {
    let hostile = "x\n\u{1b}[31m✓ landed\r";
    let mut cases = Vec::new();
    let mut v = good();
    v["members"][1]["needs"] = json!([hostile]);
    cases.push(v);
    let mut v = good();
    v["members"][0]["branch"] = json!(hostile);
    cases.push(v);
    let mut v = good();
    v["change"] = json!(hostile);
    cases.push(v);
    let mut v = good();
    v[hostile] = json!(1);
    cases.push(v);
    let mut v = good();
    v["by"] = json!(hostile);
    cases.push(v);
    for value in cases {
        let why = refusal(&value);
        assert!(
            !why.contains(['\n', '\r', '\u{1b}']),
            "a refusal carried a control character through: {why:?}"
        );
    }
}

// ---- a record built in memory is held to the parser's rules --------------------------------

#[test]
fn a_record_built_in_memory_is_refused_for_what_a_read_would_refuse() {
    let mut rec = Record::parse(GOOD, "component-api-2").unwrap();
    assert_eq!(rec.validate(), Ok(()));
    rec.why = "one\nforged row".into();
    assert!(rec.validate().unwrap_err().0.contains("why"));
    let mut rec = Record::parse(GOOD, "component-api-2").unwrap();
    rec.members[1].needs = vec!["../x".into()];
    assert!(rec.validate().unwrap_err().0.contains("needs"));
    let mut rec = Record::parse(GOOD, "component-api-2").unwrap();
    rec.excluded[0].at = String::new();
    assert!(rec.validate().unwrap_err().0.contains("at"));
}

//! The operator's own ask and deny rules, read under every spelling of the program they name
//! (#1286).
//!
//! `purlis guard ask 'terraform apply *'` writes a host rule: a glob over the command **as
//! written**. A path to the binary, another case, a `VAR=` prefix or a wrapper, a quoted or
//! escaped word, a subshell or a substitution runs the same program, and is a string the glob
//! does not match, so the host would run it with no prompt. A deny rule is stepped around the
//! same way. [`crate::consentspelling`] closed that for the project's own consent rules (#1279);
//! this is the same backstop for every `Bash` rule the operator wrote, read with the **same
//! reading** ([`consentspelling::Reading`]) and the same walks over strings and heredocs a shell
//! runs, so a spelling one learns the other learns.
//!
//! # What is refused
//!
//! A segment whose program, read as the shell runs it, is the one a rule names — compared by
//! file name, ignoring case, and with the command line's two names (`purlis`, `charter`) taken
//! for each other — and whose words then match the rule, while the segment's source as written
//! matches no rule in that file at least as strict. And any such command inside a string or a
//! heredoc a shell runs, whose outer command the host reads alone.
//!
//! # What is left to the host
//!
//! - The spelling a rule matches: the host asks, or denies, itself.
//! - A mention of the program in data — an `echo`, a commit or pull request body, a note written
//!   through a heredoc — whose program is something else.
//! - A rule whose program is a wildcard (`*kubectl delete*`): it names no program to read every
//!   spelling of, and the host matches it against the whole line as written.
//! - `allow` rules, which only stop a prompt, and the project's own consent rules, which A7 and
//!   A7b judge more closely.
//!
//! **Fail-closed**: a spelling the host might also accept, a line the lexer cannot read, and a
//! rule in only one of the files the host may read still refuse, naming the spelling that is
//! certainly matched.

use std::path::{Path, PathBuf};

use crate::consentspelling::{self, Place};
use crate::scaffold::settings::{
    self, CONSENT_PATTERNS, PURLIS_CONSENT_PATTERNS, RETIRED_HANDOFF_PATTERNS,
};
use serde_json::Value;

use crate::{cliname, heredoc, pyjson, shellwrap};

/// The trace reason this refusal is tallied under.
pub const REASON: &str = "rule-spelling";

/// What a rule does to the command it matches.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Decision {
    Ask,
    Deny,
}

/// One `Bash` rule, as the glob the host matches.
#[derive(Debug, Clone)]
struct Rule {
    /// The glob, `Bash(…)` taken off; a legacy `prefix:*` read as `prefix *`.
    glob: String,
    decision: Decision,
    /// The program the rule names, or `None` when its first word is a wildcard or the rule is
    /// one of the project's consent rules — the rule then gates no other spelling here, though
    /// it still counts as one the host matches.
    program: Option<Program>,
}

/// The program a rule names.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Program {
    /// Its first word as the rule writes it — a bare name, a path, any case — which is the
    /// spelling the host matches and the one a refusal tells the chat to use.
    written: String,
    /// Its file name, case-folded, as a [`Reading`] names a program.
    name: String,
}

impl Program {
    /// Whether a segment's program, named `name` ([`Reading::name`]), is this one: the same
    /// file name in any case, or the command line under either of its names.
    fn is(&self, name: &str) -> bool {
        self.name == name || (cliname::is_installed(&self.name) && cliname::is_installed(name))
    }
}

/// One settings file the host may read for the call, and its rules.
struct File {
    /// Where it is, as the operator would find it: relative to the project.
    shown: String,
    rules: Vec<Rule>,
}

/// A rule a segment runs a command under: which file and which rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Hit {
    file: usize,
    rule: usize,
}

/// Why `cmd` is refused in the project at `plane`, or `None`: a segment runs a program an ask or
/// deny rule of the operator's names, spelt so the host's glob would not see it.
///
/// `anchors` are where the host's settings may come from: the session's start folder and the
/// call's `cwd`. Each one inside the project adds its layer's settings to the files read.
pub fn refusal(cmd: &str, plane: &Path, anchors: &[&Path]) -> Option<String> {
    Rules::in_force(plane, anchors).refusal(cmd)
}

/// The host's `Bash` ask and deny rules, file by file, as the guard reads them.
#[derive(Default)]
pub struct Rules {
    files: Vec<File>,
}

impl Rules {
    /// Every rule in the files the host may read for a call in the project at `plane`
    /// ([`files_in_force`]).
    pub fn in_force(plane: &Path, anchors: &[&Path]) -> Self {
        Self {
            files: files_in_force(plane, anchors),
        }
    }

    /// The rules of a Claude Code settings file, from its text; `shown` is where it is.
    pub fn of_claude_settings(shown: &str, text: &str) -> Self {
        Self::of(shown, claude_rules(text))
    }

    /// The rules of an `opencode.json`, from its text.
    pub fn of_opencode(text: &str) -> Self {
        Self::of(settings::OPENCODE, opencode_rules(text))
    }

    /// These rules and `other`'s, each file still on its own.
    #[must_use]
    pub fn and(mut self, other: Self) -> Self {
        self.files.extend(other.files);
        self
    }

    fn of(shown: &str, rules: Vec<Rule>) -> Self {
        let files = if rules.is_empty() {
            Vec::new()
        } else {
            vec![File {
                shown: shown.to_owned(),
                rules,
            }]
        };
        Self { files }
    }

    /// Why `cmd` is refused under these rules, or `None` ([`refusal`]).
    pub fn refusal(&self, cmd: &str) -> Option<String> {
        let files = &self.files;
        if !files
            .iter()
            .any(|f| f.rules.iter().any(|r| r.program.is_some()))
        {
            return None;
        }
        // Every substitution a shell runs, at every depth, is read once through the leak
        // guard's scanner (#1417) and handed to the arm that reads it; past its bounds the
        // call is too big to check, not read in part.
        let inward = crate::shellsubst::every_substitution(cmd);
        if inward.too_big {
            return Some(crate::guardcaps::too_deep_refusal());
        }
        // A segment as written, and a string a shell is handed whole, read all the way in.
        let look = |text: &str| hits(files, text, false);
        let look_inward = |text: &str| hits(files, text, true);
        let named = |name: &str| named(files, name);
        let stripped = heredoc::strip_reader_heredocs(cmd);
        let first = |found: &Found, source: &str| match found {
            Found::TooDeep => None,
            Found::Hits(hits) => Some(
                hits.iter()
                    .copied()
                    .find(|h| !asks(files, *h, source))
                    .unwrap_or(hits[0]),
            ),
        };
        // Where the command sits first (a substitution, a `case` branch, a function body), so
        // the refusal says so; then a string a shell runs; then how it is spelt.
        consentspelling::in_a_substitution_by(&inward, &|segments: &[Vec<String>]| {
            hits_in(files, consentspelling::readings_in(segments, &named))
        })
        .map(|found| (found, Place::Substitution))
        .or_else(|| consentspelling::in_a_branch_or_body_by(&stripped, &look))
        .map(|(found, place)| match first(&found, "") {
            Some(hit) => placed_refusal(files, hit, place),
            None => too_deep_refusal(),
        })
        .or_else(|| {
            consentspelling::in_a_shell_string_by(&stripped, &look_inward)
                .or_else(|| consentspelling::in_a_shells_heredoc_by(cmd, &look_inward))
                .map(|found| match first(&found, "") {
                    Some(hit) => shell_string_refusal(files, hit),
                    None => too_deep_refusal(),
                })
        })
        .or_else(|| {
            let asked = |source: &str, found: &Found| match found {
                Found::TooDeep => false,
                Found::Hits(hits) => hits.iter().all(|h| asks(files, *h, source)),
            };
            consentspelling::not_as_written_by(&stripped, &look, asked).map(|(found, source)| {
                match first(&found, &source) {
                    Some(hit) => spelling_refusal(files, hit),
                    None => too_deep_refusal(),
                }
            })
        })
    }
}

/// Every rule of every file that holds a command `text` runs — at every wrapper layer a rule
/// names, of every segment, and with `inward` in every substitution a shell runs, at any depth
/// ([`consentspelling::every_reading_in`]) — or `None` for none. A segment read through more
/// wrappers than the guard follows is a hit of its own ([`Found::TooDeep`]): it fails closed.
fn hits(files: &[File], text: &str, inward: bool) -> Option<Found> {
    let named = |name: &str| named(files, name);
    hits_in(
        files,
        consentspelling::every_reading_in(text, &named, inward),
    )
}

/// Whether a rule in `files` names the program `name`.
fn named(files: &[File], name: &str) -> bool {
    files
        .iter()
        .flat_map(|f| f.rules.iter())
        .any(|r| r.program.as_ref().is_some_and(|p| p.is(name)))
}

/// [`hits`] over readings already made, with whether one stood behind too many wrappers.
fn hits_in(
    files: &[File],
    (readings, too_deep): (Vec<consentspelling::Reading>, bool),
) -> Option<Found> {
    if too_deep {
        return Some(Found::TooDeep);
    }
    let mut found: Vec<Hit> = Vec::new();
    for it in &readings {
        for (file, f) in files.iter().enumerate() {
            for (rule, r) in f.rules.iter().enumerate() {
                let Some(program) = r.program.as_ref().filter(|p| p.is(&it.name)) else {
                    continue;
                };
                let line = format!("{} {}", program.written, it.words.join(" "));
                let hit = Hit { file, rule };
                if consentspelling::matches(&line, &r.glob) && !found.contains(&hit) {
                    found.push(hit);
                }
            }
        }
    }
    (!found.is_empty()).then_some(Found::Hits(found))
}

/// What a command holds for the rules.
enum Found {
    /// The rules it runs a program under.
    Hits(Vec<Hit>),
    /// More wrappers in front of a program than the guard reads through.
    TooDeep,
}

/// What a command with more wrappers in front of its program than the guard reads is told.
fn too_deep_refusal() -> String {
    format!(
        "This command runs its program through more than {} wrappers (`env`, `sudo`, \
         `timeout` …), more than the guard reads through to check your ask and deny rules. \
         Run it with fewer.",
        shellwrap::MAX_LAYERS
    )
}

/// Whether the host itself holds `source` back at least as strictly as the rule `hit`: a rule
/// in the same file whose glob matches the source as written.
fn asks(files: &[File], hit: Hit, source: &str) -> bool {
    let file = &files[hit.file];
    let needed = file.rules[hit.rule].decision;
    file.rules
        .iter()
        .any(|r| r.decision >= needed && consentspelling::matches(source, &r.glob))
}

/// What the rule `hit` does, said.
fn what_it_does(rule: &Rule) -> &'static str {
    match rule.decision {
        Decision::Ask => "asks you first",
        Decision::Deny => "denies it",
    }
}

/// What a command spelt so the rule does not see it is told.
fn spelling_refusal(files: &[File], hit: Hit) -> String {
    let file = &files[hit.file];
    let rule = &file.rules[hit.rule];
    let program = rule.program.as_ref().map_or("", |p| p.written.as_str());
    format!(
        "`{program} …` falls under the rule `{glob}` in {shown}, which {does}, and the host \
         matches that rule against the command as written, so this spelling (a path, another \
         case, a prefix or wrapper, a quote, a subshell or a substitution) slips past it. Spell \
         it as the rule does: `{program}`, unquoted, at the start of its command.",
        glob = rule.glob,
        shown = file.shown,
        does = what_it_does(rule),
    )
}

/// What a ruled command inside a string or a heredoc a shell runs is told.
fn shell_string_refusal(files: &[File], hit: Hit) -> String {
    let file = &files[hit.file];
    let rule = &file.rules[hit.rule];
    let program = rule.program.as_ref().map_or("", |p| p.written.as_str());
    format!(
        "`{program} …` is refused inside a string or a heredoc a shell runs (`sh -c '…'`, \
         `eval`, `bash <<'EOF'`). It falls under the rule `{glob}` in {shown}, which {does}, \
         and the host matches that rule against the outer command only. Run it directly, spelt \
         as the rule does: `{program}`, unquoted, at the start of its command.",
        glob = rule.glob,
        shown = file.shown,
        does = what_it_does(rule),
    )
}

/// What a ruled command the host's rule never sees, where it sits, is told.
fn placed_refusal(files: &[File], hit: Hit, place: Place) -> String {
    let file = &files[hit.file];
    let rule = &file.rules[hit.rule];
    let program = rule.program.as_ref().map_or("", |p| p.written.as_str());
    format!(
        "`{program} …` is refused {where_}. It falls under the rule `{glob}` in {shown}, which \
         {does}, and the host matches that rule against the command as written, which does not \
         start with it. Run it as a command of its own, spelt as the rule does: `{program}`, \
         unquoted, at the start.",
        where_ = place.said(),
        glob = rule.glob,
        shown = file.shown,
        does = what_it_does(rule),
    )
}

/// The settings files the host may read for a call in the project at `plane`: its Claude Code
/// files, shared and machine-local; each anchor's layer inside the project, shared and
/// machine-local; and opencode's `opencode.json`. A file that is missing or unreadable adds no
/// rules.
fn files_in_force(plane: &Path, anchors: &[&Path]) -> Vec<File> {
    let mut claude: Vec<PathBuf> = vec![plane.join(settings::SETTINGS), plane.join(LOCAL_SETTINGS)];
    for anchor in anchors.iter().filter(|a| !a.as_os_str().is_empty()) {
        if let Some(Some(layer)) = settings::layer_settings(plane, anchor) {
            let local = layer.with_file_name("settings.local.json");
            for file in [layer, local] {
                if !claude.contains(&file) {
                    claude.push(file);
                }
            }
        }
    }
    let shown = |path: &Path| {
        let canon = |p: &Path| p.canonicalize().unwrap_or_else(|_| p.to_path_buf());
        canon(path)
            .strip_prefix(canon(plane))
            .map_or_else(|_| path.display().to_string(), |p| p.display().to_string())
    };
    let mut files: Vec<File> = claude
        .iter()
        .map(|path| File {
            shown: shown(path),
            rules: claude_rules(&read(path)),
        })
        .collect();
    files.push(File {
        shown: settings::OPENCODE.to_owned(),
        rules: opencode_rules(&read(&plane.join(settings::OPENCODE))),
    });
    files.retain(|f| !f.rules.is_empty());
    files
}

/// Claude Code's machine-local settings file, beside the shared one.
const LOCAL_SETTINGS: &str = ".claude/settings.local.json";

/// The file at `path`, or nothing for one that is missing, unreadable, not a plain file or
/// larger than the guard reads on a tool call ([`settings::read_for_the_guard`]).
fn read(path: &Path) -> String {
    settings::read_for_the_guard(path).unwrap_or_default()
}

/// The `Bash` ask and deny rules in a Claude Code settings file's `text`, tolerating every
/// other shape, as [`crate::guardcmd::rules`] reads them.
fn claude_rules(text: &str) -> Vec<Rule> {
    let Some(doc) = pyjson::loads_strict(text) else {
        return Vec::new();
    };
    [("ask", Decision::Ask), ("deny", Decision::Deny)]
        .into_iter()
        .flat_map(|(bucket, decision)| {
            doc.get("permissions")
                .and_then(|p| p.get(bucket))
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .filter_map(move |rule| {
                    let glob = rule.strip_prefix("Bash(")?.strip_suffix(')')?;
                    Some(rule_of(glob, decision))
                })
                .collect::<Vec<_>>()
        })
        .collect()
}

/// The ask and deny globs in an `opencode.json`'s `permission.bash`, from its `text`.
fn opencode_rules(text: &str) -> Vec<Rule> {
    let Some(doc) = pyjson::loads_strict(text) else {
        return Vec::new();
    };
    let Some(bash) = doc
        .get("permission")
        .and_then(|p| p.get("bash"))
        .and_then(Value::as_object)
    else {
        return Vec::new();
    };
    bash.iter()
        .filter_map(|(glob, decision)| {
            let decision = match decision.as_str()? {
                "ask" => Decision::Ask,
                "deny" => Decision::Deny,
                _ => return None,
            };
            Some(rule_of(glob, decision))
        })
        .collect()
}

/// A rule read off its glob.
fn rule_of(glob: &str, decision: Decision) -> Rule {
    let glob = glob.trim();
    // Claude Code's legacy prefix form: `npm run test:*` is `npm run test` and anything after.
    let glob = glob
        .strip_suffix(":*")
        .map_or_else(|| glob.to_owned(), |prefix| format!("{prefix} *"));
    // The retired handoff rule too (#1444), **as the `ask` `init` wrote and nothing else**:
    // a project that still carries it has not asked for every spelling of a handoff to be
    // held to it. A `deny` on the same glob is an operator's own, which `init` never
    // wrote, and is held under every spelling as any deny is: the handoff guard no longer
    // refuses the other spellings, so nothing else would hold it.
    let retired_ask =
        decision == Decision::Ask && RETIRED_HANDOFF_PATTERNS.contains(&glob.as_str());
    let consent = CONSENT_PATTERNS.contains(&glob.as_str())
        || PURLIS_CONSENT_PATTERNS.contains(&glob.as_str())
        || retired_ask;
    let first = glob.split_whitespace().next().unwrap_or_default();
    let literal = !first.is_empty() && !first.contains(['*', '?', '[']);
    let program = (literal && !consent).then(|| Program {
        written: first.to_owned(),
        name: shellwrap::base_lower(first),
    });
    Rule {
        glob,
        decision,
        program,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_rule_names_its_program_unless_that_is_a_wildcard_or_a_consent_rule() {
        let named = |glob: &str| rule_of(glob, Decision::Ask).program.map(|p| p.name);
        assert_eq!(named("terraform apply *").as_deref(), Some("terraform"));
        assert_eq!(named("Terraform apply *").as_deref(), Some("terraform"));
        assert_eq!(
            named("/usr/local/bin/terraform apply *").as_deref(),
            Some("terraform")
        );
        assert_eq!(rule_of("*kubectl delete*", Decision::Ask).program, None);
        assert_eq!(rule_of("charter handoff *", Decision::Ask).program, None);
        assert_eq!(
            rule_of("purlis report *--yes*", Decision::Ask).program,
            None
        );
        assert_eq!(
            rule_of("helm uninstall:*", Decision::Deny).glob,
            "helm uninstall *"
        );
        // A fnmatch over the whole source, so the glob's own spelling is what the host reads.
        assert!(crate::pypath::fnmatch(
            "helm uninstall x",
            "helm uninstall *"
        ));
    }

    /// The rule `init` wrote for a handoff was an `ask`, and that one is left alone (#1444).
    /// A `deny` on the same glob is an operator's own, `init` never wrote one, and it is held
    /// under every spelling of the command, as any deny is.
    #[test]
    fn a_deny_on_the_retired_handoff_glob_is_held_under_every_spelling_and_its_ask_is_not() {
        let spelt_around = [
            "/usr/local/bin/purlis handoff beta <<'B'\nx\nB",
            "purlis 'handoff' beta <<'B'\nx\nB",
            "env purlis handoff beta <<'B'\nx\nB",
            "charter handoff beta <<'B'\nx\nB",
        ];
        let plain = "purlis handoff beta <<'B'\nx\nB";
        for deny in [
            "Bash(purlis handoff *)",
            "Bash(purlis handoff:*)",
            "Bash(charter handoff *)",
        ] {
            let rules = Rules::of_claude_settings(
                ".claude/settings.json",
                &format!(r#"{{"permissions": {{"deny": ["{deny}"]}}}}"#),
            );
            for cmd in spelt_around {
                if deny.contains("charter") && cmd.starts_with("charter") {
                    continue; // the spelling the rule itself matches: the host denies it.
                }
                assert!(rules.refusal(cmd).is_some(), "{deny}: {cmd:?} got past");
            }
        }
        // The spelling the deny matches is the host's own to deny.
        let rules = Rules::of_claude_settings(
            ".claude/settings.json",
            r#"{"permissions": {"deny": ["Bash(purlis handoff *)"]}}"#,
        );
        assert_eq!(rules.refusal(plain), None);

        // The `ask` on either retired glob is what `init` wrote: no spelling is held to it.
        let rules = Rules::of_claude_settings(
            ".claude/settings.json",
            r#"{"permissions": {"ask": ["Bash(purlis handoff *)", "Bash(charter handoff *)"]}}"#,
        );
        for cmd in spelt_around.into_iter().chain([plain]) {
            assert_eq!(rules.refusal(cmd), None, "{cmd:?}");
        }
        assert_eq!(rule_of("purlis handoff *", Decision::Ask).program, None);
        assert!(
            rule_of("purlis handoff *", Decision::Deny)
                .program
                .is_some()
        );

        // And the rule purlis tells a person to write, to have their harness ask as well, is
        // not the retired glob, so it is held under every spelling too.
        let rules = Rules::of_claude_settings(
            ".claude/settings.json",
            r#"{"permissions": {"ask": ["Bash(purlis handoff*)"]}}"#,
        );
        assert_eq!(rules.refusal(plain), None, "the host asks about it itself");
        assert!(
            rules
                .refusal("/usr/local/bin/purlis handoff beta <<'B'\nx\nB")
                .is_some()
        );
        assert!(rules.refusal("purlis 'handoff' beta <<'B'\nx\nB").is_some());
        assert!(crate::guardcmd::HANDOFF_RETIRED.contains("purlis guard ask 'purlis handoff*'"));
    }
}

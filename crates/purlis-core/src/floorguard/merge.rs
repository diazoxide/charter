//! The floor's merge half: every way a forge CLI or git can merge a request, or set one to merge
//! on its own later, is the same act to the floor as `gh pr merge` (#866).
//!
//! The rule is about the act, not one spelling of it. Unattended:
//!
//! * a forge API call to a **merge endpoint**, or one that **sets auto-merge or a merge queue**, is
//!   refused;
//! * a forge API call whose effect on a merge **cannot be read off the command line** is refused
//!   too. This fails closed: what charter cannot read, it treats as a merge;
//! * a **push option that sets auto-merge** is refused, however git is handed it;
//! * an **alias** is the command it stands for, and making one that stands for a held command,
//!   or for the start of one, is refused.
//!
//! A read is never refused. An API call that is a `GET` cannot merge anything, whatever endpoint
//! it names, so an unattended run can still read a request, its checks and whether it merged.
//! Attended, nothing here runs at all: [`super::release_floor_reason`] returns first.

use std::sync::OnceLock;

use regex::Regex;

use crate::shellseg;

/// The sentence a refusal of a merge or an auto-merge setting ends with.
pub(super) const MERGES: &str =
    "Merging a request, or setting one to merge on its own later, lands code.";

/// The sentence a refusal ends with when the call's effect on a merge cannot be read.
pub(super) const UNREADABLE: &str =
    "purlis cannot read what this call would do to a merge, so it is treated as one.";

/// The sentence a refusal of an alias ends with.
const ALIAS: &str = "An alias that would land code is the command it stands for.";

/// True when a word speaks of merging: a merge, a merge train, auto-merge, merge-when-ready.
pub(super) fn speaks_of_merging(word: &str) -> bool {
    word.to_lowercase().contains("merg")
}

/// A word the shell fills in or expands, so its value is not on the command line: a parameter or
/// command substitution, a glob, a brace expansion or a tilde.
pub(super) fn opaque(word: &str) -> bool {
    word.contains(['$', '`', '*', '?', '[', '~']) || brace_expansion_re().is_match(word)
}

/// `{a,b}` or `{1..3}`: a brace the shell expands. `{owner}` is a CLI placeholder, not one.
fn brace_expansion_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\{[^{}]*(,|\.\.)[^{}]*\}").expect("compiles"))
}

/// A forge CLI's own aliases of a verb the floor holds: `(cli, noun, alias, verb)`.
const VERB_ALIASES: &[(&str, &str, &str, &str)] = &[
    ("glab", "mr", "accept", "merge"),
    ("gh", "release", "new", "create"),
];

/// A forge CLI's spellings of a verb the floor already holds, mapped to that verb.
pub(super) fn canonical_verb<'a>(name: &str, noun: &str, verb: &'a str) -> &'a str {
    VERB_ALIASES
        .iter()
        .find(|(cli, n, alias, _)| *cli == name && *n == noun && *alias == verb)
        .map_or(verb, |row| row.3)
}

/// Commands, by their leading words, that the floor reads past the `PUBLISH_FORGE` table: the
/// raw API, alias-making, the commands that can print the forge token, and the verb that can
/// open a request with auto-merge on.
const HELD_BEYOND_THE_TABLE: &[(&str, &[&str])] = &[
    ("gh", &["api"]),
    ("gh", &["alias"]),
    ("gh", &["auth"]),
    ("gh", &["config", "get"]),
    ("glab", &["api"]),
    ("glab", &["alias"]),
    ("glab", &["auth"]),
    ("glab", &["config", "get"]),
    ("glab", &["mr", "create"]),
    ("glab", &["mr", "new"]),
];

/// Every held command of one CLI, by its leading words: the `PUBLISH_FORGE` rows and their
/// aliases, and [`HELD_BEYOND_THE_TABLE`]. An alias may not stand for one, or for its start.
fn held_commands(name: &str) -> Vec<Vec<&'static str>> {
    let table = super::PUBLISH_FORGE
        .iter()
        .filter(|(cli, _, _)| *cli == name)
        .map(|(_, noun, verb)| vec![*noun, *verb]);
    let aliases = VERB_ALIASES
        .iter()
        .filter(|(cli, ..)| *cli == name)
        .map(|(_, noun, alias, _)| vec![*noun, *alias]);
    let beyond = HELD_BEYOND_THE_TABLE
        .iter()
        .filter(|(cli, _)| *cli == name)
        .map(|(_, words)| words.to_vec());
    table.chain(aliases).chain(beyond).collect()
}

/// The refusal for a call the floor cannot read.
fn unreadable() -> Option<String> {
    Some(UNREADABLE.to_owned())
}

/// The positional words of a forge CLI's argv, with the value of a repository flag dropped, so
/// the noun and verb are read where the CLI reads them. Answers each word with its index in
/// `args`.
pub(super) fn forge_words(args: &[String]) -> Vec<(usize, &str)> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < args.len() {
        let a = args[i].as_str();
        if a == "-R" || a == "--repo" {
            i += 2;
            continue;
        }
        if !a.starts_with('-') {
            out.push((i, a));
        }
        i += 1;
    }
    out
}

/// The merge refusal for a `gh` or `glab` call, past the `PUBLISH_FORGE` table.
///
/// `cmd` is the whole command line, which a GraphQL query's quoting is read from.
pub(super) fn forge_reason(
    name: &str,
    args: &[String],
    cmd: &str,
    unattended: bool,
    depth: usize,
) -> Option<String> {
    let words = forge_words(args);
    let first = words.first().map(|w| w.1);
    let second = words.get(1).map(|w| w.1);
    match (first, second) {
        (Some("api"), _) => api_reason(&args[words[0].0 + 1..], cmd).map(str::to_owned),
        (Some("alias"), Some("import")) => unreadable(),
        (Some("alias"), Some("set")) => {
            alias_reason(name, &args[words[1].0 + 1..], unattended, depth)
        }
        (Some("mr"), Some("create" | "new")) if name == "glab" && sets_auto_merge(args) => {
            Some(MERGES.to_owned())
        }
        _ => None,
    }
}

/// `glab mr create --auto-merge`, in any spelling that leaves it on.
fn sets_auto_merge(args: &[String]) -> bool {
    args.iter().any(|a| match a.strip_prefix("--auto-merge") {
        Some("") => true,
        Some(v) => v
            .strip_prefix('=')
            .is_some_and(|v| !matches!(v, "0" | "f" | "F" | "false" | "FALSE" | "False")),
        None => false,
    })
}

/// `gh alias set` / `glab alias set`: refused when what the alias stands for would be, or when
/// it stands for the start of a held command, whose rest the caller would add.
fn alias_reason(name: &str, args: &[String], unattended: bool, depth: usize) -> Option<String> {
    let positional: Vec<&String> = args.iter().filter(|a| !a.starts_with('-')).collect();
    let expansion = match (positional.first(), positional.get(1)) {
        (Some(_), Some(e)) => e.as_str(),
        // No expansion on the line, or `-` (stdin): it cannot be read.
        _ => return unreadable(),
    };
    // A shell alias runs a shell command this reader does not follow, and an expansion the shell
    // fills in cannot be read.
    let shell = args.iter().any(|a| a == "--shell" || a == "-s");
    if shell || expansion.starts_with('!') || expansion.starts_with(['$', '`']) {
        return unreadable();
    }
    let expanded: Vec<String> = shellseg::segment_argv(expansion)
        .into_iter()
        .next()
        .unwrap_or_default();
    let words: Vec<&str> = forge_words(&expanded).into_iter().map(|w| w.1).collect();
    // A noun or verb the caller supplies (`$1`) could be any command.
    if words.iter().take(2).any(|w| w.contains('$')) {
        return unreadable();
    }
    let held = !words.is_empty()
        && held_commands(name)
            .iter()
            .any(|held| held.starts_with(&words[..]) || words.starts_with(held));
    let cmd = format!("{name} {expansion}");
    if words.is_empty() || held || super::floor(&cmd, unattended, depth + 1).is_some() {
        Some(ALIAS.to_owned())
    } else {
        None
    }
}

/// `gh api` / `glab api` value flags, short and long.
const API_SHORT_VALUE: &[char] = &['X', 'H', 'f', 'F', 'q', 't', 'p'];
const API_SHORT_BOOL: &[char] = &['i', 'h'];
const API_LONG_VALUE: &[&str] = &[
    "method",
    "header",
    "raw-field",
    "field",
    "input",
    "jq",
    "template",
    "hostname",
    "preview",
    "cache",
    "output",
    "form",
];
const API_LONG_BOOL: &[&str] = &["paginate", "slurp", "include", "silent", "verbose", "help"];

/// How an `api` call's field is sent: `-f` as a string, `-F` with its type inferred (and `@` read
/// from a file).
#[derive(Clone, Copy, PartialEq, Eq)]
enum FieldKind {
    Raw,
    Typed,
}

/// One `-f`/`-F` field, `key=value` as written.
struct Field {
    kind: FieldKind,
    key: String,
    value: String,
}

impl Field {
    fn new(kind: FieldKind, written: &str) -> Self {
        let (key, value) = written.split_once('=').unwrap_or((written, ""));
        Self {
            kind,
            key: key.to_owned(),
            value: value.to_owned(),
        }
    }
}

/// What one `api` call says about itself.
#[derive(Default)]
struct ApiCall {
    method: Option<String>,
    unknown_flag: bool,
    positionals: Vec<String>,
    fields: Vec<Field>,
    /// `--input` or `--form`: a body this reader does not see.
    opaque_body: bool,
}

impl ApiCall {
    fn parse(args: &[String]) -> Self {
        let mut call = ApiCall::default();
        let mut i = 0;
        while i < args.len() {
            let a = args[i].as_str();
            i += 1;
            if a == "--" {
                call.positionals.extend(args[i..].iter().cloned());
                break;
            }
            if let Some(long) = a.strip_prefix("--") {
                let (name, inline) = match long.split_once('=') {
                    Some((n, v)) => (n, Some(v.to_owned())),
                    None => (long, None),
                };
                if API_LONG_VALUE.contains(&name) {
                    let value = inline.or_else(|| {
                        i += 1;
                        args.get(i - 1).cloned()
                    });
                    call.take(name, value);
                } else if !API_LONG_BOOL.contains(&name) {
                    call.unknown_flag = true;
                }
                continue;
            }
            if a.len() > 1 && a.starts_with('-') {
                let chars: Vec<char> = a.chars().skip(1).collect();
                for (k, c) in chars.iter().enumerate() {
                    if API_SHORT_BOOL.contains(c) {
                        continue;
                    }
                    if API_SHORT_VALUE.contains(c) {
                        let rest: String = chars[k + 1..].iter().collect();
                        let rest = rest.strip_prefix('=').unwrap_or(&rest).to_owned();
                        let value = if rest.is_empty() {
                            i += 1;
                            args.get(i - 1).cloned()
                        } else {
                            Some(rest)
                        };
                        let name = match c {
                            'X' => "method",
                            'f' => "raw-field",
                            'F' => "field",
                            _ => "other",
                        };
                        call.take(name, value);
                    } else {
                        call.unknown_flag = true;
                    }
                    break;
                }
                continue;
            }
            call.positionals.push(a.to_owned());
        }
        call
    }

    fn take(&mut self, name: &str, value: Option<String>) {
        let value = value.unwrap_or_default();
        match name {
            "method" => self.method = Some(value),
            "raw-field" => self.fields.push(Field::new(FieldKind::Raw, &value)),
            "field" => self.fields.push(Field::new(FieldKind::Typed, &value)),
            "input" | "form" => self.opaque_body = true,
            _ => {}
        }
    }

    /// True when the call is a read: an explicit `GET`/`HEAD`, or no method and no body, which
    /// both CLIs send as a `GET`. A flag this reader does not know could change either, so a call
    /// carrying one is not a read.
    fn is_read(&self) -> bool {
        if self.unknown_flag {
            return false;
        }
        match &self.method {
            Some(m) => m.eq_ignore_ascii_case("GET") || m.eq_ignore_ascii_case("HEAD"),
            None => self.fields.is_empty() && !self.opaque_body,
        }
    }
}

/// An endpoint as lower-case, percent-decoded path segments, with any scheme, host, query and
/// fragment taken off.
fn segments(endpoint: &str) -> Vec<String> {
    let path = endpoint.split(['?', '#']).next().unwrap_or("");
    let path = match path.split_once("://") {
        Some((_, rest)) => rest.split_once('/').map_or("", |(_, p)| p),
        None => path,
    };
    path.split('/')
        .filter(|s| !s.is_empty())
        .map(|s| percent_decode(s).to_lowercase())
        .collect()
}

/// `%XX` decoded; anything that is not a valid escape is kept as written.
fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && let Some(b) = s
                .get(i + 1..i + 3)
                .and_then(|h| u8::from_str_radix(h, 16).ok())
        {
            out.push(b);
            i += 3;
            continue;
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Where a path's repository or project ends: the segments after `repos/<owner>/<name>`,
/// `repositories/<id>` or `projects/<id>`, or `None` when the path names none.
fn under_repository(segs: &[String]) -> Option<&[String]> {
    let at = segs
        .iter()
        .position(|s| matches!(s.as_str(), "repos" | "repositories" | "projects"))?;
    let names = if segs[at] == "repos" { 2 } else { 1 };
    Some(segs.get(at + 1 + names..).unwrap_or(&[]))
}

/// True when a path is a merge endpoint: a merge of a pull or merge request, a branch merge, or
/// a merge train, by the forges' own endpoint shapes. A ref, file or branch that is merely named
/// for a merge is not one. A path that names no repository is judged by any segment that speaks
/// of merging, since its shape is not one this reader knows.
fn is_merge_path(segs: &[String]) -> bool {
    let Some(rest) = under_repository(segs) else {
        return segs.iter().any(|s| speaks_of_merging(s));
    };
    match rest.first().map(String::as_str) {
        Some("pulls" | "merge_requests") => rest.get(2).is_some_and(|s| speaks_of_merging(s)),
        Some(collection) => speaks_of_merging(collection),
        None => false,
    }
}

/// True when a path is a pull or merge request itself, where a body could set auto-merge. Its
/// reviews, notes and other sub-paths cannot.
fn is_request_path(segs: &[String]) -> bool {
    under_repository(segs).is_some_and(|rest| {
        rest.len() == 2 && matches!(rest[0].as_str(), "pulls" | "merge_requests")
    })
}

/// True when a path is a repository or project itself, whose settings include auto-merge.
fn is_repository(segs: &[String]) -> bool {
    under_repository(segs).is_some_and(|rest| rest.is_empty())
}

/// The refusal for one `gh api` / `glab api` call, or `None` when it cannot merge.
fn api_reason(args: &[String], cmd: &str) -> Option<&'static str> {
    let call = ApiCall::parse(args);
    let read = call.is_read();
    if !read && call.positionals.is_empty() {
        return Some(UNREADABLE);
    }
    for endpoint in &call.positionals {
        let segs = segments(endpoint);
        if segs.iter().any(|s| s == "graphql") {
            if let Some(why) = graphql_reason(&call, cmd) {
                return Some(why);
            }
            continue;
        }
        if read {
            continue;
        }
        if opaque(endpoint) || segs.iter().any(|s| s == "." || s == "..") {
            return Some(UNREADABLE);
        }
        if is_merge_path(&segs) {
            return Some(MERGES);
        }
        let settable = is_request_path(&segs) || is_repository(&segs);
        for field in &call.fields {
            // A key the shell fills in could be an auto-merge setting where one can be set.
            if opaque(&field.key) && settable {
                return Some(UNREADABLE);
            }
            if speaks_of_merging(&field.key) {
                return Some(MERGES);
            }
        }
        if call.opaque_body && settable {
            return Some(UNREADABLE);
        }
    }
    if !read && call.method.as_deref().is_some_and(opaque) {
        return Some(UNREADABLE);
    }
    None
}

/// `$name` in a GraphQL document, with what stands before it and whether a `:` follows.
fn variable_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(\S?)\s*\$([A-Za-z_][A-Za-z0-9_]*)(\s*:)?").expect("compiles"))
}

/// The refusal for a GraphQL call, or `None` when its query is readable and changes no merge.
fn graphql_reason(call: &ApiCall, cmd: &str) -> Option<&'static str> {
    if call.opaque_body {
        return Some(UNREADABLE);
    }
    let mut query = String::new();
    for field in &call.fields {
        if !field.key.eq_ignore_ascii_case("query") {
            continue;
        }
        if field.kind == FieldKind::Typed && field.value.starts_with('@') {
            return Some(UNREADABLE);
        }
        query.push_str(&field.value);
        query.push('\n');
    }
    if !query.contains('{') || query.contains("$(") || query.contains("${") || query.contains('`') {
        return Some(UNREADABLE);
    }
    // The lexer keeps a `\$` that bash would unescape inside double quotes; the document has `$`.
    let query = query.replace("\\$", "$");
    if query.contains('$')
        && (!variables_stand_in_value_positions(&query) || shell_expands_query(cmd))
    {
        return Some(UNREADABLE);
    }
    let lower = query.to_lowercase();
    if lower.contains("mutation") && (speaks_of_merging(&lower) || lower.contains("queue")) {
        return Some(MERGES);
    }
    None
}

/// True when every `$` in a GraphQL document is a variable the document declares, declared in its
/// operation's variable list and used only where a value goes. Anything else is text the shell
/// could have put there.
fn variables_stand_in_value_positions(query: &str) -> bool {
    let mut declared = Vec::new();
    let mut used = Vec::new();
    let mut seen = 0;
    for m in variable_re().captures_iter(query) {
        seen += 1;
        let before = m.get(1).map_or("", |b| b.as_str());
        let name = m.get(2).map_or("", |n| n.as_str());
        if m.get(3).is_some() {
            if !matches!(before, "(" | ",") {
                return false;
            }
            declared.push(name);
        } else {
            if !matches!(before, ":" | "[" | ",") {
                return false;
            }
            used.push(name);
        }
    }
    seen == query.matches('$').count() && used.iter().all(|u| declared.contains(u))
}

/// True when a `$` in a `query=` word of the command line is one the shell expands: written
/// outside single quotes and not escaped. A line the lexer cannot read counts as one.
fn shell_expands_query(cmd: &str) -> bool {
    let Ok(toks) = shellseg::lex(cmd) else {
        return true;
    };
    let chars: Vec<char> = cmd.chars().collect();
    toks.iter()
        .filter(|t| t.text.to_lowercase().contains("query=") && t.text.contains('$'))
        .any(|t| {
            let (Ok(start), Ok(end)) = (usize::try_from(t.start), usize::try_from(t.end)) else {
                return true;
            };
            expanding_dollar(chars.get(start..end).unwrap_or(&[]))
        })
}

/// True when the raw text of one word holds a `$` the shell expands.
fn expanding_dollar(raw: &[char]) -> bool {
    let mut single = false;
    let mut double = false;
    let mut i = 0;
    while i < raw.len() {
        let c = raw[i];
        if single {
            single = c != '\'';
        } else if c == '\\' {
            i += 1;
        } else if c == '\'' && !double {
            single = true;
        } else if c == '"' {
            double = !double;
        } else if c == '$' {
            return true;
        }
        i += 1;
    }
    false
}

/// True when a push option sets a request to merge: its key, past GitLab's `merge_request.`
/// namespace, speaks of merging.
fn is_merge_push_option(opt: &str) -> bool {
    let key = opt.split('=').next().unwrap_or("").to_lowercase();
    let key = key
        .strip_prefix("merge_request.")
        .or_else(|| key.strip_prefix("mr."))
        .unwrap_or(&key);
    speaks_of_merging(key)
}

/// The refusal for one push option. Its key decides, since a value such as a request's title
/// sets nothing; a key the shell fills in, or a value a substitution writes, cannot be read.
fn push_option_reason(opt: &str) -> Option<&'static str> {
    // A substitution in the value runs a command whose output is the option.
    if opaque(opt.split('=').next().unwrap_or("")) || opt.contains("$(") || opt.contains('`') {
        Some(UNREADABLE)
    } else if is_merge_push_option(opt) {
        Some(MERGES)
    } else {
        None
    }
}

/// Shell-quotes one word, so a resolved alias is handed back as the words it was.
fn quoted(word: &str) -> String {
    format!("'{}'", word.replace('\'', r"'\''"))
}

/// What a git segment's subcommand is, when it may be an alias defined on the line.
pub(super) enum GitAlias {
    /// No alias defined on the line is run here.
    None,
    /// The alias (`-c alias.<name>=…`) resolved to the command it stands for.
    Resolved(String),
    /// An alias this reader cannot follow: a shell alias, an expansion the shell fills in, or
    /// one taken from the environment (`GIT_CONFIG_*`, `--config-env`).
    Unreadable,
}

/// The git alias this segment runs, if one is defined on the line.
pub(super) fn git_alias(args: &[String], env: &[String], sub: Option<&str>) -> GitAlias {
    let Some(sub) = sub else {
        return GitAlias::None;
    };
    if env.iter().any(|e| {
        e.split_once('=').is_some_and(|(k, v)| {
            k.starts_with("GIT_CONFIG") && v.to_lowercase().starts_with("alias.")
        })
    }) {
        return GitAlias::Unreadable;
    }
    let Some(sub_at) = args.iter().position(|a| a == sub) else {
        return GitAlias::None;
    };
    let globals = &args[..sub_at];
    for (i, a) in globals.iter().enumerate() {
        let config_env = match a.strip_prefix("--config-env=") {
            Some(v) => Some(v),
            None if a == "--config-env" => globals.get(i + 1).map(String::as_str),
            None => None,
        };
        if config_env.is_some_and(|v| v.to_lowercase().starts_with("alias.")) {
            return GitAlias::Unreadable;
        }
    }
    let mut i = 0;
    while i + 1 < globals.len() {
        if globals[i] == "-c"
            && let Some((key, expansion)) = globals[i + 1].split_once('=')
            && let Some(name) = key.to_lowercase().strip_prefix("alias.")
            && name == sub.to_lowercase()
        {
            if expansion.starts_with('!') || opaque(expansion) {
                return GitAlias::Unreadable;
            }
            let mut words: Vec<String> = Vec::new();
            words.extend(globals[..i].iter().map(|w| quoted(w)));
            words.extend(globals[i + 2..].iter().map(|w| quoted(w)));
            words.push(expansion.to_owned());
            words.extend(args[sub_at + 1..].iter().map(|w| quoted(w)));
            return GitAlias::Resolved(format!("git {}", words.join(" ")));
        }
        i += 1;
    }
    GitAlias::None
}

/// The merge refusal for a `git` call: a push option that sets auto-merge, given on the command
/// line, through `-c`, through the `GIT_CONFIG_*` environment, or written into git's config; and
/// a git alias written into git's config that would publish.
pub(super) fn git_reason(
    args: &[String],
    env: &[String],
    sub: Option<&str>,
    unattended: bool,
    depth: usize,
) -> Option<String> {
    let sub_at = sub.and_then(|s| args.iter().position(|a| a == s));
    let globals = &args[..sub_at.unwrap_or(args.len())];
    let rest = sub_at.map_or(&[][..], |i| &args[i + 1..]);
    match sub {
        Some("push") => {
            let mut i = 0;
            while i < globals.len() {
                let a = globals[i].as_str();
                if a == "-c"
                    && let Some(kv) = globals.get(i + 1)
                {
                    if let Some((k, v)) = kv.split_once('=')
                        && k.eq_ignore_ascii_case("push.pushoption")
                        && let Some(why) = push_option_reason(v)
                    {
                        return Some(why.to_owned());
                    }
                    i += 2;
                    continue;
                }
                // `--config-env` takes the value from a variable this reader does not see.
                let config_env = match a.strip_prefix("--config-env=") {
                    Some(v) => Some(v),
                    None if a == "--config-env" => globals.get(i + 1).map(String::as_str),
                    None => None,
                };
                if config_env.is_some_and(|v| v.to_lowercase().starts_with("push.pushoption")) {
                    return unreadable();
                }
                i += 1;
            }
            if let Some(why) = env_reason(env) {
                return Some(why.to_owned());
            }
            push_options(rest)
                .iter()
                .find_map(|o| push_option_reason(o))
                .map(str::to_owned)
        }
        Some("config") => config_write_reason(rest, unattended, depth),
        _ => None,
    }
}

/// The git subcommands an alias may not stand for or begin with, because the floor holds them.
const HELD_GIT: &[&str] = &["push", "tag"];

/// `git config` writing a push option that sets auto-merge, or an alias that would publish.
fn config_write_reason(rest: &[String], unattended: bool, depth: usize) -> Option<String> {
    let key_at = rest.iter().position(|a| {
        let a = a.to_lowercase();
        a == "push.pushoption" || a.starts_with("alias.")
    })?;
    // The value is the word after the key, even one that starts like an option: an alias's
    // expansion may begin with git's own global options.
    let value = rest.get(key_at + 1)?;
    if rest[key_at].eq_ignore_ascii_case("push.pushoption") {
        return push_option_reason(value).map(str::to_owned);
    }
    if value.starts_with('!') || opaque(value) {
        return unreadable();
    }
    // The subcommand the alias runs, past any global options it carries.
    let words: Vec<String> = value.split_whitespace().map(str::to_owned).collect();
    let sub = crate::credguard::git_subcommand(&words).unwrap_or_default();
    let cmd = format!("git {value}");
    if HELD_GIT.contains(&sub.to_lowercase().as_str())
        || super::floor(&cmd, unattended, depth + 1).is_some()
    {
        Some(ALIAS.to_owned())
    } else {
        None
    }
}

/// `GIT_CONFIG_*` assignments in front of a push that name `push.pushOption`.
fn env_reason(env: &[String]) -> Option<&'static str> {
    let config: Vec<&str> = env
        .iter()
        .filter_map(|e| e.split_once('='))
        .filter(|(k, _)| k.starts_with("GIT_CONFIG"))
        .map(|(_, v)| v)
        .collect();
    if !config
        .iter()
        .any(|v| v.to_lowercase().contains("pushoption"))
    {
        return None;
    }
    for v in config {
        if opaque(v) {
            return Some(UNREADABLE);
        }
        let lower = v
            .to_lowercase()
            .replace("merge_requests", "")
            .replace("merge_request", "");
        if speaks_of_merging(&lower) {
            return Some(MERGES);
        }
    }
    None
}

/// Every push option on a `git push` argv: `-o x`, `-ox`, `-uo x`, `--push-option[=]x`.
fn push_options(args: &[String]) -> Vec<String> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < args.len() {
        let a = args[i].as_str();
        i += 1;
        if a == "--" {
            break;
        }
        if let Some(v) = a.strip_prefix("--push-option=") {
            out.push(v.to_owned());
        } else if a == "--push-option" {
            if let Some(v) = args.get(i) {
                out.push(v.clone());
                i += 1;
            }
        } else if !a.starts_with("--")
            && a.len() > 1
            && let Some(cluster) = a.strip_prefix('-')
            && let Some((_, v)) = cluster.split_once('o')
        {
            if v.is_empty() {
                if let Some(v) = args.get(i) {
                    out.push(v.clone());
                    i += 1;
                }
            } else {
                out.push(v.to_owned());
            }
        }
    }
    out
}

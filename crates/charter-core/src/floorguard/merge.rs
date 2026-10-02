//! The floor's merge half: every way a forge CLI or git can merge a request, or set one to merge
//! on its own later, is the same act to the floor as `gh pr merge` (V41).
//!
//! The rule is about the act, not one spelling of it. Unattended:
//!
//! * a forge API call to a **merge endpoint**, or one that **sets auto-merge or a merge queue**, is
//!   refused;
//! * a forge API call whose effect on a merge **cannot be read off the command line** is refused
//!   too. This fails closed: what charter cannot read, it treats as a merge;
//! * a **push option that sets auto-merge** is refused, however git is handed it;
//! * a forge CLI's own **aliases** of a verb, and an alias made to stand for a merge, are the
//!   command they stand for.
//!
//! A read is never refused. An API call that is a `GET` cannot merge anything, whatever endpoint
//! it names, so an unattended run can still read a request, its checks and whether it merged.
//! Attended, nothing here runs at all: [`super::release_floor_reason`] returns first.

use std::sync::OnceLock;

use regex::Regex;

/// The sentence a refusal of a merge or an auto-merge setting ends with.
pub(super) const MERGES: &str =
    "Merging a request, or setting one to merge on its own later, lands code.";

/// The sentence a refusal ends with when the call's effect on a merge cannot be read.
pub(super) const UNREADABLE: &str =
    "charter cannot read what this call would do to a merge, so it is treated as one.";

/// The sentence a refusal of an alias ends with.
const ALIAS: &str = "An alias that would land code is the command it stands for.";

/// A forge CLI's spellings of a verb the floor already holds, mapped to that verb.
pub(super) fn canonical_verb<'a>(name: &str, noun: &str, verb: &'a str) -> &'a str {
    match (name, noun, verb) {
        ("glab", "mr", "accept") => "merge",
        ("gh", "release", "new") => "create",
        _ => verb,
    }
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
pub(super) fn forge_reason(name: &str, args: &[String]) -> Option<String> {
    let words = forge_words(args);
    let first = words.first().map(|w| w.1);
    let second = words.get(1).map(|w| w.1);
    match (first, second) {
        (Some("api"), _) => api_reason(&args[words[0].0 + 1..]).map(str::to_owned),
        (Some("alias"), Some("import")) => Some(UNREADABLE.to_owned()),
        (Some("alias"), Some("set")) => alias_reason(name, &args[words[1].0 + 1..]),
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
        Some(v) => match v.strip_prefix('=') {
            Some(v) => !matches!(v, "0" | "f" | "F" | "false" | "FALSE" | "False"),
            None => false,
        },
        None => false,
    })
}

/// `gh alias set` / `glab alias set`: the alias is refused when what it stands for would be.
fn alias_reason(name: &str, args: &[String]) -> Option<String> {
    let shell = args.iter().any(|a| a == "--shell" || a == "-s");
    let positional: Vec<&String> = args.iter().filter(|a| !a.starts_with('-')).collect();
    let expansion = match (positional.first(), positional.get(1)) {
        (Some(_), Some(e)) => e.as_str(),
        // No expansion on the line, or `-` (stdin): it cannot be read.
        _ => return Some(UNREADABLE.to_owned()),
    };
    if expansion.starts_with('$') || expansion.starts_with('`') {
        return Some(UNREADABLE.to_owned());
    }
    let cmd = match expansion.strip_prefix('!') {
        Some(rest) => rest.to_owned(),
        None if shell => expansion.to_owned(),
        None => format!("{name} {expansion}"),
    };
    super::release_floor_reason(&cmd, true).map(|_| ALIAS.to_owned())
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

/// What one `api` call says about itself.
#[derive(Default)]
struct ApiCall {
    method: Option<String>,
    unknown_flag: bool,
    positionals: Vec<String>,
    /// `-f`/`-F` values, `key=value` as written; the bool is true for a typed (`-F`) field.
    fields: Vec<(bool, String)>,
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
                    let value = match inline {
                        Some(v) => Some(v),
                        None => {
                            i += 1;
                            args.get(i - 1).cloned()
                        }
                    };
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
        match name {
            "method" => self.method = Some(value.unwrap_or_default()),
            "raw-field" => self.fields.push((false, value.unwrap_or_default())),
            "field" => self.fields.push((true, value.unwrap_or_default())),
            "input" | "form" => self.opaque_body = true,
            _ => {}
        }
    }

    /// True when the call is a read: an explicit `GET`/`HEAD`, or no method and no body, which
    /// both CLIs send as a `GET`.
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

/// A word the shell fills in, so its value is not on the command line.
fn opaque(s: &str) -> bool {
    s.contains('$') || s.contains('`') || s.contains('*')
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

/// True when a path names a merge endpoint: a segment that speaks of merging, other than the
/// owner, repository or project it is under and GitLab's `merge_requests` collection itself.
fn is_merge_path(segs: &[String]) -> bool {
    let mut skip = 0;
    for s in segs {
        if skip > 0 {
            skip -= 1;
            continue;
        }
        match s.as_str() {
            "repos" => skip = 2,
            "projects" | "groups" | "users" | "orgs" | "namespaces" => skip = 1,
            "merge_requests" => {}
            _ if s.contains("merg") => return true,
            _ => {}
        }
    }
    false
}

/// True when a path is under a pull or merge request, where a body could set auto-merge.
fn is_request_path(segs: &[String]) -> bool {
    segs.iter().any(|s| s == "merge_requests" || s == "pulls")
}

/// True when a path is a repository or project itself, whose settings include auto-merge.
fn is_repository(segs: &[String]) -> bool {
    let start = segs
        .iter()
        .position(|s| s == "repos" || s == "projects")
        .unwrap_or(segs.len());
    match segs.get(start).map(String::as_str) {
        Some("repos") => segs.len() == start + 3,
        Some("projects") => segs.len() == start + 2,
        _ => false,
    }
}

/// The refusal for one `gh api` / `glab api` call, or `None` when it cannot merge.
fn api_reason(args: &[String]) -> Option<&'static str> {
    let call = ApiCall::parse(args);
    let read = call.is_read();
    for endpoint in &call.positionals {
        let segs = segments(endpoint);
        if segs.last().is_some_and(|s| s == "graphql") {
            if let Some(why) = graphql_reason(&call) {
                return Some(why);
            }
            continue;
        }
        if read {
            continue;
        }
        if opaque(endpoint) {
            return Some(UNREADABLE);
        }
        if is_merge_path(&segs) {
            return Some(MERGES);
        }
        for (_, field) in &call.fields {
            let key = field.split('=').next().unwrap_or("");
            // A key the shell fills in could be an auto-merge setting where one can be set.
            if opaque(key) && (is_request_path(&segs) || is_repository(&segs)) {
                return Some(UNREADABLE);
            }
            if key.to_lowercase().contains("merg") {
                return Some(MERGES);
            }
        }
        if call.opaque_body && is_request_path(&segs) {
            return Some(UNREADABLE);
        }
    }
    if !read && call.method.as_deref().is_some_and(opaque) {
        return Some(UNREADABLE);
    }
    None
}

/// `$name` in a GraphQL document.
fn variable_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\$([A-Za-z_][A-Za-z0-9_]*)(\s*:)?").expect("compiles"))
}

/// The refusal for a GraphQL call, or `None` when its query is readable and changes no merge.
fn graphql_reason(call: &ApiCall) -> Option<&'static str> {
    if call.opaque_body {
        return Some(UNREADABLE);
    }
    let mut query = String::new();
    for (typed, field) in &call.fields {
        let (key, value) = field.split_once('=').unwrap_or((field.as_str(), ""));
        if !key.eq_ignore_ascii_case("query") {
            continue;
        }
        if *typed && value.starts_with('@') {
            return Some(UNREADABLE);
        }
        query.push_str(value);
        query.push('\n');
    }
    if !query.contains('{') || query.contains("$(") || query.contains("${") || query.contains('`') {
        return Some(UNREADABLE);
    }
    // Every `$name` has to be a variable the document declares. One that is not is text the shell
    // put there, and it could be anything.
    let mut declared = Vec::new();
    let mut used = Vec::new();
    for m in variable_re().captures_iter(&query) {
        let name = m.get(1).map_or("", |n| n.as_str());
        if m.get(2).is_some() {
            declared.push(name);
        } else {
            used.push(name);
        }
    }
    let dollars = query.matches('$').count();
    if dollars != declared.len() + used.len() || used.iter().any(|u| !declared.contains(u)) {
        return Some(UNREADABLE);
    }
    let lower = query.to_lowercase();
    if lower.contains("mutation") && (lower.contains("merg") || lower.contains("queue")) {
        return Some(MERGES);
    }
    None
}

/// True when a push option sets a request to merge: its key, past GitLab's `merge_request.`
/// namespace, speaks of merging.
fn is_merge_push_option(opt: &str) -> bool {
    let key = opt.split('=').next().unwrap_or("").to_lowercase();
    let key = key
        .strip_prefix("merge_request.")
        .or_else(|| key.strip_prefix("mr."))
        .unwrap_or(&key);
    key.contains("merg")
}

/// The refusal for one push option's value.
fn push_option_reason(opt: &str) -> Option<&'static str> {
    if opaque(opt) {
        Some(UNREADABLE)
    } else if is_merge_push_option(opt) {
        Some(MERGES)
    } else {
        None
    }
}

/// The merge refusal for a `git` call: a push option that sets auto-merge, given on the command
/// line, through `-c`, through the `GIT_CONFIG_*` environment, or written into git's config.
pub(super) fn git_reason(args: &[String], env: &[String], sub: Option<&str>) -> Option<String> {
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
                    return Some(UNREADABLE.to_owned());
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
        Some("config") => {
            let key_at = rest
                .iter()
                .position(|a| a.eq_ignore_ascii_case("push.pushoption"))?;
            rest[key_at + 1..]
                .iter()
                .filter(|a| !a.starts_with('-'))
                .find_map(|v| push_option_reason(v))
                .map(str::to_owned)
        }
        _ => None,
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
        if lower.contains("merg") {
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

//! **Brokered git reads none of your git config, and says so when it matters** (#1413).
//!
//! A clone or a worktree the app makes for a sandboxed chat runs git with no global and no
//! system config ([`crate::worktree::git::Isolated`], D-1335-7): the person's config routinely
//! defines the very programs a repository could ask git to run outside the chat's sandbox. That
//! is the right default, and it makes such a clone differ from the same clone in a terminal in
//! two ways people meet:
//!
//! - **a filter a repository's `.gitattributes` names is not run** — Git LFS above all — so the
//!   checkout holds what is stored, pointer files for LFS ([`filters_named`], [`filter_notes`]);
//! - **network settings do not apply** — a proxy, a certificate authority, a `url.*.insteadOf`
//!   rewrite — so a clone can fail where the terminal's succeeds ([`network_note`]).
//!
//! **Your config is read here only to tell you, never applied** (security: a chat must not
//! steer it). [`global_entries`] lists it through the hardened runner, and what it answers is
//! matched against the clone's URL by key name alone: the note names which keys would have
//! changed the route, never a value (a proxy URL or a rewrite base can carry a credential).

use std::path::{Path, PathBuf};

use crate::worktree::git;

/// The filters a `.gitattributes` text turns on (`filter=<name>`), each once, in the order
/// first named. A comment, an unset (`-filter`) or a reset (`!filter`) turns none on.
pub fn filters_named(attributes: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for line in attributes.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        for word in line.split_whitespace().skip(1) {
            if let Some(name) = word.strip_prefix("filter=")
                && !name.is_empty()
                && !out.iter().any(|seen| seen == name)
            {
                out.push(name.to_owned());
            }
        }
    }
    out
}

/// What a checkout at `top`, made by brokered git, says of the filters its `.gitattributes`
/// names: one sentence per filter, none where it names none. `name` is how the checkout is
/// called in the answer, `at` where it is, as the person would `cd` to it.
///
/// Only the top `.gitattributes` is read: it is where a repository turns LFS on. It is read
/// as **committed** ([`committed_attributes`]), never from the working tree a chat writes.
pub fn filter_notes(top: &Path, name: &str, at: &str) -> Vec<String> {
    let Some(text) = committed_attributes(top) else {
        return Vec::new();
    };
    filters_named(&text)
        .into_iter()
        .map(|filter| {
            if filter == "lfs" {
                format!(
                    "{name} keeps some files in Git LFS, and git run by the app for a chat runs \
                     no LFS filter (it reads none of your git config), so those files were \
                     checked out as small pointer files. Run `git lfs pull` in {at} to fetch \
                     them."
                )
            } else {
                format!(
                    "{name}'s .gitattributes names the `{}` filter, which git run by the app \
                     for a chat does not run (it reads none of your git config), so the files \
                     it covers were checked out as stored in the repository.",
                    // A name is one word; a backtick in it would close the quote around it.
                    crate::shown::short(&filter.replace('`', ""))
                )
            }
        })
        .collect()
}

/// The most of a committed `.gitattributes` read: far past any real one.
pub const ATTRIBUTES_AT_MOST: u64 = 64 * 1024;

/// The top `.gitattributes` of the checkout at `top` **as its `HEAD` commits it**, asked of git
/// through the hardened runner: the app reads it outside the chat's sandbox, and the working
/// tree is the chat's to write, so a file there could be a link to anything the app can read,
/// a pipe that never ends, or a file without end (#1413). `None` where `HEAD` holds no regular
/// file by that name (a committed link is not followed, as git itself does not follow one), or
/// one larger than [`ATTRIBUTES_AT_MOST`], or git cannot say.
pub fn committed_attributes(top: &Path) -> Option<String> {
    let ask = |args: &[&str]| {
        git::run(top, args, git::READ)
            .ok()
            .filter(git::Run::ok)
            .map(|run| run.out)
    };
    // `<mode> blob <object>\t.gitattributes`, or nothing where HEAD has none.
    let listed = ask(&["ls-tree", "HEAD", "--", ".gitattributes"])?;
    let (meta, _) = listed.split_once('\t')?;
    let mut fields = meta.split_whitespace();
    let (mode, kind, object) = (fields.next()?, fields.next()?, fields.next()?);
    if kind != "blob" || !matches!(mode, "100644" | "100755") {
        return None;
    }
    let size: u64 = ask(&["cat-file", "-s", object])?.trim().parse().ok()?;
    if size > ATTRIBUTES_AT_MOST {
        return None;
    }
    ask(&["cat-file", "blob", object])
}

/// The keys of `entries`, a git config listing (`key`, `value`; the section and name
/// lowercased as git lists them), that would have changed how a clone of `url` goes: a
/// proxy, a certificate authority or check, an extra header, for every URL or for one that
/// matches `url`; and a `url.<base>.insteadOf` whose value starts `url`. Each named once, as
/// git's documentation spells it, a URL in the key shown as `<url>` or `<base>`.
pub fn route_keys(entries: &[(String, String)], url: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut add = |key: String| {
        if !out.contains(&key) {
            out.push(key);
        }
    };
    for (key, value) in entries {
        let (section, rest) = key.split_once('.').unwrap_or((key, ""));
        let (sub, name) = match rest.rsplit_once('.') {
            Some((sub, name)) => (Some(sub), name),
            None => (None, rest),
        };
        match section.to_ascii_lowercase().as_str() {
            "http" => {
                let Some(spelled) = NETWORK_KEYS
                    .iter()
                    .find(|one| one.eq_ignore_ascii_case(name))
                else {
                    continue;
                };
                // An empty proxy is git's way to switch one off: it changes no route.
                if *spelled == "proxy" && value.is_empty() {
                    continue;
                }
                match sub {
                    None => add(format!("http.{spelled}")),
                    Some(sub) if url_matches(sub, url) => add(format!("http.<url>.{spelled}")),
                    Some(_) => {}
                }
            }
            "url"
                if sub.is_some()
                    && name.eq_ignore_ascii_case("insteadof")
                    && !value.is_empty()
                    && url.starts_with(value.as_str()) =>
            {
                add("url.<base>.insteadOf".to_owned());
            }
            _ => {}
        }
    }
    out
}

/// The `http.*` keys that change how an HTTPS clone reaches its host, as git spells them.
const NETWORK_KEYS: [&str; 8] = [
    "proxy",
    "sslCAInfo",
    "sslCAPath",
    "sslVerify",
    "sslCert",
    "sslKey",
    "cookieFile",
    "extraHeader",
];

/// Whether `pattern`, the URL of an `http.<url>.*` key, covers `url`, as git matches one: the
/// same scheme, the same host (a `*` stands for one label), the same port, and a path that is
/// a prefix of `url`'s at a `/`. A user name in either is not compared, where git requires one
/// the pattern names to match: that can only name a key that did not apply, never hide one
/// that did, and a note that says too much here costs nothing.
fn url_matches(pattern: &str, url: &str) -> bool {
    let split = |url: &str| -> Option<(String, String, String)> {
        let (scheme, rest) = url.split_once("://")?;
        let (authority, path) = rest.split_once('/').unwrap_or((rest, ""));
        let host = authority
            .rsplit_once('@')
            .map_or(authority, |(_, host)| host);
        Some((
            scheme.to_ascii_lowercase(),
            host.to_ascii_lowercase(),
            path.trim_end_matches('/').to_owned(),
        ))
    };
    let (Some((p_scheme, p_host, p_path)), Some((scheme, host, path))) =
        (split(pattern), split(url))
    else {
        return false;
    };
    let hosts = {
        let p: Vec<&str> = p_host.split('.').collect();
        let h: Vec<&str> = host.split('.').collect();
        p.len() == h.len() && p.iter().zip(&h).all(|(p, h)| *p == "*" || p == h)
    };
    let paths = p_path.is_empty()
        || path == p_path
        || path
            .strip_prefix(p_path.as_str())
            .is_some_and(|rest| rest.starts_with('/'));
    p_scheme == scheme && hosts && paths
}

/// The person's global git config files, as git would find them for their terminal: the one
/// `GIT_CONFIG_GLOBAL` names, or `$XDG_CONFIG_HOME/git/config` (`~/.config/git/config`) and
/// `~/.gitconfig`.
fn global_files() -> Vec<PathBuf> {
    if let Some(named) = std::env::var_os("GIT_CONFIG_GLOBAL") {
        return vec![PathBuf::from(named)];
    }
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let xdg = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|dir| dir.is_absolute())
        .or_else(|| home.as_ref().map(|home| home.join(".config")));
    xdg.map(|dir| dir.join("git").join("config"))
        .into_iter()
        .chain(home.map(|home| home.join(".gitconfig")))
        .collect()
}

/// Every entry of the person's global git config files ([`global_files`]), **read to tell,
/// never applied**: listed by the hardened runner with `--file`, which reads that file alone
/// and no include, and handed to nothing but [`route_keys`]. A file that is not there or does
/// not read gives nothing.
pub fn global_entries() -> Vec<(String, String)> {
    let mut out = Vec::new();
    for file in global_files() {
        if !file.is_file() {
            continue;
        }
        let file = file.display().to_string();
        let Ok(listed) = git::run(
            Path::new("/"),
            &["config", "--file", &file, "--no-includes", "--list", "-z"],
            git::READ,
        ) else {
            continue;
        };
        if !listed.ok() {
            continue;
        }
        for entry in listed.out.split('\0').filter(|one| !one.is_empty()) {
            let (key, value) = entry.split_once('\n').unwrap_or((entry, ""));
            out.push((key.to_owned(), value.to_owned()));
        }
    }
    out
}

/// The sentence a failed brokered clone of `url` adds when the person's own git config would
/// have changed its route (`entries`, [`global_entries`]): which keys, and where to run it
/// instead. `None` when none would have.
pub fn network_note(entries: &[(String, String)], url: &str) -> Option<String> {
    let keys = route_keys(entries, url);
    if keys.is_empty() {
        return None;
    }
    let named = keys
        .iter()
        .map(|key| format!("`{key}`"))
        .collect::<Vec<_>>()
        .join(", ");
    Some(format!(
        "Your own git config sets {named} for this URL, and git run by the app for a chat reads \
         none of your git config, so this clone can fail here where it works in your terminal. \
         Run `purlis clone` in your own terminal to clone it with your config."
    ))
}

#[cfg(test)]
#[path = "unread_tests.rs"]
mod tests;

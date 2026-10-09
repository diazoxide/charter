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
/// files name: one sentence per filter, none where they name none. `name` is how the checkout
/// is called in the answer, `at` where it is, as the person would `cd` to it.
///
/// Every `.gitattributes` is read, the top one first and then each one in a folder (#1550): a
/// repository can turn a filter on for one folder alone. Each is read as **committed**
/// ([`committed_attributes`]), never from the working tree a chat writes.
pub fn filter_notes(top: &Path, name: &str, at: &str) -> Vec<String> {
    let mut filters: Vec<String> = Vec::new();
    for text in committed_attributes(top) {
        for filter in filters_named(&text) {
            if !filters.contains(&filter) {
                filters.push(filter);
            }
        }
    }
    filters
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

/// The most of one committed `.gitattributes` read: far past any real one.
pub const ATTRIBUTES_AT_MOST: u64 = 64 * 1024;

/// The most `.gitattributes` files one checkout's note reads: far past any real repository's.
/// The top one is always among them.
pub const ATTRIBUTES_FILES_AT_MOST: usize = 64;

/// Every `.gitattributes` of the checkout at `top` **as its `HEAD` commits it**, the top one
/// first, asked of git through the hardened runner: the app reads them outside the chat's
/// sandbox, and the working tree is the chat's to write, so a file there could be a link to
/// anything the app can read, a pipe that never ends, or a file without end (#1413).
///
/// Which files there are is asked of the index git just wrote at the checkout (`ls-files`,
/// which matches a pattern in every folder and answers only the names that match), and each
/// one is then looked up in `HEAD` (`ls-tree`) and read from there: a name the index holds and
/// `HEAD` does not is not read. Left out: a name `HEAD` holds as anything but a regular file
/// (a committed link is not followed, as git itself does not follow one), one larger than
/// [`ATTRIBUTES_AT_MOST`], every one past [`ATTRIBUTES_FILES_AT_MOST`], and all of them where
/// git cannot say.
pub fn committed_attributes(top: &Path) -> Vec<String> {
    let ask = |args: &[&str]| {
        git::run(top, args, git::READ)
            .ok()
            .filter(git::Run::ok)
            .map(|run| run.out)
    };
    let Some(indexed) = ask(&[
        "ls-files",
        "-z",
        "--",
        ".gitattributes",
        ":(glob)**/.gitattributes",
    ]) else {
        return Vec::new();
    };
    let mut names: Vec<&str> = Vec::new();
    for name in indexed.split('\0').filter(|one| !one.is_empty()) {
        if !names.contains(&name) {
            names.push(name);
        }
    }
    // The top one first, whatever sorts before it.
    names.sort_by_key(|name| *name != ".gitattributes");
    names.truncate(ATTRIBUTES_FILES_AT_MOST);
    if names.is_empty() {
        return Vec::new();
    }
    // Each name as written: a name is never read as a pattern here.
    let mut listing = vec!["--literal-pathspecs", "ls-tree", "-l", "-z", "HEAD", "--"];
    listing.extend(names.iter().copied());
    let Some(listed) = ask(&listing) else {
        return Vec::new();
    };
    // `<mode> <kind> <object> <size>\t<path>`, one for each name HEAD holds.
    let mut by_name: Vec<(&str, &str)> = Vec::new();
    for entry in listed.split('\0').filter(|one| !one.is_empty()) {
        let Some((meta, path)) = entry.split_once('\t') else {
            continue;
        };
        let mut fields = meta.split_whitespace();
        let (Some(mode), Some(kind), Some(object), Some(size)) =
            (fields.next(), fields.next(), fields.next(), fields.next())
        else {
            continue;
        };
        let size: Option<u64> = size.parse().ok();
        if kind == "blob"
            && matches!(mode, "100644" | "100755")
            && size.is_some_and(|size| size <= ATTRIBUTES_AT_MOST)
        {
            by_name.push((path, object));
        }
    }
    names
        .iter()
        .filter_map(|name| by_name.iter().find(|(path, _)| path == name))
        .filter_map(|(_, object)| ask(&["cat-file", "blob", object]))
        .collect()
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

/// Whether a failed clone's own words (`err`, git's standard error) read as a failure to reach
/// the host: a name that did not resolve, a connection, a proxy, a certificate, or the host
/// refusing who asked (curl's "unable to access", git's "Authentication failed"). Only such a
/// failure is one a key of [`route_keys`] could have changed, so only then is it named
/// ([`network_note`]): a clone that fails for another reason (a repository that is not there,
/// a ref, a disk) names none, so a clone made to fail cannot ask which keys the person's config
/// sets for a host (#1550).
///
/// What git quotes (`'…'`, a URL or a folder) is left out first: a repository called `openssl`
/// that is not there does not read as a certificate failure.
pub fn reads_as_a_route_failure(err: &str) -> bool {
    let unquoted: String = err
        .split('\'')
        .step_by(2)
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase();
    ROUTE_FAILURES.iter().any(|said| unquoted.contains(said))
}

/// What git and curl say, lowercased, when a clone could not reach its host or was refused at
/// it.
const ROUTE_FAILURES: [&str; 9] = [
    "unable to access",
    "authentication failed",
    "could not resolve",
    "failed to connect",
    "connection timed out",
    "connection refused",
    "proxy",
    "ssl",
    "certificate",
];

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

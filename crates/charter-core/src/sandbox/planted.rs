//! Where the later-code class (ADR 0067 §5 class 5, rulings V73b and V73d) is found in a plane
//! when a chat starts. Names that are protected wherever they are ([`super::PLANTED`]) need no
//! search. What a protected config points at elsewhere does: the directory a `core.hooksPath`
//! names, and every script a harness's project config runs. Each is denied writing from the
//! chat's start, resolved then.
//!
//! The search is bounded ([`DEPTH`], [`LOOKED_AT`]) and follows no link. Something added after
//! the start, or beyond the bound, is not resolved: the config that would name it is itself
//! never written by the chat, so only a config a person writes later can name something new.

use std::path::{Path, PathBuf};

/// How many directories below the plane the search goes.
pub const DEPTH: usize = 4;

/// How many directory entries the search looks at in all.
pub const LOOKED_AT: usize = 20_000;

/// Directories the search never enters: a repository's own internals, and package trees.
const SKIPPED: [&str; 3] = [".git", "node_modules", "target"];

/// `root` and the directories below it, to [`DEPTH`], without links or [`SKIPPED`] ones.
pub fn directories(root: &Path) -> Vec<PathBuf> {
    let mut found = vec![root.to_path_buf()];
    let mut looked = 0;
    let mut level = vec![root.to_path_buf()];
    for _ in 0..DEPTH {
        let mut next = Vec::new();
        for dir in level {
            let Ok(entries) = std::fs::read_dir(&dir) else {
                continue;
            };
            for entry in entries.flatten() {
                looked += 1;
                if looked > LOOKED_AT {
                    return found;
                }
                let is_dir = entry.file_type().is_ok_and(|kind| kind.is_dir());
                let name = entry.file_name();
                if !is_dir || SKIPPED.iter().any(|skipped| name == *skipped) {
                    continue;
                }
                found.push(entry.path());
                next.push(entry.path());
            }
        }
        level = next;
    }
    found
}

/// The directories of `dirs` that hold a `.git`: each a clone or a worktree.
pub fn clones(dirs: &[PathBuf]) -> Vec<PathBuf> {
    dirs.iter()
        .filter(|dir| dir.join(".git").exists())
        .cloned()
        .collect()
}

/// The git directory of the clone at `clone`, and the common one its config lives in: the
/// `.git` directory, or for a worktree the directory its `.git` file names and that one's
/// `commondir`.
pub fn git_dirs(clone: &Path) -> Vec<PathBuf> {
    let dot_git = clone.join(".git");
    if dot_git.is_dir() {
        return vec![dot_git];
    }
    let Ok(text) = std::fs::read_to_string(&dot_git) else {
        return Vec::new();
    };
    let Some(named) = text.lines().find_map(|line| line.strip_prefix("gitdir:")) else {
        return Vec::new();
    };
    let git_dir = clone.join(named.trim());
    let mut dirs = vec![git_dir.clone()];
    if let Ok(common) = std::fs::read_to_string(git_dir.join("commondir")) {
        dirs.push(git_dir.join(common.trim()));
    }
    dirs
}

/// The submodules' git directories below a clone's `git_dir` (`modules/…`, nested), each one
/// a directory holding a `HEAD`.
pub fn module_git_dirs(git_dir: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut looked = 0;
    let mut level = vec![git_dir.join("modules")];
    while !level.is_empty() && looked < LOOKED_AT {
        let mut next = Vec::new();
        for dir in level {
            let Ok(entries) = std::fs::read_dir(&dir) else {
                continue;
            };
            for entry in entries.flatten() {
                looked += 1;
                if !entry.file_type().is_ok_and(|kind| kind.is_dir()) {
                    continue;
                }
                let path = entry.path();
                if path.join("HEAD").is_file() {
                    found.push(path.clone());
                    next.push(path.join("modules"));
                } else {
                    next.push(path);
                }
            }
        }
        level = next;
    }
    found
}

/// `core.hooksPath` in the git config `text`, as written.
pub fn hooks_path_in(text: &str) -> Option<String> {
    let mut in_core = false;
    let mut found = None;
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            let section = line.trim_start_matches('[').split([']', ' ', '"']).next();
            in_core = section.is_some_and(|it| it.eq_ignore_ascii_case("core"));
            continue;
        }
        if !in_core {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        if key.trim().eq_ignore_ascii_case("hookspath") {
            let value = value.trim();
            let value = value
                .strip_prefix('"')
                .and_then(|it| it.strip_suffix('"'))
                .unwrap_or(value);
            found = Some(value.to_owned());
        }
    }
    found
}

/// `written`, a path a config names, made absolute against `base` with `~` as `home`.
fn absolute(written: &str, base: &Path, home: Option<&Path>) -> Option<PathBuf> {
    if written.is_empty() || written.contains('$') {
        return None;
    }
    if let Some(rest) = written.strip_prefix("~/") {
        return home.map(|home| home.join(rest));
    }
    let path = Path::new(written);
    let joined = if path.is_absolute() {
        path.to_path_buf()
    } else {
        base.join(path)
    };
    // `./` parts said as written, not as a sandbox matches a path.
    Some(
        joined
            .components()
            .filter(|part| !matches!(part, std::path::Component::CurDir))
            .collect(),
    )
}

/// The directories every `core.hooksPath` that git would use names: each clone's own, and the
/// user's global one, resolved against each clone. Those directories hold the hooks git runs.
pub fn hooks_paths(
    clones: &[PathBuf],
    home: Option<&Path>,
    xdg_config: Option<&Path>,
) -> Vec<PathBuf> {
    let global: Vec<String> = [
        home.map(|home| home.join(".gitconfig")),
        xdg_config.map(|config| config.join("git/config")),
    ]
    .into_iter()
    .flatten()
    .filter_map(|file| std::fs::read_to_string(file).ok())
    .filter_map(|text| hooks_path_in(&text))
    .collect();
    let mut found = Vec::new();
    for clone in clones {
        let own = git_dirs(clone)
            .into_iter()
            .filter_map(|dir| std::fs::read_to_string(dir.join("config")).ok())
            .filter_map(|text| hooks_path_in(&text));
        for written in own.chain(global.iter().cloned()) {
            if let Some(path) = absolute(&written, clone, home) {
                found.push(path);
            }
        }
    }
    found
}

/// The project config files whose commands a harness runs, relative to the directory they
/// configure (ruling V73d).
pub const COMMAND_CONFIGS: [&str; 6] = [
    ".claude/settings.json",
    ".claude/settings.local.json",
    ".mcp.json",
    "opencode.json",
    "opencode.jsonc",
    ".codex/config.toml",
];

/// The keys whose values are a command, or a command's words, or a plugin, in those files.
const COMMAND_KEYS: [&str; 4] = ["command", "args", "plugin", "plugins"];

/// Every script the project config files in `dir` name for a harness to run: each word of a
/// command, its arguments or a plugin entry that is a path, made absolute against `dir`.
pub fn scripts_named(dir: &Path, home: Option<&Path>) -> Vec<PathBuf> {
    let mut words = Vec::new();
    for file in COMMAND_CONFIGS {
        let Ok(text) = std::fs::read_to_string(dir.join(file)) else {
            continue;
        };
        let value: Option<serde_json::Value> = if file.ends_with(".toml") {
            text.parse::<toml::Table>()
                .ok()
                .and_then(|table| serde_json::to_value(table).ok())
        } else {
            serde_json::from_str(&without_comments(&text)).ok()
        };
        if let Some(value) = value {
            commands_in(&value, false, &mut words);
        }
    }
    let dir_text = dir.display().to_string();
    words
        .iter()
        .flat_map(|text| {
            text.split_whitespace()
                .map(str::to_owned)
                .collect::<Vec<_>>()
        })
        .map(|word| {
            word.replace(['"', '\''], "")
                .trim_start_matches("file://")
                .replace("${CLAUDE_PROJECT_DIR}", &dir_text)
                .replace("$CLAUDE_PROJECT_DIR", &dir_text)
        })
        .map(|word| match word.split_once('=') {
            Some((flag, value)) if flag.starts_with('-') => value.to_owned(),
            _ => word,
        })
        .filter(|word| word.contains('/') && !word.contains("://"))
        .filter_map(|word| absolute(&word, dir, home))
        .collect()
}

/// The strings under a [`COMMAND_KEYS`] key anywhere in `value`.
fn commands_in(value: &serde_json::Value, under: bool, into: &mut Vec<String>) {
    match value {
        serde_json::Value::String(text) if under => into.push(text.clone()),
        serde_json::Value::Array(items) => {
            for item in items {
                commands_in(item, under, into);
            }
        }
        serde_json::Value::Object(map) => {
            for (key, item) in map {
                commands_in(item, under || COMMAND_KEYS.contains(&key.as_str()), into);
            }
        }
        _ => {}
    }
}

/// `text` without `//` line comments, which `opencode.jsonc` may hold. A `//` inside a string,
/// as in a URL, is kept.
fn without_comments(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for line in text.lines() {
        let mut in_string = false;
        let mut escaped = false;
        let mut cut = line.len();
        let bytes = line.as_bytes();
        for (at, byte) in bytes.iter().enumerate() {
            match byte {
                _ if escaped => escaped = false,
                b'\\' if in_string => escaped = true,
                b'"' => in_string = !in_string,
                b'/' if !in_string && bytes.get(at + 1) == Some(&b'/') => {
                    cut = at;
                    break;
                }
                _ => {}
            }
        }
        out.push_str(&line[..cut]);
        out.push('\n');
    }
    out
}

/// Everything resolved for the plane at `root` at a chat's start: what `core.hooksPath` names
/// and every script a harness's project config runs, in the plane and the clones in it.
pub fn resolved(root: &Path, home: Option<&Path>, xdg_config: Option<&Path>) -> Vec<PathBuf> {
    let dirs = directories(root);
    let clones = clones(&dirs);
    let mut found = hooks_paths(&clones, home, xdg_config);
    for dir in &dirs {
        found.extend(scripts_named(dir, home));
    }
    let mut seen = std::collections::BTreeSet::new();
    found.retain(|path| seen.insert(path.clone()));
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_hooks_path_is_read_from_the_core_section_in_any_case_and_quoting() {
        assert_eq!(
            hooks_path_in(
                "[user]\n\thooksPath = no\n[core]\n\tbare = false\n\tHooksPath = \"tools/hooks\"\n"
            ),
            Some("tools/hooks".to_owned())
        );
        assert_eq!(hooks_path_in("[core]\n\tbare = false\n"), None);
        assert_eq!(
            hooks_path_in("[core \"x\"]\nhookspath=h\n"),
            Some("h".to_owned())
        );
    }

    #[test]
    fn every_hooks_path_git_would_use_is_resolved_against_its_clone() {
        let plane = tempfile::tempdir().expect("a plane");
        let clone = plane.path().join("ws/repo");
        std::fs::create_dir_all(clone.join(".git")).expect("a clone");
        std::fs::write(
            clone.join(".git/config"),
            "[core]\n\thooksPath = scripts/hooks\n",
        )
        .expect("its config");
        let home = tempfile::tempdir().expect("a home");
        std::fs::write(
            home.path().join(".gitconfig"),
            "[core]\n\thooksPath = ~/my-hooks\n",
        )
        .expect("a global config");
        let found = resolved(plane.path(), Some(home.path()), None);
        assert!(found.contains(&clone.join("scripts/hooks")), "{found:?}");
        assert!(found.contains(&home.path().join("my-hooks")), "{found:?}");
    }

    #[test]
    fn a_worktree_s_hooks_path_is_read_from_its_common_git_directory() {
        let plane = tempfile::tempdir().expect("a plane");
        let main = plane.path().join("main");
        std::fs::create_dir_all(main.join(".git/worktrees/w")).expect("a main clone");
        std::fs::write(main.join(".git/config"), "[core]\nhooksPath = /abs/hooks\n")
            .expect("its config");
        std::fs::write(main.join(".git/worktrees/w/commondir"), "../..\n").expect("commondir");
        let worktree = plane.path().join("w");
        std::fs::create_dir_all(&worktree).expect("a worktree");
        std::fs::write(
            worktree.join(".git"),
            format!("gitdir: {}\n", main.join(".git/worktrees/w").display()),
        )
        .expect("its .git file");
        let found = hooks_paths(&[worktree], None, None);
        assert_eq!(found, [PathBuf::from("/abs/hooks")]);
    }

    #[test]
    fn every_script_a_harness_s_project_config_runs_is_named() {
        let dir = tempfile::tempdir().expect("a project");
        let at = |rel: &str| dir.path().join(rel);
        std::fs::create_dir_all(at(".claude")).expect(".claude");
        std::fs::write(
            at(".claude/settings.json"),
            r#"{"hooks": {"Stop": [{"hooks": [{"type": "command", "command": "\"$CLAUDE_PROJECT_DIR\"/.claude/hooks/stop.sh --x"}]}]},
                "statusLine": {"command": "./bin/status.sh"}}"#,
        )
        .expect("settings");
        std::fs::write(
            at(".mcp.json"),
            r#"{"mcpServers": {"x": {"command": "node", "args": ["tools/server.js", "--port=3"]}}}"#,
        )
        .expect(".mcp.json");
        std::fs::write(
            at("opencode.jsonc"),
            "{\n // a comment\n \"plugin\": [\"file://./plugins/p.ts\", \"some-npm-plugin\"],\n \"mcp\": {\"y\": {\"command\": [\"bun\", \"run\", \"mcp/y.ts\"], \"url\": \"https://x.test/a\"}}\n}",
        )
        .expect("opencode.jsonc");
        std::fs::create_dir_all(at(".codex")).expect(".codex");
        std::fs::write(
            at(".codex/config.toml"),
            "[mcp_servers.z]\ncommand = \"/opt/z/run\"\nargs = [\"-c\", \"scripts/z.py\"]\n",
        )
        .expect("config.toml");
        let found = scripts_named(dir.path(), None);
        for want in [
            at(".claude/hooks/stop.sh"),
            at("bin/status.sh"),
            at("tools/server.js"),
            at("plugins/p.ts"),
            at("mcp/y.ts"),
            PathBuf::from("/opt/z/run"),
            at("scripts/z.py"),
        ] {
            assert!(found.contains(&want), "{want:?} not in {found:?}");
        }
        assert!(
            !found.iter().any(|it| it.ends_with("some-npm-plugin")),
            "{found:?}"
        );
    }

    #[test]
    fn a_submodule_s_git_directory_is_found_at_any_depth() {
        let clone = tempfile::tempdir().expect("a clone");
        let git = clone.path().join(".git");
        for dir in ["modules/a", "modules/a/modules/b", "modules/group/c"] {
            std::fs::create_dir_all(git.join(dir)).expect("a module");
            std::fs::write(git.join(dir).join("HEAD"), "ref: x\n").expect("HEAD");
        }
        let mut found = module_git_dirs(&git);
        found.sort();
        assert_eq!(
            found,
            [
                git.join("modules/a"),
                git.join("modules/a/modules/b"),
                git.join("modules/group/c")
            ]
        );
    }
}

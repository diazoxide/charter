//! The request budget meters what `forge::route::Resolver` resolves (FW-4), so a native transport
//! built anywhere else would send with a sign-in token and spend the account's budget unseen.
//! Outside tests, only `forge/route.rs` builds one: this walks every shipped source tree.

use std::path::{Path, PathBuf};

fn sources(dir: &Path, into: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap().flatten() {
        let path = entry.path();
        if path.is_dir() {
            sources(&path, into);
        } else if path.extension().is_some_and(|x| x == "rs") {
            into.push(path);
        }
    }
}

/// The text of `file` before its first `#[cfg(test)]`: what a shipped build compiles of it.
fn shipped(file: &Path) -> String {
    let text = std::fs::read_to_string(file).unwrap();
    match text.find("#[cfg(test)]") {
        Some(at) => text[..at].to_string(),
        None => text,
    }
}

#[test]
fn only_the_resolver_builds_the_native_transport() {
    purlis_core::unsteered!();
    let crate_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let workspace = crate_dir.join("../..");
    let mut files = Vec::new();
    for tree in [
        "crates/purlis-core/src",
        "crates/purlis-cli/src",
        "app/src-tauri/src",
    ] {
        let dir = workspace.join(tree);
        assert!(dir.is_dir(), "{} is not there", dir.display());
        sources(&dir, &mut files);
    }
    assert!(files.len() > 100, "the walk found {} files", files.len());
    let builders: Vec<String> = files
        .iter()
        .filter(|f| {
            let name = f.to_string_lossy();
            !name.ends_with("/tests.rs") && !name.contains("/tests/")
        })
        .filter(|f| shipped(f).contains("Http::new("))
        .map(|f| {
            f.strip_prefix(&workspace)
                .unwrap_or(f)
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    assert_eq!(builders, ["crates/purlis-core/src/forge/route.rs"]);
}

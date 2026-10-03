//! What the adapters' tests share.

use super::Harness;

/// A plane with the sandbox on, and what a chat of `harness` starts under in it on a macOS
/// machine, where every harness has a sandbox charter compiles: the sandbox compiled for that harness, or
/// why it was not started. The plane is handed back so a test can start the chat in it.
pub(crate) fn sandbox_compiled_for(
    harness: Harness,
) -> (
    tempfile::TempDir,
    Result<crate::sandbox::Applied, crate::sandbox::NotStarted>,
) {
    let plane = tempfile::tempdir().expect("a plane");
    std::fs::write(
        plane.path().join("charter.toml"),
        "[sandbox]\nmode = \"on\"\n",
    )
    .expect("charter.toml");
    let machine = crate::sandbox::Machine {
        env: crate::secrets::Env::of(&[]),
        home: None,
        os: crate::sandbox::Os::MacOs,
    };
    // Codex is held back from a sandboxed start (V87f); its compiler is what is tested.
    let applied = crate::sandbox::compiled_anyway(harness, plane.path(), &machine);
    (plane, applied)
}

/// The plugin bundle the app ships, as it sits in this repository.
pub(crate) fn bundled_plugin() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../app/src-tauri/plugin")
}

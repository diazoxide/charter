//! Tauri's build step, handed the app's commands so the IPC is an allow-list (ADR 0052).

use std::fs;
use std::path::PathBuf;

// The one list of commands (`src/ipc_commands.rs`), the same file the crate registers them from.
include!("src/ipc_commands.rs");

fn main() {
    println!("cargo:rerun-if-changed=src/ipc_commands.rs");
    let (value_free, vault_values) = app_commands!(command_names);
    let every: &'static [&'static str] =
        Box::leak([value_free, vault_values].concat().into_boxed_slice());

    // The two sets the capabilities grant, written from the list rather than beside it, so a
    // command cannot be listed and left ungranted, or granted and never listed.
    let sets = PathBuf::from(std::env::var("OUT_DIR").expect("cargo sets OUT_DIR"))
        .join("app-permissions");
    fs::create_dir_all(&sets).expect("a directory for the app's permission sets");
    fs::write(
        sets.join("sets.toml"),
        [
            set(
                "value-free",
                "Every app command that hands the window no secret's value.",
                value_free,
            ),
            set(
                "vault-values",
                "A vault's reveal and copy: the commands that put a secret's value where the \
                 window can reach it. Granted to the main window only.",
                vault_values,
            ),
        ]
        .concat(),
    )
    .expect("the app's permission sets are written");
    let pattern: &'static str = Box::leak(format!("{}/*.toml", sets.display()).into_boxed_str());

    tauri_build::try_build(
        tauri_build::Attributes::new().app_manifest(
            tauri_build::AppManifest::new()
                .commands(every)
                .permissions_path_pattern(pattern),
        ),
    )
    .expect("tauri-build");
}

/// A permission set granting `allow-<command>` for each command, in Tauri's TOML.
fn set(identifier: &str, description: &str, commands: &[&str]) -> String {
    let permissions: Vec<String> = commands
        .iter()
        .map(|command| format!("\"allow-{}\"", command.replace('_', "-")))
        .collect();
    format!(
        "[[set]]\nidentifier = \"{identifier}\"\ndescription = \"{description}\"\npermissions = [{}]\n\n",
        permissions.join(", ")
    )
}

//! What the window may invoke, and from which window (ADR 0052, charter-app#276).
//!
//! The commands are one list, in `ipc_commands.rs`. `build.rs` turns it into Tauri's app
//! manifest: a permission per command and two permission sets, `value-free` and `vault-values`.
//! The capabilities in `capabilities/` grant those sets to windows by label, and Tauri refuses
//! any app command that no capability grants to the window asking. Nothing in this module runs
//! in the app; it is the tests that hold the list, the manifest and the capabilities together.

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::collections::BTreeSet;
    use std::path::{Path, PathBuf};
    use std::rc::Rc;

    use tauri::test::{INVOKE_KEY, MockRuntime, mock_builder};
    use tauri::webview::InvokeRequest;

    use crate::lifecycle::WINDOW;
    use crate::windows::{SPLIT_GLOB, split_label};

    /// The permission set, and the capability granting it, for the commands that put a
    /// secret's value where the window can reach it. Spelled as `build.rs` spells it.
    const VAULT_VALUES: &str = "vault-values";

    /// `ipc_commands.rs`'s names: `(value_free, vault_values)`.
    fn listed() -> (&'static [&'static str], &'static [&'static str]) {
        app_commands!(command_names)
    }

    fn every_listed() -> BTreeSet<&'static str> {
        let (free, values) = listed();
        free.iter().chain(values).copied().collect()
    }

    /// The names `tauri-specta` registers in the invoke handler, read from what it hands an
    /// exporter: the same configuration the bindings and the handler are made from.
    fn registered() -> BTreeSet<String> {
        struct Names(Rc<RefCell<BTreeSet<String>>>);
        impl tauri_specta::LanguageExt for Names {
            type Error = std::io::Error;
            fn export(
                self,
                cfg: &tauri_specta::BuilderConfiguration,
                _: &Path,
            ) -> Result<(), Self::Error> {
                self.0
                    .borrow_mut()
                    .extend(cfg.commands.iter().map(|f| f.name().to_owned()));
                Ok(())
            }
        }
        let names = Rc::new(RefCell::new(BTreeSet::new()));
        crate::commands()
            .export(Names(Rc::clone(&names)), "unused")
            .expect("the names are read");
        names.take()
    }

    #[test]
    fn every_command_the_handler_registers_is_on_the_allow_list_and_nothing_else_is() {
        // One list feeds both, so this holds by construction. It is here for the day someone
        // registers a command the other way, or the IPC stops naming a command after its
        // function: either would be a command the window can reach that the list never named,
        // or a grant for a command that does not exist.
        let registered = registered();
        let listed: BTreeSet<String> = every_listed().into_iter().map(str::to_owned).collect();
        assert_eq!(registered, listed);
    }

    #[test]
    fn no_command_is_in_both_classes() {
        let (free, values) = listed();
        let both: Vec<_> = free.iter().filter(|name| values.contains(name)).collect();
        assert!(
            both.is_empty(),
            "listed as value-free and value-bearing: {both:?}"
        );
        assert_eq!(
            every_listed().len(),
            free.len() + values.len(),
            "a command is listed twice"
        );
    }

    #[test]
    fn a_vaults_reveal_and_copy_are_the_value_bearing_commands() {
        // A command joining this class is a design decision (ADR 0052), not a list edit.
        let (_, values) = listed();
        assert_eq!(values, ["vault_secret_reveal", "vault_secret_copy"]);
    }

    /// The app as `tauri.conf.json` and `capabilities/` build it, on Tauri's mock runtime, with
    /// a handler that answers every command it is handed. Whatever is refused was refused by
    /// the ACL before any handler saw it.
    fn app() -> tauri::App<MockRuntime> {
        mock_builder()
            .invoke_handler(|invoke| {
                invoke.resolver.resolve(true);
                true
            })
            .build(tauri::generate_context!(test = true))
            .expect("the app builds with its real ACL")
    }

    fn window(app: &tauri::App<MockRuntime>, label: &str) -> tauri::WebviewWindow<MockRuntime> {
        tauri::WebviewWindowBuilder::new(app, label, tauri::WebviewUrl::default())
            .build()
            .expect("a window")
    }

    /// Invokes `command` from `window`, as the app's own page would.
    fn invoke(window: &tauri::WebviewWindow<MockRuntime>, command: &str) -> Result<(), String> {
        tauri::test::get_ipc_response(
            window,
            InvokeRequest {
                cmd: command.into(),
                callback: tauri::ipc::CallbackFn(0),
                error: tauri::ipc::CallbackFn(1),
                url: "tauri://localhost".parse().expect("a URL"),
                body: tauri::ipc::InvokeBody::default(),
                headers: tauri::http::HeaderMap::default(),
                invoke_key: INVOKE_KEY.to_owned(),
            },
        )
        .map(|_| ())
        .map_err(|why| why.to_string())
    }

    #[test]
    fn the_main_window_may_invoke_every_listed_command() {
        let app = app();
        let main = window(&app, WINDOW);
        let refused: Vec<_> = every_listed()
            .into_iter()
            .filter(|command| invoke(&main, command).is_err())
            .collect();
        assert!(
            refused.is_empty(),
            "listed but not granted to the main window: {refused:?}"
        );
    }

    #[test]
    fn a_command_nobody_listed_is_refused_even_from_the_main_window() {
        let app = app();
        let main = window(&app, WINDOW);
        assert!(invoke(&main, "a_command_nobody_listed").is_err());
    }

    #[test]
    fn a_second_window_may_invoke_the_allow_listed_commands() {
        // A project tab split into its own window (charter#126) runs the same page as the main
        // window, so it is granted the same commands: every listed one, the vault's reveal and
        // copy included, because a vault's tab can be in front there too (ADR 0052, amended
        // 2026-09-26). The same as the main window, and no more.
        let app = app();
        for label in [split_label(1), split_label(27)] {
            let split = window(&app, &label);
            let refused: Vec<_> = every_listed()
                .into_iter()
                .filter(|command| invoke(&split, command).is_err())
                .collect();
            assert!(
                refused.is_empty(),
                "listed but not granted to {label}: {refused:?}"
            );
            assert!(invoke(&split, "a_command_nobody_listed").is_err());
        }
    }

    #[test]
    fn no_window_but_charters_own_may_invoke_any_listed_command() {
        // A window charter did not make starts with nothing, and is granted what it needs by
        // name; above all it is never handed a vault's reveal or copy by default. The split
        // windows' grant is a pattern, so the labels nearest to it are tried too.
        let app = app();
        for label in [
            "another-window",
            "window-",
            "window-x",
            "windows-1",
            "mainly",
        ] {
            let other = window(&app, label);
            let allowed: Vec<_> = every_listed()
                .into_iter()
                .filter(|command| invoke(&other, command).is_ok())
                .collect();
            assert!(
                allowed.is_empty(),
                "{label}, which is not one of charter's windows, may invoke {allowed:?}"
            );
        }
    }

    #[test]
    fn every_capability_grants_the_main_window_and_the_split_windows_alike() {
        // The two capability files are one decision: a split window gets exactly what the main
        // window gets. One naming a window the other does not would be a window with half the
        // app, or a vault's values granted somewhere the ordinary commands are not.
        for capability in capability_files() {
            assert_eq!(
                capability["windows"],
                serde_json::json!([WINDOW, SPLIT_GLOB]),
                "{}",
                capability["identifier"]
            );
        }
    }

    fn src_tauri() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
    }

    fn json(path: &Path) -> serde_json::Value {
        serde_json::from_str(&std::fs::read_to_string(path).expect("readable"))
            .unwrap_or_else(|why| panic!("{} is not JSON: {why}", path.display()))
    }

    /// Each capability file in `capabilities/`: the JSON ones, which are the only kind the app
    /// has, and not whatever else an editor or Finder leaves beside them.
    fn capability_files() -> Vec<serde_json::Value> {
        std::fs::read_dir(src_tauri().join("capabilities"))
            .expect("capabilities/")
            .map(|entry| entry.expect("an entry").path())
            .filter(|path| path.extension().is_some_and(|ext| ext == "json"))
            .map(|path| json(&path))
            .collect()
    }

    /// Every capability the app is built with: each file in `capabilities/`, and each one
    /// `tauri.e2e.conf.json` writes inline.
    fn capabilities() -> Vec<serde_json::Value> {
        let mut all = capability_files();
        let e2e = json(&src_tauri().join("tauri.e2e.conf.json"));
        all.extend(
            e2e["app"]["security"]["capabilities"]
                .as_array()
                .expect("the e2e build names its capabilities")
                .iter()
                .filter(|entry| entry.is_object())
                .cloned(),
        );
        all
    }

    fn grants(capability: &serde_json::Value) -> Vec<String> {
        capability["permissions"]
            .as_array()
            .expect("permissions")
            .iter()
            .map(|entry| {
                entry
                    .as_str()
                    .or_else(|| entry["identifier"].as_str())
                    .expect("a permission names itself")
                    .to_owned()
            })
            .collect()
    }

    #[test]
    fn only_the_vault_values_capability_grants_the_value_bearing_commands_and_only_to_charters_windows()
     {
        let (_, values) = listed();
        let value_permissions: Vec<String> = values
            .iter()
            .map(|command| format!("allow-{}", command.replace('_', "-")))
            .collect();
        let mut granting = Vec::new();
        for capability in capabilities() {
            let grants = grants(&capability);
            if grants
                .iter()
                .any(|grant| grant == VAULT_VALUES || value_permissions.contains(grant))
            {
                granting.push(capability);
            }
        }
        assert_eq!(granting.len(), 1, "granted by: {granting:?}");
        let capability = &granting[0];
        assert_eq!(capability["identifier"], VAULT_VALUES);
        assert_eq!(
            capability["windows"],
            serde_json::json!([WINDOW, SPLIT_GLOB])
        );
        assert!(
            capability.get("webviews").is_none() && capability.get("remote").is_none(),
            "the vault-values capability reaches past charter's own windows: {capability}"
        );
        assert_eq!(grants(capability), [VAULT_VALUES]);
    }

    #[test]
    fn the_e2e_build_keeps_every_capability_the_app_ships() {
        // A config that names its capabilities gets only those: one the e2e build left out
        // would be a command the scenario tests could not reach, and a gap nobody saw.
        let e2e = json(&src_tauri().join("tauri.e2e.conf.json"));
        let named: BTreeSet<&str> = e2e["app"]["security"]["capabilities"]
            .as_array()
            .expect("the e2e build names its capabilities")
            .iter()
            .filter_map(serde_json::Value::as_str)
            .collect();
        for capability in capability_files() {
            let id = capability["identifier"].as_str().expect("an identifier");
            assert!(
                named.contains(id),
                "tauri.e2e.conf.json leaves out capability {id}"
            );
        }
    }

    /// The directives of `tauri.conf.json`'s Content-Security-Policy, by name.
    fn csp() -> std::collections::BTreeMap<String, Vec<String>> {
        let conf = json(&src_tauri().join("tauri.conf.json"));
        conf["app"]["security"]["csp"]
            .as_str()
            .expect("a CSP")
            .split(';')
            .filter_map(|directive| {
                let mut words = directive.split_whitespace().map(str::to_owned);
                Some((words.next()?, words.collect()))
            })
            .collect()
    }

    #[test]
    fn the_csp_runs_only_the_apps_own_scripts_and_loads_no_plugins_frames_or_forms() {
        let csp = csp();
        let say = |name: &str| csp.get(name).cloned().unwrap_or_default();
        assert_eq!(say("default-src"), ["'self'"]);
        assert_eq!(say("script-src"), ["'self'"]);
        for directive in ["object-src", "base-uri", "form-action", "frame-src"] {
            assert_eq!(say(directive), ["'none'"], "{directive}");
        }
        assert!(
            !csp.values()
                .flatten()
                .any(|source| source == "'unsafe-eval'"),
            "the CSP allows eval"
        );
    }
}

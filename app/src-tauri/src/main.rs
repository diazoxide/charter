// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // Started again as the bounded reader of a branch (FM-4, D-88h): answer one question and
    // exit, before anything of the app starts.
    if let Some(code) = purlis_core::files::serve_if_asked() {
        std::process::exit(code);
    }
    // Started again by the `charter` command to write one keyring item, so the item is this
    // binary's and charter's app alone reads it without asking (ruling V90a).
    if let Some(code) = purlis_core::secrets::keyhold::serve_if_asked() {
        std::process::exit(code);
    }
    purlis_core::secrets::keyhold::this_process_is_the_app();
    purlis_app_lib::run()
}

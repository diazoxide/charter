// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // Started again as the bounded reader of a branch (FM-4, D-88h): answer one question and
    // exit, before anything of the app starts.
    if let Some(code) = charter_core::files::serve_if_asked() {
        std::process::exit(code);
    }
    charter_app_lib::run()
}

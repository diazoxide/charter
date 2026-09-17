/// The plane the app was started in, or why there is none.
#[tauri::command]
fn plane_root() -> Result<String, String> {
    let cwd = std::env::current_dir()
        .map_err(|err| format!("cannot read the current directory: {err}"))?;
    charter_core::plane::find_root(&cwd)
        .map(|root| root.display().to_string())
        .map_err(|err| err.to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![plane_root])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

pub mod audio;
mod commands;
pub mod input;
pub mod pack;
mod runtime;
#[allow(dead_code)]
mod settings;

use tauri::Manager;

#[cfg(test)]
mod test_alloc;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let runtime = app
                .path()
                .app_data_dir()
                .map(runtime::KeyForgeRuntime::start)
                .unwrap_or_else(|_| runtime::KeyForgeRuntime::new_unavailable());
            app.manage(runtime);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::app_info::get_app_info,
            commands::runtime::get_runtime_status,
            commands::runtime::set_sound_enabled,
            commands::runtime::set_master_volume
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

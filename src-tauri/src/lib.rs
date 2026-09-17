pub mod audio;
mod commands;
pub mod input;
pub mod pack;
mod runtime;
#[allow(dead_code)]
mod settings;
mod tray;

use tauri::{Manager, WindowEvent};

#[cfg(test)]
mod test_alloc;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let runtime = app
                .path()
                .app_data_dir()
                .map(runtime::KeyForgeRuntime::start)
                .unwrap_or_else(|_| runtime::KeyForgeRuntime::new_unavailable());
            app.manage(runtime);
            app.manage(runtime::lifecycle::Lifecycle::default());
            app.manage(tray::TrayState::default());
            if tray::install(app.handle()).is_err() {
                app.state::<runtime::lifecycle::Lifecycle>()
                    .set_tray_available(false);
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            if window.label() != "main" {
                return;
            }
            if let WindowEvent::CloseRequested { api, .. } = event {
                let lifecycle = window.state::<runtime::lifecycle::Lifecycle>();
                if lifecycle.on_close_requested() == runtime::lifecycle::CloseDecision::Hide {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::app_info::get_app_info,
            commands::runtime::get_runtime_status,
            commands::runtime::set_sound_enabled,
            commands::runtime::set_master_volume,
            commands::runtime::import_sound_pack,
            commands::runtime::select_sound_pack
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

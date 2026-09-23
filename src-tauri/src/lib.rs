pub mod audio;
mod commands;
pub mod input;
pub mod pack;
mod panel;
mod runtime;
#[allow(dead_code)]
mod settings;
mod tray;

use std::sync::Mutex;
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
            app.manage(Mutex::new(panel::PanelState::default()));
            let tray_available = tray::install(app.handle()).is_ok();
            app.state::<runtime::lifecycle::Lifecycle>()
                .set_tray_available(tray_available);

            if let Some(window) = app.get_webview_window("main") {
                #[cfg(target_os = "macos")]
                {
                    let snapshot = app.state::<runtime::KeyForgeRuntime>().snapshot();
                    if panel::should_show_window_on_startup(
                        true,
                        snapshot.input_status == runtime::state::RuntimeInputStatus::Ready,
                        snapshot.audio_status == runtime::state::RuntimeAudioStatus::Ready,
                        tray_available,
                    ) {
                        let panel = app.state::<Mutex<panel::PanelState>>();
                        if let Ok(mut panel) = panel.lock() {
                            let _ = commands::panel::show_startup_controls(&window, &mut panel);
                        };
                    }
                }

                #[cfg(not(target_os = "macos"))]
                {
                    window.set_decorations(true)?;
                    window.set_resizable(true)?;
                    window.show()?;
                }
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            if window.label() != "main" {
                return;
            }
            match event {
                WindowEvent::CloseRequested { api, .. } => {
                    let lifecycle = window.state::<runtime::lifecycle::Lifecycle>();
                    if lifecycle.on_close_requested() == runtime::lifecycle::CloseDecision::Hide {
                        api.prevent_close();
                        let _ = window.hide();
                    }
                }
                #[cfg(target_os = "macos")]
                WindowEvent::Focused(false) => {
                    let panel = window.state::<Mutex<panel::PanelState>>();
                    if let Ok(mut panel) = panel.lock() {
                        if let Some(webview) = window.app_handle().get_webview_window("main") {
                            commands::panel::dismiss_controls_on_focus_loss(&webview, &mut panel);
                        }
                    };
                }
                _ => {}
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::app_info::get_app_info,
            commands::runtime::get_runtime_status,
            commands::runtime::set_sound_enabled,
            commands::runtime::set_master_volume,
            commands::runtime::import_sound_pack,
            commands::runtime::select_sound_pack,
            commands::panel::set_panel_presentation
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

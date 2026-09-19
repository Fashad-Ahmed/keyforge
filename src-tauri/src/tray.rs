use std::sync::Mutex;

use tauri::{
    menu::{CheckMenuItem, Menu, MenuItem},
    tray::TrayIconBuilder,
    AppHandle, Manager,
};

use crate::runtime::{lifecycle::Lifecycle, KeyForgeRuntime};

const ENABLE_ID: &str = "keyforge-enable";
const SHOW_ID: &str = "keyforge-show";
const QUIT_ID: &str = "keyforge-quit";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TrayAction {
    SetEnabled(bool),
    Show,
    Quit,
}

pub(crate) trait TrayTarget {
    fn set_enabled(&self, enabled: bool);
    fn show(&self);
    fn quit(&self);
}

pub(crate) fn dispatch_action(target: &impl TrayTarget, action: TrayAction) {
    match action {
        TrayAction::SetEnabled(enabled) => target.set_enabled(enabled),
        TrayAction::Show => target.show(),
        TrayAction::Quit => target.quit(),
    }
}

#[derive(Default)]
pub(crate) struct TrayState {
    enabled_item: Mutex<Option<CheckMenuItem<tauri::Wry>>>,
}

impl TrayState {
    pub(crate) fn set_enabled(&self, enabled: bool) {
        if let Some(item) = self
            .enabled_item
            .lock()
            .expect("tray state mutex poisoned")
            .as_ref()
        {
            let _ = item.set_checked(enabled);
        }
    }
}

struct AppTrayTarget<'a> {
    app: &'a AppHandle,
}

impl TrayTarget for AppTrayTarget<'_> {
    fn set_enabled(&self, enabled: bool) {
        let runtime = self.app.state::<KeyForgeRuntime>();
        match runtime.set_enabled(enabled) {
            Ok(snapshot) => self
                .app
                .state::<TrayState>()
                .set_enabled(snapshot.sound_enabled),
            Err(_) => self
                .app
                .state::<TrayState>()
                .set_enabled(runtime.snapshot().sound_enabled),
        }
    }

    fn show(&self) {
        if let Some(window) = self.app.get_webview_window("main") {
            let _ = window.show();
            let _ = window.unminimize();
            let _ = window.set_focus();
        }
    }

    fn quit(&self) {
        self.app.state::<Lifecycle>().request_quit();
        self.app.exit(0);
    }
}

pub(crate) fn install(app: &AppHandle) -> tauri::Result<()> {
    let snapshot = app.state::<KeyForgeRuntime>().snapshot();
    let enabled = CheckMenuItem::with_id(
        app,
        ENABLE_ID,
        "Enable Sounds",
        true,
        snapshot.sound_enabled,
        None::<&str>,
    )?;
    let show = MenuItem::with_id(app, SHOW_ID, "Show KeyForge", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, QUIT_ID, "Quit", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&enabled, &show, &quit])?;
    *app.state::<TrayState>()
        .enabled_item
        .lock()
        .expect("tray state mutex poisoned") = Some(enabled.clone());

    let mut builder = TrayIconBuilder::with_id("keyforge-tray")
        .menu(&menu)
        .tooltip("KeyForge")
        .on_menu_event(|app, event| {
            let action = match event.id().0.as_str() {
                ENABLE_ID => {
                    let enabled = app
                        .state::<TrayState>()
                        .enabled_item
                        .lock()
                        .expect("tray state mutex poisoned")
                        .as_ref()
                        .and_then(|item| item.is_checked().ok())
                        .unwrap_or_else(|| app.state::<KeyForgeRuntime>().snapshot().sound_enabled);
                    TrayAction::SetEnabled(enabled)
                }
                SHOW_ID => TrayAction::Show,
                QUIT_ID => TrayAction::Quit,
                _ => return,
            };
            dispatch_action(&AppTrayTarget { app }, action);
        });
    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }
    builder.build(app)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use super::{dispatch_action, TrayAction, TrayTarget};

    #[derive(Default)]
    struct FakeTarget {
        enabled: Cell<Option<bool>>,
        shown: Cell<bool>,
        quit: Cell<bool>,
    }

    impl TrayTarget for FakeTarget {
        fn set_enabled(&self, enabled: bool) {
            self.enabled.set(Some(enabled));
        }

        fn show(&self) {
            self.shown.set(true);
        }

        fn quit(&self) {
            self.quit.set(true);
        }
    }

    #[test]
    fn tray_actions_route_to_one_shared_target() {
        let target = FakeTarget::default();

        dispatch_action(&target, TrayAction::SetEnabled(false));
        dispatch_action(&target, TrayAction::Show);
        dispatch_action(&target, TrayAction::Quit);

        assert_eq!(target.enabled.get(), Some(false));
        assert!(target.shown.get());
        assert!(target.quit.get());
    }
}

use std::sync::Mutex;

pub(crate) use crate::panel::PanelCommandError;
use crate::panel::{
    dispatch_panel_request, on_focus_lost, panel_origin, toggle_panel, PanelAnchor, PanelHost,
    PanelRequest, PanelSize, PanelState, PanelView, ScreenRect,
};

#[tauri::command]
pub(crate) fn set_panel_presentation(
    window: tauri::WebviewWindow,
    panel: tauri::State<'_, Mutex<PanelState>>,
    request: PanelRequest,
) -> Result<(), PanelCommandError> {
    #[cfg(target_os = "macos")]
    {
        let mut panel = panel.lock().map_err(|_| PanelCommandError::Unavailable)?;
        let mut host = WindowPanelHost { window: &window };
        dispatch_panel_request(&mut panel, &mut host, request).map(|_| ())
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (window, panel, request);
        Err(PanelCommandError::Unavailable)
    }
}

#[cfg(target_os = "macos")]
pub(crate) fn handle_tray_click(app: &tauri::AppHandle, rect: tauri::Rect) {
    use tauri::Manager;

    let Some(window) = app.get_webview_window("main") else {
        return;
    };
    let panel = app.state::<Mutex<PanelState>>();
    let Ok(mut panel) = panel.lock() else {
        return;
    };
    if let Some(anchor) = anchor_for_tray_rect(&window, rect) {
        panel.set_anchor(anchor);
    }
    let mut host = WindowPanelHost { window: &window };
    let _ = toggle_panel(&mut panel, &mut host);
}

#[cfg(target_os = "macos")]
pub(crate) fn show_manage(app: &tauri::AppHandle) {
    use tauri::Manager;

    let Some(window) = app.get_webview_window("main") else {
        return;
    };
    let panel = app.state::<Mutex<PanelState>>();
    let Ok(mut panel) = panel.lock() else {
        return;
    };
    let mut host = WindowPanelHost { window: &window };
    let _ = dispatch_panel_request(&mut panel, &mut host, PanelRequest::Manage);
}

#[cfg(target_os = "macos")]
pub(crate) fn show_startup_controls(
    window: &tauri::WebviewWindow,
    panel: &mut PanelState,
) -> Result<(), PanelCommandError> {
    if let Some(anchor) = anchor_for_primary_monitor(window) {
        panel.set_anchor(anchor);
    }
    let mut host = WindowPanelHost { window };
    dispatch_panel_request(panel, &mut host, PanelRequest::Controls).map(|_| ())
}

#[cfg(target_os = "macos")]
pub(crate) fn dismiss_controls_on_focus_loss(
    window: &tauri::WebviewWindow,
    panel: &mut PanelState,
) {
    let mut host = WindowPanelHost { window };
    let _ = on_focus_lost(panel, &mut host);
}

#[cfg(target_os = "macos")]
struct WindowPanelHost<'a> {
    window: &'a tauri::WebviewWindow,
}

#[cfg(target_os = "macos")]
impl PanelHost for WindowPanelHost<'_> {
    fn is_visible(&self) -> Result<bool, PanelCommandError> {
        self.window
            .is_visible()
            .map_err(|_| PanelCommandError::Unavailable)
    }

    fn present(
        &mut self,
        view: PanelView,
        anchor: Option<PanelAnchor>,
    ) -> Result<(), PanelCommandError> {
        use tauri::{LogicalSize, PhysicalPosition, Position, Size};

        let (width, height) = match view {
            PanelView::Controls => (360.0, 420.0),
            PanelView::Manage => (760.0, 800.0),
        };
        self.window
            .set_decorations(view == PanelView::Manage)
            .map_err(|_| PanelCommandError::Unavailable)?;
        self.window
            .set_resizable(view == PanelView::Manage)
            .map_err(|_| PanelCommandError::Unavailable)?;
        self.window
            .set_always_on_top(view == PanelView::Controls)
            .map_err(|_| PanelCommandError::Unavailable)?;
        self.window
            .set_size(Size::Logical(LogicalSize::new(width, height)))
            .map_err(|_| PanelCommandError::Unavailable)?;

        if let Some(anchor) = anchor {
            let factor = anchor.scale_factor();
            let physical_size = PanelSize::new(
                (width * factor).round() as u32,
                (height * factor).round() as u32,
            );
            let origin = panel_origin(anchor.icon, anchor.monitor, physical_size, 6);
            self.window
                .set_position(Position::Physical(PhysicalPosition::new(
                    origin.x, origin.y,
                )))
                .map_err(|_| PanelCommandError::Unavailable)?;
        }

        self.window
            .show()
            .and_then(|_| self.window.unminimize())
            .and_then(|_| self.window.set_focus())
            .map_err(|_| PanelCommandError::Unavailable)
    }

    fn dismiss(&mut self) -> Result<(), PanelCommandError> {
        self.window
            .set_always_on_top(false)
            .and_then(|_| self.window.hide())
            .map_err(|_| PanelCommandError::Unavailable)
    }
}

#[cfg(target_os = "macos")]
fn anchor_for_tray_rect(window: &tauri::WebviewWindow, rect: tauri::Rect) -> Option<PanelAnchor> {
    use tauri::{Position, Size};

    let fallback_scale = window.scale_factor().ok().unwrap_or(1.0);
    let (x, y) = match rect.position {
        Position::Physical(position) => (f64::from(position.x), f64::from(position.y)),
        Position::Logical(position) => (position.x * fallback_scale, position.y * fallback_scale),
    };
    let (width, height) = match rect.size {
        Size::Physical(size) => (f64::from(size.width), f64::from(size.height)),
        Size::Logical(size) => (size.width * fallback_scale, size.height * fallback_scale),
    };
    let icon = ScreenRect::new(
        x.round() as i32,
        y.round() as i32,
        width.round() as u32,
        height.round() as u32,
    );
    let monitors = window.available_monitors().unwrap_or_default();
    let center_x = i64::from(icon.x) + i64::from(icon.width) / 2;
    let center_y = i64::from(icon.y) + i64::from(icon.height) / 2;
    let monitor = monitors
        .iter()
        .find(|monitor| {
            let position = monitor.position();
            let size = monitor.size();
            let left = i64::from(position.x);
            let top = i64::from(position.y);
            center_x >= left
                && center_x < left + i64::from(size.width)
                && center_y >= top
                && center_y < top + i64::from(size.height)
        })
        .cloned()
        .or_else(|| window.primary_monitor().ok().flatten())?;
    Some(PanelAnchor::new_scaled(
        icon,
        screen_rect_from_monitor(&monitor),
        monitor.scale_factor(),
    ))
}

#[cfg(target_os = "macos")]
fn anchor_for_primary_monitor(window: &tauri::WebviewWindow) -> Option<PanelAnchor> {
    let monitor = window.primary_monitor().ok().flatten()?;
    let bounds = screen_rect_from_monitor(&monitor);
    let icon_x = (i64::from(bounds.x) + i64::from(bounds.width) - 24)
        .clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32;
    Some(PanelAnchor::new_scaled(
        ScreenRect::new(icon_x, bounds.y, 24, 24),
        bounds,
        monitor.scale_factor(),
    ))
}

#[cfg(target_os = "macos")]
fn screen_rect_from_monitor(monitor: &tauri::Monitor) -> ScreenRect {
    let position = monitor.position();
    let size = monitor.size();
    ScreenRect::new(position.x, position.y, size.width, size.height)
}

#[cfg(test)]
mod tests {
    use super::PanelCommandError;
    use crate::panel::PanelRequest;

    #[test]
    fn panel_request_deserialization_accepts_only_closed_values() {
        assert_eq!(
            serde_json::from_str::<PanelRequest>("\"manage\"").unwrap(),
            PanelRequest::Manage
        );
        assert!(serde_json::from_str::<PanelRequest>("\"open_url\"").is_err());
        assert!(serde_json::from_str::<PanelRequest>("{\"url\":\"https://example.com\"}").is_err());
    }

    #[test]
    fn panel_command_error_is_sanitized() {
        assert_eq!(
            PanelCommandError::Unavailable.to_string(),
            "panel unavailable"
        );
    }
}

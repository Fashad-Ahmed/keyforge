use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ScreenRect {
    pub(crate) x: i32,
    pub(crate) y: i32,
    pub(crate) width: u32,
    pub(crate) height: u32,
}

impl ScreenRect {
    pub(crate) const fn new(x: i32, y: i32, width: u32, height: u32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PanelSize {
    pub(crate) width: u32,
    pub(crate) height: u32,
}

impl PanelSize {
    pub(crate) const fn new(width: u32, height: u32) -> Self {
        Self { width, height }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PhysicalPoint {
    pub(crate) x: i32,
    pub(crate) y: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PanelAnchor {
    pub(crate) icon: ScreenRect,
    pub(crate) monitor: ScreenRect,
    scale_factor_milli: u32,
}

impl PanelAnchor {
    pub(crate) fn new_scaled(icon: ScreenRect, monitor: ScreenRect, scale_factor: f64) -> Self {
        Self {
            icon,
            monitor,
            scale_factor_milli: (scale_factor * 1000.0).round() as u32,
        }
    }

    pub(crate) fn scale_factor(self) -> f64 {
        f64::from(self.scale_factor_milli) / 1000.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PanelView {
    Controls,
    Manage,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum PanelRequest {
    Controls,
    Manage,
    Dismiss,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PanelEffect {
    Show(PanelView),
    Hide,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum PanelCommandError {
    Unavailable,
}

impl std::fmt::Display for PanelCommandError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("panel unavailable")
    }
}

impl std::error::Error for PanelCommandError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PanelState {
    view: PanelView,
    anchor: Option<PanelAnchor>,
}

impl Default for PanelState {
    fn default() -> Self {
        Self {
            view: PanelView::Controls,
            anchor: None,
        }
    }
}

impl PanelState {
    #[cfg(test)]
    pub(crate) const fn view(&self) -> PanelView {
        self.view
    }

    pub(crate) fn set_anchor(&mut self, anchor: PanelAnchor) {
        self.anchor = Some(anchor);
    }
}

pub(crate) trait PanelHost {
    fn is_visible(&self) -> Result<bool, PanelCommandError>;
    fn present(
        &mut self,
        view: PanelView,
        anchor: Option<PanelAnchor>,
    ) -> Result<(), PanelCommandError>;
    fn dismiss(&mut self) -> Result<(), PanelCommandError>;
}

pub(crate) fn dispatch_panel_request(
    state: &mut PanelState,
    host: &mut impl PanelHost,
    request: PanelRequest,
) -> Result<PanelEffect, PanelCommandError> {
    let view = match request {
        PanelRequest::Controls => Some(PanelView::Controls),
        PanelRequest::Manage => Some(PanelView::Manage),
        PanelRequest::Dismiss => None,
    };

    match view {
        Some(view) => {
            host.present(view, state.anchor)?;
            state.view = view;
            Ok(PanelEffect::Show(view))
        }
        None => {
            host.dismiss()?;
            Ok(PanelEffect::Hide)
        }
    }
}

pub(crate) fn toggle_panel(
    state: &mut PanelState,
    host: &mut impl PanelHost,
) -> Result<PanelEffect, PanelCommandError> {
    if host.is_visible()? {
        dispatch_panel_request(state, host, PanelRequest::Dismiss)
    } else {
        dispatch_panel_request(state, host, PanelRequest::Controls)
    }
}

pub(crate) fn on_focus_lost(
    state: &mut PanelState,
    host: &mut impl PanelHost,
) -> Result<PanelEffect, PanelCommandError> {
    if state.view == PanelView::Controls && host.is_visible()? {
        dispatch_panel_request(state, host, PanelRequest::Dismiss)
    } else {
        Ok(PanelEffect::None)
    }
}

pub(crate) const fn should_show_window_on_startup(
    is_macos: bool,
    input_ready: bool,
    audio_ready: bool,
    tray_available: bool,
) -> bool {
    !is_macos || !input_ready || !audio_ready || !tray_available
}

impl PhysicalPoint {
    pub(crate) const fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }
}

pub(crate) fn panel_origin(
    icon: ScreenRect,
    monitor: ScreenRect,
    panel: PanelSize,
    gap: i32,
) -> PhysicalPoint {
    fn clamp_axis(origin: i32, extent: u32, desired: i64, panel_extent: u32) -> i32 {
        let origin = i64::from(origin);
        let extent = i64::from(extent);
        let panel_extent = i64::from(panel_extent);
        if panel_extent >= extent {
            return origin as i32;
        }

        desired
            .clamp(origin, origin + extent - panel_extent)
            .clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32
    }

    let x = i64::from(icon.x) + i64::from(icon.width) - i64::from(panel.width);
    let y = i64::from(icon.y) + i64::from(icon.height) + i64::from(gap);

    PhysicalPoint::new(
        clamp_axis(monitor.x, monitor.width, x, panel.width),
        clamp_axis(monitor.y, monitor.height, y, panel.height),
    )
}

#[cfg(test)]
mod tests {
    use super::PanelCommandError;
    use super::{
        dispatch_panel_request, on_focus_lost, panel_origin, should_show_window_on_startup,
        toggle_panel, PanelAnchor, PanelEffect, PanelHost, PanelRequest, PanelSize, PanelState,
        PanelView, PhysicalPoint, ScreenRect,
    };

    #[derive(Default)]
    struct FakePanelHost {
        visible: bool,
        fail_next: bool,
        last_view: Option<PanelView>,
        last_position: Option<PhysicalPoint>,
    }

    impl PanelHost for FakePanelHost {
        fn is_visible(&self) -> Result<bool, PanelCommandError> {
            Ok(self.visible)
        }

        fn present(
            &mut self,
            view: PanelView,
            anchor: Option<PanelAnchor>,
        ) -> Result<(), PanelCommandError> {
            if self.fail_next {
                self.fail_next = false;
                return Err(PanelCommandError::Unavailable);
            }
            self.visible = true;
            self.last_view = Some(view);
            if let Some(anchor) = anchor {
                let size = match view {
                    PanelView::Controls => PanelSize::new(360, 420),
                    PanelView::Manage => PanelSize::new(760, 800),
                };
                self.last_position = Some(panel_origin(anchor.icon, anchor.monitor, size, 6));
            }
            Ok(())
        }

        fn dismiss(&mut self) -> Result<(), PanelCommandError> {
            if self.fail_next {
                self.fail_next = false;
                return Err(PanelCommandError::Unavailable);
            }
            self.visible = false;
            Ok(())
        }
    }

    #[test]
    fn panel_origin_aligns_below_the_tray_icon() {
        let icon = ScreenRect::new(1380, 18, 24, 24);
        let monitor = ScreenRect::new(0, 0, 1440, 900);
        let size = PanelSize::new(380, 460);
        assert_eq!(
            panel_origin(icon, monitor, size, 6),
            PhysicalPoint::new(1024, 48)
        );
    }

    #[test]
    fn panel_origin_clamps_to_monitor_edges_and_handles_negative_origins() {
        assert_eq!(
            panel_origin(
                ScreenRect::new(0, 18, 24, 24),
                ScreenRect::new(0, 0, 1440, 900),
                PanelSize::new(380, 460),
                6,
            ),
            PhysicalPoint::new(0, 48)
        );
        assert_eq!(
            panel_origin(
                ScreenRect::new(1416, 18, 24, 24),
                ScreenRect::new(0, 0, 1440, 900),
                PanelSize::new(380, 460),
                6,
            ),
            PhysicalPoint::new(1060, 48)
        );
        assert_eq!(
            panel_origin(
                ScreenRect::new(700, 870, 24, 20),
                ScreenRect::new(0, 0, 1440, 900),
                PanelSize::new(380, 460),
                6,
            ),
            PhysicalPoint::new(344, 440)
        );
        assert_eq!(
            panel_origin(
                ScreenRect::new(-1200, 18, 24, 24),
                ScreenRect::new(-1600, 0, 1200, 900),
                PanelSize::new(380, 460),
                6,
            ),
            PhysicalPoint::new(-1556, 48)
        );
        assert_eq!(
            panel_origin(
                ScreenRect::new(20, 18, 24, 24),
                ScreenRect::new(0, 0, 100, 80),
                PanelSize::new(380, 460),
                6,
            ),
            PhysicalPoint::new(0, 0)
        );
    }

    #[test]
    fn panel_requests_show_controls_and_manage_then_dismiss_without_changing_view() {
        let mut state = PanelState::default();
        let mut host = FakePanelHost::default();

        assert_eq!(
            dispatch_panel_request(&mut state, &mut host, PanelRequest::Controls),
            Ok(PanelEffect::Show(PanelView::Controls))
        );
        assert_eq!(state.view(), PanelView::Controls);
        assert_eq!(
            dispatch_panel_request(&mut state, &mut host, PanelRequest::Manage),
            Ok(PanelEffect::Show(PanelView::Manage))
        );
        assert_eq!(state.view(), PanelView::Manage);
        assert_eq!(
            dispatch_panel_request(&mut state, &mut host, PanelRequest::Dismiss),
            Ok(PanelEffect::Hide)
        );
        assert_eq!(state.view(), PanelView::Manage);
        assert!(!host.visible);
    }

    #[test]
    fn tray_click_toggles_controls_and_focus_loss_only_hides_controls() {
        let mut state = PanelState::default();
        let mut host = FakePanelHost::default();

        assert_eq!(
            toggle_panel(&mut state, &mut host),
            Ok(PanelEffect::Show(PanelView::Controls))
        );
        assert_eq!(toggle_panel(&mut state, &mut host), Ok(PanelEffect::Hide));
        dispatch_panel_request(&mut state, &mut host, PanelRequest::Manage).unwrap();
        assert_eq!(on_focus_lost(&mut state, &mut host), Ok(PanelEffect::None));
        dispatch_panel_request(&mut state, &mut host, PanelRequest::Controls).unwrap();
        assert_eq!(on_focus_lost(&mut state, &mut host), Ok(PanelEffect::Hide));
    }

    #[test]
    fn manage_reuses_the_latest_rust_owned_tray_anchor() {
        let mut state = PanelState::default();
        let mut host = FakePanelHost::default();
        state.set_anchor(PanelAnchor::new_scaled(
            ScreenRect::new(1380, 18, 24, 24),
            ScreenRect::new(0, 0, 1440, 900),
            1.0,
        ));
        dispatch_panel_request(&mut state, &mut host, PanelRequest::Controls).unwrap();
        state.set_anchor(PanelAnchor::new_scaled(
            ScreenRect::new(-1200, 18, 24, 24),
            ScreenRect::new(-1600, 0, 1200, 900),
            1.0,
        ));

        dispatch_panel_request(&mut state, &mut host, PanelRequest::Manage).unwrap();

        assert_eq!(host.last_position, Some(PhysicalPoint::new(-1600, 48)));
    }

    #[test]
    fn failed_show_does_not_commit_the_new_panel_view() {
        let mut state = PanelState::default();
        let mut host = FakePanelHost {
            fail_next: true,
            ..FakePanelHost::default()
        };
        assert_eq!(
            dispatch_panel_request(&mut state, &mut host, PanelRequest::Manage),
            Err(PanelCommandError::Unavailable)
        );
        assert_eq!(state.view(), PanelView::Controls);
    }

    #[test]
    fn startup_shows_recovery_ui_only_when_needed_on_macos() {
        assert!(!should_show_window_on_startup(true, true, true, true));
        assert!(should_show_window_on_startup(true, false, true, true));
        assert!(should_show_window_on_startup(true, true, false, true));
        assert!(should_show_window_on_startup(true, true, true, false));
        assert!(should_show_window_on_startup(false, true, true, true));
    }
}

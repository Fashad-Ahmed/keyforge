#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ScreenRect {
    pub(crate) x: i32,
    pub(crate) y: i32,
    pub(crate) width: u32,
    pub(crate) height: u32,
}

impl ScreenRect {
    const fn new(x: i32, y: i32, width: u32, height: u32) -> Self {
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
    const fn new(width: u32, height: u32) -> Self {
        Self { width, height }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PhysicalPoint {
    pub(crate) x: i32,
    pub(crate) y: i32,
}

impl PhysicalPoint {
    const fn new(x: i32, y: i32) -> Self {
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
    use super::{panel_origin, PanelSize, PhysicalPoint, ScreenRect};

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
}

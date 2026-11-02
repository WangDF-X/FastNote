use super::{LogicalRect, Monitor, ResolvedGeometry, WindowGeometry};

pub(super) fn usable(monitor: &Monitor) -> bool {
    let r = monitor.work;
    [
        r.x,
        r.y,
        r.w,
        r.h,
        r.x + r.w,
        r.y + r.h,
        monitor.scale,
        monitor.origin_x,
        monitor.origin_y,
    ]
    .iter()
    .all(|v| v.is_finite())
        && r.w > 0.0
        && r.h > 0.0
        && monitor.scale > 0.0
}

pub(super) fn primary(monitors: &[Monitor]) -> Option<&Monitor> {
    monitors
        .iter()
        .find(|m| m.primary && usable(m))
        .or_else(|| monitors.iter().find(|m| usable(m)))
}

fn visible_area(g: WindowGeometry, work: LogicalRect) -> f64 {
    let width = ((g.x + g.w).min(work.x + work.w) - g.x.max(work.x)).max(0.0);
    let height = ((g.y + g.h).min(work.y + work.h) - g.y.max(work.y)).max(0.0);
    // ARCH-06 §10：标题栏和至少 80×80 的区域可见。
    if width >= 80.0 && height >= 80.0 && g.y >= work.y && g.y + 32.0 <= work.y + work.h {
        width * height
    } else {
        0.0
    }
}

fn resolve(geometry: WindowGeometry, monitor: &Monitor, corrected: bool) -> ResolvedGeometry {
    let (physical_x, physical_y) = monitor.to_physical(geometry.x, geometry.y);
    ResolvedGeometry {
        geometry,
        physical_x,
        physical_y,
        scale: monitor.scale,
        corrected,
    }
}

/// 恢复主窗口：无效/屏幕移除时以默认尺寸居中主屏，保留合法的最大化标记。
/// 没有可用屏幕时返回 None，由应用层提示并使用平台默认位置。
pub fn restore_main(
    saved: Option<WindowGeometry>,
    monitors: &[Monitor],
) -> Option<ResolvedGeometry> {
    let fallback = primary(monitors)?;
    // 配置文件可被外部编辑。尺寸不能超过整个桌面可用范围，以免创建巨型渲染缓冲。
    let usable_monitors: Vec<_> = monitors.iter().filter(|monitor| usable(monitor)).collect();
    let left = usable_monitors
        .iter()
        .map(|monitor| monitor.work.x)
        .fold(f64::INFINITY, f64::min);
    let top = usable_monitors
        .iter()
        .map(|monitor| monitor.work.y)
        .fold(f64::INFINITY, f64::min);
    let right = usable_monitors
        .iter()
        .map(|monitor| monitor.work.x + monitor.work.w)
        .fold(f64::NEG_INFINITY, f64::max);
    let bottom = usable_monitors
        .iter()
        .map(|monitor| monitor.work.y + monitor.work.h)
        .fold(f64::NEG_INFINITY, f64::max);
    if let Some(g) = saved.filter(|g| g.is_valid()) {
        if let Some(target) = monitors
            .iter()
            .filter(|m| usable(m))
            .filter(|_| g.w <= (right - left).max(1024.0) && g.h <= (bottom - top).max(680.0))
            .filter(|m| visible_area(g, m.work) > 0.0)
            .max_by(|a, b| visible_area(g, a.work).total_cmp(&visible_area(g, b.work)))
        {
            return Some(resolve(g, target, false));
        }
    }
    let work = fallback.work;
    let w = 1280.0_f64.min(work.w).max(1024.0);
    let h = 800.0_f64.min(work.h).max(680.0);
    let geometry = WindowGeometry {
        x: work.x + (work.w - w) / 2.0,
        y: (work.y + (work.h - h) / 2.0).max(work.y),
        w,
        h,
        maximized: saved.filter(|g| g.is_valid()).is_some_and(|g| g.maximized),
    };
    Some(resolve(geometry, fallback, saved.is_some()))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn monitor(x: f64, scale: f64, main: bool) -> Monitor {
        Monitor {
            name: if main { "primary" } else { "other" }.to_owned(),
            work: LogicalRect {
                x,
                y: 0.0,
                w: 1920.0,
                h: 1040.0,
            },
            origin_x: x,
            origin_y: 0.0,
            scale,
            primary: main,
        }
    }
    fn geometry(x: f64) -> WindowGeometry {
        WindowGeometry {
            x,
            y: 100.0,
            w: 1280.0,
            h: 800.0,
            maximized: false,
        }
    }
    #[test]
    fn test_first_window_is_centered_without_a_correction_warning() {
        let result = restore_main(None, &[monitor(0.0, 1.0, true)]).unwrap();
        assert_eq!((result.geometry.x, result.geometry.y), (320.0, 120.0));
        assert!(!result.corrected);
    }
    #[test]
    fn test_valid_geometry_is_kept_on_negative_coordinate_monitor() {
        let saved = geometry(-1800.0);
        let screens = [monitor(0.0, 1.0, true), monitor(-1920.0, 1.5, false)];
        let result = restore_main(Some(saved), &screens).unwrap();
        assert_eq!(result.geometry, saved);
        assert_eq!(result.scale, 1.5);
        assert_eq!(result.physical_x, -1740.0);
    }
    #[test]
    fn test_mixed_dpi_restore_does_not_choose_the_adjacent_monitor() {
        let screens = [monitor(0.0, 1.25, true), monitor(2560.0, 1.5, false)];
        let result = restore_main(Some(geometry(2660.0)), &screens).unwrap();
        assert_eq!(result.physical_x, 2710.0);
        assert_eq!(result.scale, 1.5);
    }
    #[test]
    fn test_missing_monitor_recenters_with_default_size_and_keeps_maximized() {
        let saved = WindowGeometry {
            w: 1600.0,
            maximized: true,
            ..geometry(9000.0)
        };
        let result = restore_main(Some(saved), &[monitor(0.0, 1.0, true)]).unwrap();
        assert!(result.corrected && result.geometry.maximized);
        assert_eq!((result.geometry.x, result.geometry.w), (320.0, 1280.0));
    }
    #[test]
    fn test_invisible_title_or_invalid_size_uses_default_geometry() {
        for saved in [
            WindowGeometry {
                y: -500.0,
                ..geometry(100.0)
            },
            WindowGeometry {
                w: 100.0,
                ..geometry(100.0)
            },
            WindowGeometry {
                x: f64::NAN,
                ..geometry(100.0)
            },
        ] {
            assert!(
                restore_main(Some(saved), &[monitor(0.0, 1.0, true)])
                    .unwrap()
                    .corrected
            );
        }
    }
    #[test]
    fn test_small_work_area_does_not_violate_minimum_window_size() {
        let mut m = monitor(0.0, 2.0, true);
        m.work.w = 800.0;
        m.work.h = 600.0;
        let g = restore_main(None, &[m]).unwrap().geometry;
        assert_eq!((g.w, g.h, g.y), (1024.0, 680.0, 0.0));
    }
    #[test]
    fn test_no_usable_monitor_returns_none() {
        assert_eq!(restore_main(None, &[]), None);
        assert_eq!(restore_main(None, &[monitor(0.0, 0.0, true)]), None);
        assert!(restore_main(None, &[monitor(0.0, 1.0, false)]).is_some());
    }

    #[test]
    fn test_giant_external_geometry_recovers_without_a_giant_render_buffer() {
        let saved = WindowGeometry {
            w: 1_000_000_000.0,
            h: 1_000_000_000.0,
            ..geometry(100.0)
        };
        let restored = restore_main(Some(saved), &[monitor(0.0, 1.0, true)]).unwrap();
        assert!(restored.corrected);
        assert_eq!((restored.geometry.w, restored.geometry.h), (1280.0, 800.0));
    }
}

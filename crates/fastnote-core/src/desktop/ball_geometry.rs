use super::geometry::{primary, usable};
use super::{BallPosition, Monitor};

fn fits(monitor: &Monitor, size: f64) -> bool {
    usable(monitor)
        && size.is_finite()
        && size > 0.0
        && size <= monitor.work.w
        && size <= monitor.work.h
}

/// 恢复球的位置并贴最近的左/右工作区边缘；屏幕移除/越界时回到主屏右下位置。
/// 球体与工具窗口同尺寸，完整留在工作区，不覆盖任务栏（Q-SHELL-05）。
pub fn restore_ball(
    saved: Option<&BallPosition>,
    monitors: &[Monitor],
    size: f64,
) -> Option<(BallPosition, f64)> {
    let fallback = primary(monitors)
        .filter(|monitor| fits(monitor, size))
        .or_else(|| monitors.iter().find(|monitor| fits(monitor, size)))?;
    let target = saved.filter(|position| position.is_valid()).and_then(|p| {
        monitors
            .iter()
            .find(|monitor| monitor.name == p.monitor && fits(monitor, size))
    });
    if let (Some(position), Some(monitor)) = (saved, target) {
        let work = monitor.work;
        if position.x >= work.x
            && position.y >= work.y
            && position.x + size <= work.x + work.w
            && position.y + size <= work.y + work.h
        {
            return Some((
                snap_ball(position.x, position.y, size, monitor)?,
                monitor.scale,
            ));
        }
    }
    let work = fallback.work;
    Some((
        BallPosition {
            x: work.x + work.w - size,
            y: work.y + work.h - size,
            monitor: fallback.name.clone(),
        },
        fallback.scale,
    ))
}

/// 拖动结束只贴最近的左/右边；等距时左侧优先，纵向保留位置，仅越界夹取。
/// 不吸附上下边（Q-SHELL-07）。
/// 非法工作区/尺寸不生成持久化值，不能让过大的球遮挡任务栏。
pub fn snap_ball(x: f64, y: f64, size: f64, monitor: &Monitor) -> Option<BallPosition> {
    if !fits(monitor, size) {
        return None;
    }
    let work = monitor.work;
    let right = work.x + work.w - size;
    let bottom = work.y + work.h - size;
    let x = if x.is_finite() {
        x.clamp(work.x, right)
    } else {
        work.x
    };
    let y = if y.is_finite() {
        y.clamp(work.y, bottom)
    } else {
        work.y
    };
    let x = if x - work.x <= right - x {
        work.x
    } else {
        right
    };
    Some(BallPosition {
        x,
        y,
        monitor: monitor.name.clone(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::LogicalRect;

    fn screen() -> Monitor {
        Monitor {
            name: "primary".to_owned(),
            work: LogicalRect {
                x: 0.0,
                y: 0.0,
                w: 1920.0,
                h: 1040.0,
            },
            origin_x: 0.0,
            origin_y: 0.0,
            scale: 1.0,
            primary: true,
        }
    }

    #[test]
    fn test_default_ball_touches_right_and_bottom_without_taskbar_overlap() {
        let (position, scale) = restore_ball(None, &[screen()], 48.0).unwrap();
        assert_eq!((position.x, position.y, scale), (1872.0, 992.0, 1.0));
    }

    #[test]
    fn test_old_interior_position_is_docked_to_nearest_side_on_restore() {
        let position = BallPosition {
            x: 20.0,
            y: 30.0,
            monitor: "primary".to_owned(),
        };
        let restored = restore_ball(Some(&position), &[screen()], 48.0).unwrap().0;
        assert_eq!((restored.x, restored.y), (0.0, 30.0));
        assert_eq!(restored.monitor, position.monitor);
    }

    #[test]
    fn test_removed_or_invalid_saved_position_returns_to_default_corner() {
        for (x, monitor) in [
            (20.0, "removed"),
            (9999.0, "primary"),
            (f64::INFINITY, "primary"),
        ] {
            let saved = BallPosition {
                x,
                y: 30.0,
                monitor: monitor.to_owned(),
            };
            let restored = restore_ball(Some(&saved), &[screen()], 48.0).unwrap().0;
            assert_eq!((restored.x, restored.y), (1872.0, 992.0));
        }
    }

    #[test]
    fn test_release_always_docks_to_nearest_side_even_far_from_an_edge() {
        let monitor = screen();
        for (x, y, expected) in [
            (100.0, 400.0, (0.0, 400.0)),
            (1800.0, 400.0, (1872.0, 400.0)),
            (800.0, 100.0, (0.0, 100.0)),
            (800.0, 900.0, (0.0, 900.0)),
            (1100.0, 100.0, (1872.0, 100.0)),
            (1100.0, 900.0, (1872.0, 900.0)),
            (-20.0, 2000.0, (0.0, 992.0)),
            (f64::NAN, f64::INFINITY, (0.0, 0.0)),
        ] {
            let position = snap_ball(x, y, 48.0, &monitor).unwrap();
            assert_eq!((position.x, position.y), expected);
        }
    }

    #[test]
    fn test_equal_horizontal_distances_choose_left_at_every_height() {
        for y in [8.0, 496.0, 984.0] {
            let position = snap_ball(936.0, y, 48.0, &screen()).unwrap();
            assert_eq!((position.x, position.y), (0.0, y));
        }
    }

    #[test]
    fn test_vertical_proximity_never_causes_top_or_bottom_snapping() {
        for (x, expected_x) in [(800.0, 0.0), (1200.0, 1872.0)] {
            for y in [1.0, 8.0, 984.0, 991.0] {
                let position = snap_ball(x, y, 48.0, &screen()).unwrap();
                assert_eq!((position.x, position.y), (expected_x, y));
            }
        }
    }

    #[test]
    fn test_old_top_or_bottom_only_positions_restore_to_sides_at_the_same_height() {
        for y in [0.0, 992.0] {
            let saved = BallPosition {
                x: 800.0,
                y,
                monitor: "primary".to_owned(),
            };
            let position = restore_ball(Some(&saved), &[screen()], 48.0).unwrap().0;
            assert_eq!((position.x, position.y), (0.0, y));
        }
    }

    #[test]
    fn test_vertical_overflow_is_clamped_without_changing_the_nearest_side() {
        for (y, expected_y) in [(-100.0, 0.0), (2000.0, 992.0)] {
            let position = snap_ball(1800.0, y, 48.0, &screen()).unwrap();
            assert_eq!((position.x, position.y), (1872.0, expected_y));
        }
    }

    #[test]
    fn test_negative_coordinate_and_mixed_dpi_docking_reach_physical_edge() {
        let mut monitor = screen();
        monitor.work.x = -1920.0;
        monitor.work.w = 1280.0;
        monitor.origin_x = -1920.0;
        monitor.scale = 1.5;
        let position = snap_ball(-700.0, 200.0, 48.0, &monitor).unwrap();
        assert_eq!((position.x, position.y), (-688.0, 200.0));
        assert_eq!(monitor.to_physical(position.x, position.y).0 + 72.0, 0.0);
    }

    #[test]
    fn test_invalid_monitor_or_size_never_generates_a_position() {
        assert_eq!(restore_ball(None, &[], 48.0), None);
        for size in [f64::NAN, f64::INFINITY, -1.0, 0.0, 2000.0] {
            assert_eq!(restore_ball(None, &[screen()], size), None);
            assert_eq!(snap_ball(0.0, 0.0, size, &screen()), None);
        }
        let mut monitor = screen();
        monitor.work.w = -1.0;
        assert_eq!(snap_ball(0.0, 0.0, 48.0, &monitor), None);
        monitor.work.w = f64::MAX;
        monitor.work.x = f64::MAX;
        assert_eq!(restore_ball(None, &[monitor], 48.0), None);
    }

    #[test]
    fn test_too_small_primary_falls_back_to_a_usable_other_monitor() {
        let mut tiny = screen();
        tiny.work.w = 30.0;
        let mut other = screen();
        other.primary = false;
        other.name = "other".to_owned();
        let position = restore_ball(None, &[tiny, other], 48.0).unwrap().0;
        assert_eq!(position.monitor, "other");
    }
}

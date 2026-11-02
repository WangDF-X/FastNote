/// 球体拖动纯状态：按物理指针位移计算，不使用移动窗口后的局部 mouse_x。
pub(super) struct BallDrag {
    pointer: (i32, i32),
    window: (i32, i32),
    threshold: f64,
    moved: bool,
}

impl BallDrag {
    pub(super) fn new(pointer: (i32, i32), window: (i32, i32), scale: f64) -> Self {
        Self {
            pointer,
            window,
            threshold: 4.0 * scale,
            moved: false,
        }
    }

    pub(super) fn update(&mut self, pointer: (i32, i32)) -> Option<(i32, i32)> {
        let dx = i64::from(pointer.0) - i64::from(self.pointer.0);
        let dy = i64::from(pointer.1) - i64::from(self.pointer.1);
        if !self.moved && (dx as f64).hypot(dy as f64) <= self.threshold {
            return None;
        }
        self.moved = true;
        let clamp = |value: i64| value.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32;
        Some((
            clamp(i64::from(self.window.0) + dx),
            clamp(i64::from(self.window.1) + dy),
        ))
    }

    pub(super) fn moved(&self) -> bool {
        self.moved
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_click_threshold_scales_and_drag_stays_a_drag_after_returning_to_origin() {
        let mut drag = BallDrag::new((100, 100), (-500, 200), 1.5);
        assert_eq!(drag.update((106, 100)), None);
        assert!(!drag.moved());
        assert_eq!(drag.update((107, 100)), Some((-493, 200)));
        assert!(drag.moved());
        assert_eq!(drag.update((100, 100)), Some((-500, 200)));
        assert!(drag.moved());
    }

    #[test]
    fn test_diagonal_movement_and_large_negative_coordinates_do_not_overflow() {
        let mut drag = BallDrag::new((0, 0), (0, 0), 1.0);
        assert_eq!(drag.update((3, 3)), Some((3, 3)));
        let mut drag = BallDrag::new((i32::MIN, 0), (100, 0), 1.0);
        assert_eq!(drag.update((i32::MAX, 1)), Some((i32::MAX, 1)));
    }
}

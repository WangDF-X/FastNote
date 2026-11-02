use serde::{Deserialize, Serialize};

/// 逻辑像素矩形。屏幕位置以该屏幕的物理原点为锚点，偏移量为逻辑像素。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LogicalRect {
    /// 左侧坐标，可为负。
    pub x: f64,
    /// 顶部坐标，可为负。
    pub y: f64,
    /// 宽度。
    pub w: f64,
    /// 高度。
    pub h: f64,
}

/// 显示器工作区与缩放；物理原点锚定防止不同 DPI 屏幕的逻辑区域相互重叠。
#[derive(Clone, Debug, PartialEq)]
pub struct Monitor {
    /// Windows 设备标识，不含用户内容。
    pub name: String,
    /// 排除任务栏的逻辑工作区。
    pub work: LogicalRect,
    /// 物理屏幕原点 X（不是除以 scale 后的全局坐标）。
    pub origin_x: f64,
    /// 物理屏幕原点 Y。
    pub origin_y: f64,
    /// 逻辑像素到物理像素的倍率。
    pub scale: f64,
    /// 是否主显示器。
    pub primary: bool,
}

impl Monitor {
    /// 物理屏幕坐标转为原点锚定的逻辑坐标，支持负坐标与不同 DPI。
    pub fn to_logical(&self, x: f64, y: f64) -> (f64, f64) {
        (
            self.origin_x + (x - self.origin_x) / self.scale,
            self.origin_y + (y - self.origin_y) / self.scale,
        )
    }

    /// 逻辑位置转为实际物理屏幕位置，输入来自已校验的恢复/拖动规则。
    pub fn to_physical(&self, x: f64, y: f64) -> (f64, f64) {
        (
            self.origin_x + (x - self.origin_x) * self.scale,
            self.origin_y + (y - self.origin_y) * self.scale,
        )
    }
}

/// ARCH-04：普通窗口的还原矩形，不用最大化/最小化矩形覆盖。
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct WindowGeometry {
    /// 原点锚定的逻辑左侧坐标。
    pub x: f64,
    /// 原点锚定的逻辑顶部坐标。
    pub y: f64,
    /// 客户区逻辑宽度。
    pub w: f64,
    /// 客户区逻辑高度。
    pub h: f64,
    /// 是否恢复最大化；兼容旧 JSON 缺少字段。
    #[serde(default)]
    pub maximized: bool,
}

impl WindowGeometry {
    /// 数值、坐标范围与最小窗口尺寸是否合法。
    pub fn is_valid(self) -> bool {
        [self.x, self.y, self.w, self.h]
            .iter()
            .all(|v| v.is_finite())
            && self.x.abs() <= f64::from(i32::MAX)
            && self.y.abs() <= f64::from(i32::MAX)
            && self.w >= 1024.0
            && self.h >= 680.0
            && self.w <= f64::from(i32::MAX)
            && self.h <= f64::from(i32::MAX)
    }
}

/// 已按可见工作区校验的主窗口恢复结果。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ResolvedGeometry {
    /// 要应用的还原矩形和最大化状态。
    pub geometry: WindowGeometry,
    /// 原生窗口外框的物理 X。
    pub physical_x: f64,
    /// 原生窗口外框的物理 Y。
    pub physical_y: f64,
    /// 目标显示器缩放。
    pub scale: f64,
    /// 原配置是否已被校正。
    pub corrected: bool,
}

/// 悬浮球工具窗口位置，字段与 ARCH-04 appearance.ball_position 对应。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BallPosition {
    /// 原点锚定的逻辑左侧坐标。
    pub x: f64,
    /// 原点锚定的逻辑顶部坐标。
    pub y: f64,
    /// 显示器标识。
    pub monitor: String,
}

impl BallPosition {
    /// 位置数值与坐标范围是否合法。
    pub fn is_valid(&self) -> bool {
        self.x.is_finite()
            && self.y.is_finite()
            && self.x.abs() <= f64::from(i32::MAX)
            && self.y.abs() <= f64::from(i32::MAX)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mixed_dpi_position_round_trip_keeps_the_screen_origin() {
        let monitor = Monitor {
            name: "fake".to_owned(),
            origin_x: 2560.0,
            origin_y: -1080.0,
            work: LogicalRect {
                x: 2560.0,
                y: -1080.0,
                w: 1280.0,
                h: 720.0,
            },
            scale: 1.5,
            primary: false,
        };
        let logical = monitor.to_logical(2710.0, -930.0);
        assert_eq!(logical, (2660.0, -980.0));
        assert_eq!(monitor.to_physical(logical.0, logical.1), (2710.0, -930.0));
    }
}

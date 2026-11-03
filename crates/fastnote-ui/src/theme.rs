use slint::language::ColorScheme;
use slint::ComponentHandle;

use crate::{BallMenuWindow, FloatingBall, MainWindow, Palette, ShellPalette};

/// ARCH-05 §7：同步三个独立顶层组件的自绘调色板与 Slint 标准控件主题。
/// 必须从 UI 线程调用；隐藏组件也更新，确保重新显示的第一帧使用正确主题。
pub fn apply_theme(main: &MainWindow, ball: &FloatingBall, menu: &BallMenuWindow, dark: bool) {
    let scheme = if dark {
        ColorScheme::Dark
    } else {
        ColorScheme::Light
    };
    main.global::<ShellPalette>().set_dark(dark);
    ball.global::<ShellPalette>().set_dark(dark);
    menu.global::<ShellPalette>().set_dark(dark);
    main.global::<Palette>().set_color_scheme(scheme);
    ball.global::<Palette>().set_color_scheme(scheme);
    menu.global::<Palette>().set_color_scheme(scheme);
}

#[cfg(test)]
mod tests;

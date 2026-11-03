use slint::Color;

// 直接读取正式令牌，避免测试里的第二套色值与 UI 漂移；不创建或操作窗口。
fn token(name: &str, dark: bool) -> Color {
    let prefix = format!("out property <color> {name}:");
    let expression = include_str!("../../ui/palette.slint")
        .lines()
        .find_map(|line| line.trim().strip_prefix(&prefix))
        .unwrap();
    let (dark_value, light_value) = expression
        .trim()
        .strip_prefix("dark ? ")
        .unwrap()
        .trim_end_matches(';')
        .split_once(':')
        .unwrap();
    let hex = if dark { dark_value } else { light_value }
        .trim()
        .trim_start_matches('#');
    assert_eq!(hex.len(), 6, "expected RGB token {name}");
    Color::from_rgb_u8(
        u8::from_str_radix(&hex[0..2], 16).unwrap(),
        u8::from_str_radix(&hex[2..4], 16).unwrap(),
        u8::from_str_radix(&hex[4..6], 16).unwrap(),
    )
}

fn luminance(color: Color) -> f64 {
    let linear = |channel: u8| {
        let value = f64::from(channel) / 255.0;
        if value <= 0.04045 {
            value / 12.92
        } else {
            ((value + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * linear(color.red()) + 0.7152 * linear(color.green()) + 0.0722 * linear(color.blue())
}

fn contrast(foreground: Color, background: Color) -> f64 {
    let a = luminance(foreground);
    let b = luminance(background);
    (a.max(b) + 0.05) / (a.min(b) + 0.05)
}

#[test]
fn test_primary_and_secondary_text_meet_arch_05_contrast_in_both_themes() {
    for dark in [false, true] {
        for background in ["canvas", "surface", "subtle", "hover", "selected"] {
            for (foreground, minimum) in [("text-primary", 7.0), ("text-secondary", 4.5)] {
                let ratio = contrast(token(foreground, dark), token(background, dark));
                assert!(
                    ratio >= minimum,
                    "dark={dark}, {foreground}/{background}: {ratio:.2}"
                );
            }
        }
    }
}

#[test]
fn test_filled_button_labels_remain_legible_in_default_hover_and_pressed_states() {
    for dark in [false, true] {
        for background in ["accent", "danger"] {
            let factors = if dark {
                [0.0, 0.05, 0.1]
            } else {
                [0.0, 0.1, 0.25]
            };
            for factor in factors {
                let ratio = contrast(
                    token("on-accent", dark),
                    token(background, dark).darker(factor),
                );
                assert!(
                    ratio >= 4.5,
                    "dark={dark}, on-accent/{background}, darker={factor}: {ratio:.2}"
                );
            }
        }
    }
}

#[test]
fn test_task_calendar_and_counter_text_meet_contrast_in_both_themes() {
    for dark in [false, true] {
        for (foreground, background) in [
            ("text-primary", "surface"),
            ("text-primary", "selected"),
            ("text-secondary", "danger-subtle"),
            ("danger", "danger-subtle"),
            ("success", "surface"),
            ("warning", "surface"),
            ("on-accent", "accent"),
            ("on-accent", "success"),
        ] {
            let ratio = contrast(token(foreground, dark), token(background, dark));
            assert!(
                ratio >= 4.5,
                "dark={dark}, {foreground}/{background}: {ratio:.2}"
            );
        }
    }
}

#[test]
fn test_pastel_task_rows_keep_readable_text_in_default_hover_and_pressed_states() {
    let style = include_str!("../../ui/tasks/task-style.slint");
    for expression in [
        "surface.brighter(0.08) : surface.darker(0.02)",
        "surface.brighter(0.04) : surface.darker(0.01)",
        "ShellPalette.danger.brighter(0.04) : ShellPalette.danger.darker(0.08)",
        "ShellPalette.warning.brighter(0.04) : ShellPalette.warning.darker(0.08)",
    ] {
        assert!(style.contains(expression));
    }
    for dark in [false, true] {
        let readable_semantic = |name| {
            let color = token(name, dark);
            if dark {
                color.brighter(0.04)
            } else {
                color.darker(0.08)
            }
        };
        for name in [
            "task-todo",
            "task-progress",
            "task-waiting",
            "task-done",
            "danger-subtle",
        ] {
            let base = token(name, dark);
            assert!(
                contrast(base, token("surface", dark)) <= 1.4,
                "{name} is too strong"
            );
            let states = if dark {
                [base, base.brighter(0.04), base.brighter(0.08)]
            } else {
                [base, base.darker(0.01), base.darker(0.02)]
            };
            for background in states {
                for (foreground, minimum) in [
                    (token("text-primary", dark), 7.0),
                    (token("text-secondary", dark), 4.5),
                    (readable_semantic("danger"), 4.5),
                    (readable_semantic("warning"), 4.5),
                ] {
                    let ratio = contrast(foreground, background);
                    assert!(
                        ratio >= minimum,
                        "dark={dark}, {name}: {ratio:.2} < {minimum}"
                    );
                }
            }
        }
        for name in ["surface", "hover", "subtle"] {
            for semantic in ["danger", "warning"] {
                assert!(contrast(readable_semantic(semantic), token(name, dark)) >= 4.5);
            }
        }
    }
}

#[test]
fn test_all_eight_section_surfaces_have_readable_neutral_text_and_semantic_icons() {
    let section_surfaces = [
        ("danger-subtle", "danger"),
        ("task-progress", "accent"),
        ("task-progress", "accent"),
        ("task-progress", "accent"),
        ("task-waiting", "warning"),
        ("task-todo", "text-secondary"),
        ("task-progress", "accent"),
        ("task-done", "success"),
    ];
    let style = include_str!("../../ui/tasks/task-style.slint");
    for expression in [
        "kind == 0 ? ShellPalette.danger-subtle",
        "kind == 4 ? ShellPalette.task-waiting",
        "kind == 5 ? ShellPalette.task-todo",
        "kind == 7 ? ShellPalette.task-done",
    ] {
        assert!(style.contains(expression));
    }
    for dark in [false, true] {
        for (surface, icon) in section_surfaces {
            let base = token(surface, dark);
            let icon_color = match icon {
                "danger" | "warning" if dark => token(icon, dark).brighter(0.04),
                "danger" | "warning" => token(icon, dark).darker(0.08),
                _ => token(icon, dark),
            };
            let states = if dark {
                [base, base.brighter(0.04), base.brighter(0.08)]
            } else {
                [base, base.darker(0.01), base.darker(0.02)]
            };
            for background in states {
                assert!(contrast(token("text-primary", dark), background) >= 7.0);
                assert!(contrast(token("text-secondary", dark), background) >= 4.5);
                assert!(contrast(icon_color, background) >= 3.0);
            }
        }
    }
}

#[test]
fn test_caption_close_pressed_label_meets_contrast_in_both_themes() {
    for dark in [false, true] {
        assert!(contrast(token("on-accent", dark), token("danger", dark).darker(0.15)) >= 4.5);
    }
}

#[test]
fn test_caption_buttons_are_inside_the_application_shortcut_scope() {
    let source = include_str!("../../ui/app-window.slint");
    let scope = source.find("shortcuts := FocusScope").unwrap();
    let caption = source.find("if root.custom-titlebar: TitleBar").unwrap();
    let body = source[scope..].find("HorizontalLayout {").unwrap() + scope;
    assert!(scope < caption && caption < body);
    assert!(source[scope..caption].contains("root.route_action(\"desktop:close\")"));
}

#[test]
fn test_caption_buttons_are_fixed_to_the_right_edge_without_a_stretching_layout() {
    // 接线/定位契约检查，不启动窗口或模拟鼠标；实际显示仍交给用户验收。
    let source = include_str!("../../ui/components/title-bar.slint");
    assert!(!source.contains("HorizontalLayout"));
    for (id, inset, action) in [
        ("minimize-button", 138, "minimize"),
        ("maximize-button", 92, "maximize"),
        ("close-button", 46, "close"),
    ] {
        let start = source.find(&format!("{id} := CaptionButton {{")).unwrap();
        let block = source[start..].split_once("\n    }").unwrap().0;
        assert!(block.contains(&format!("x: root.width - {inset}px; y: 0px;")));
        assert!(block.contains(&format!("root.{action}();")));
    }
}

#[test]
fn test_theme_control_is_only_at_the_right_of_the_settings_row() {
    let source = include_str!("../../ui/app-window.slint");
    let settings = source.find("focus-key: \"nav:设置\"").unwrap();
    let end = source[settings..].find("ThemeSwitch {").unwrap() + settings;
    assert!(!source.contains("sidebar-theme"));
    assert_eq!(source.matches("ThemeSwitch {").count(), 1);
    // 同一行：设置项收窄，按钮贴右且垂直居中（行高 64、按钮 36）。
    let row_start = source[..end].rfind("height: 64px;").unwrap();
    assert!(source[row_start..end].contains("width: parent.width - 64px;"));
    let block = &source[end..];
    assert!(block.contains("x: parent.width - 48px;"));
    assert!(block.contains("y: 14px;"));
    assert!(block.contains("mode: root.theme-mode;"));
    assert!(block.contains("root.route_action(\"desktop:theme:cycle\")"));
    assert!(block.contains("root.desktop-state == 1 && !root.desktop-busy && !root.dialog-visible"));
    assert!(!include_str!("../../ui/components/general-settings.slint").contains("ThemeSwitch"));
}

#[test]
fn test_theme_switch_is_one_icon_button_with_three_state_images_and_no_visible_text() {
    let source = include_str!("../../ui/components/theme-switch.slint");
    assert!(source.contains("width: 36px;"));
    assert!(source.contains("height: 36px;"));
    assert!(source.contains("accessible-role: button;"));
    assert!(source.contains("accessible-label:"));
    assert!(!source.contains("Text {"));
    assert!(!source.contains("radio-button"));
    assert_eq!(source.matches("Image {").count(), 1);
    for mode in ["system", "light", "dark"] {
        assert!(source.contains(&format!("theme-{mode}.svg")));
    }
    assert!(source.contains("source: root.mode == \"light\""));
    assert!(source.contains("root.mode == \"dark\" ? @image-url"));
    assert!(source.contains("ShellFocus.claim(\"theme-switch\")"));
    assert!(source.contains("root.cycle();"));
    assert!(source.contains("event.text == \"\\n\" || event.text == \" \""));
}

#[test]
fn test_brand_images_use_existing_reversed_asset_only_for_effective_dark_theme() {
    let palette = include_str!("../../ui/palette.slint");
    for (name, light_asset) in [
        ("brand-icon", "fastnote-app-icon.svg"),
        ("brand-icon-small", "fastnote-app-icon-small.svg"),
    ] {
        let prefix = format!("out property <image> {name}:");
        let expression = palette
            .lines()
            .find_map(|line| line.trim().strip_prefix(&prefix))
            .unwrap();
        let expected = format!(
            "dark ? @image-url(\"../../../ui/branding/fastnote-app-icon-reversed.svg\") : @image-url(\"../../../ui/branding/{light_asset}\");"
        );
        assert_eq!(expression.trim(), expected);
    }
    let main = include_str!("../../ui/app-window.slint");
    assert!(main.contains("icon: ShellPalette.brand-icon;"));
    assert!(main.contains("source: ShellPalette.brand-icon-small;"));
    for source in [
        include_str!("../../ui/components/title-bar.slint"),
        include_str!("../../ui/floating-ball.slint"),
    ] {
        assert!(source.contains("source: ShellPalette.brand-icon-small;"));
    }
}

#[test]
fn test_palette_bridge_updates_every_top_level_component_and_standard_widgets() {
    // 静态接线回归检查；仅验证全局同步接口存在，不宣称界面或交互已验收。
    let source = include_str!("../theme.rs");
    for component in ["main", "ball", "menu"] {
        assert!(source.contains(&format!(
            "{component}.global::<ShellPalette>().set_dark(dark)"
        )));
        assert!(source.contains(&format!(
            "{component}.global::<Palette>().set_color_scheme(scheme)"
        )));
    }
}

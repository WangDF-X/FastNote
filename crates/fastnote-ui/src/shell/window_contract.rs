// D-SHELL-01：仅检查被动边缘和布局接线，不创建窗口；真实圆角/阴影由用户验收。

#[test]
fn test_window_outline_is_passive_and_only_visible_for_a_windowed_custom_caption() {
    let window = include_str!("../../ui/app-window.slint");
    let outline = window.find("window-outline := WindowOutline {").unwrap();
    assert!(window.find("DialogHost {").unwrap() < outline);
    let block = window[outline..].split_once('\n').unwrap().1;
    let block = block.split_once('}').unwrap().0;
    for expected in [
        "x: 0px; y: 0px;",
        "width: parent.width; height: parent.height;",
        "visible: root.custom-titlebar && !root.window-maximized;",
    ] {
        assert!(block.contains(expected));
    }
    let source = include_str!("../../ui/components/window-outline.slint");
    assert!(source.contains("border-radius: 8px;"));
    assert!(source.contains("border-width: 1px;"));
    assert!(source.contains("border-color: ShellPalette.border-strong;"));
    assert!(source.contains("background: transparent;"));
    assert!(source.contains("accessible-role: none;"));
    for input in [
        "TouchArea",
        "FocusScope",
        "key-pressed",
        "forward-focus",
        "drop-shadow",
    ] {
        assert!(!source.contains(input), "{input}");
    }
}

#[test]
fn test_window_frame_keeps_opaque_canvas_client_dimensions_and_native_resize_hot_zones() {
    let window = include_str!("../../ui/app-window.slint");
    let properties = window
        .split_once("export component MainWindow inherits Window {")
        .unwrap()
        .1;
    let properties = properties
        .split_once("in property <string> current_page:")
        .unwrap()
        .0;
    for expected in [
        "preferred-width: 1280px;",
        "preferred-height: 800px;",
        "min-width: 1024px;",
        "min-height: 680px;",
        "background: ShellPalette.canvas;",
        "no-frame: root.custom-titlebar;",
        "resize-border-width: root.window-maximized ? 0px : 6px;",
    ] {
        assert!(properties.contains(expected));
    }
    assert!(!properties.contains("background: transparent;"));
    let layout = window
        .split_once("if root.custom-titlebar: TitleBar {")
        .unwrap()
        .1;
    assert!(layout.contains("height: root.caption-height;"));
    assert!(layout.contains("y: root.caption-height;"));
    assert!(layout.contains("height: parent.height - self.y;"));
}

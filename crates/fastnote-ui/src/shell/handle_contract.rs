#[test]
fn test_shell_hide_only_hides_and_pauses_without_dropping_component_or_models() {
    let source = include_str!("handle.rs");
    let hide = source
        .split_once("pub fn hide(")
        .unwrap()
        .1
        .split_once("pub fn pause_timers(")
        .unwrap()
        .0;
    assert!(hide.contains("self.window.hide()?;"), "shell/handle.rs");
    assert!(hide.contains("self.pause_timers();"), "shell/handle.rs");
    assert!(!hide.contains(".take()"), "shell/handle.rs");
    assert!(!hide.contains("drop("), "shell/handle.rs");
    assert!(!hide.contains("MainWindow::new"), "shell/handle.rs");
    assert!(source.contains("hide` 不是卸载"), "shell/handle.rs");
}

#[test]
fn test_shell_owns_component_and_feedback_until_handle_is_dropped() {
    let source = include_str!("handle.rs");
    let fields = source
        .split_once("pub struct ShellHandle {")
        .unwrap()
        .1
        .split_once("impl ShellHandle {")
        .unwrap()
        .0;
    assert!(fields.contains("window: MainWindow"), "shell/handle.rs");
    assert!(fields.contains("shell: Rc<Shell>"), "shell/handle.rs");
    let shell = include_str!("mod.rs")
        .split_once("struct Shell {")
        .unwrap()
        .1
        .split_once("/// Starts")
        .unwrap()
        .0;
    assert!(
        shell.contains("window: slint::Weak<MainWindow>"),
        "shell/mod.rs"
    );
    assert!(shell.contains("toast_timer: Timer"), "shell/mod.rs");
}

#[test]
fn test_component_access_only_borrows_window_and_keeps_public_signature() {
    let source = include_str!("handle.rs");
    let component = source
        .split_once("pub fn component(")
        .unwrap()
        .1
        .split_once("pub fn show(")
        .unwrap()
        .0;
    let compact = component.split_whitespace().collect::<String>();
    assert!(
        compact.starts_with("&self)->&MainWindow{"),
        "shell/handle.rs"
    );
    assert!(compact.contains("&self.window"), "shell/handle.rs");
    assert!(!compact.contains(".clone()"), "shell/handle.rs");
    assert!(!compact.contains("MainWindow::new"), "shell/handle.rs");
}

use std::cell::RefCell;

use super::*;

#[test]
fn test_theme_snapshot_does_not_keep_kernel_borrow_across_projection() {
    let state = RefCell::new(ThemeState::default());
    state.borrow_mut().preview = Some(ThemeMode::Dark);
    let snapshot = state.borrow().snapshot(ThemeMode::Light);
    state.borrow_mut().preview = None;
    assert_eq!(snapshot.mode, ThemeMode::Dark);
    assert!(snapshot.dark);
    assert_eq!(state.borrow().selected(ThemeMode::Light), ThemeMode::Light);
}

#[test]
fn test_missing_ui_theme_projection_updates_entry_palettes_without_creating_main() {
    let source = include_str!("../theme.rs");
    let absent = source
        .split_once("if projected.is_none() {")
        .unwrap()
        .1
        .split_once("pub(super) fn apply_main_caption(")
        .unwrap()
        .0;
    let absent = absent.split_whitespace().collect::<String>();
    assert!(absent.contains("self.ball.global::<fastnote_ui::ShellPalette>()"));
    assert!(absent.contains("self.menu.global::<fastnote_ui::ShellPalette>()"));
    assert!(absent.contains("self.ball.global::<fastnote_ui::Palette>()"));
    assert!(absent.contains("self.menu.global::<fastnote_ui::Palette>()"));
    assert!(!absent.contains("ShellHandle::new"));
    assert!(!absent.contains("MainWindow::new"));
}

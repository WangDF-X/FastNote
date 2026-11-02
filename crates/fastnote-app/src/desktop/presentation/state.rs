use std::cell::Cell;

use fastnote_core::EntryMode;
use thiserror::Error;

/// 一次只允许主窗或球之一作为可见呈现；托盘图标独立于窗口呈现。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::desktop) enum Presentation {
    Hidden,
    Main,
    Ball,
}

pub(super) trait Surfaces {
    fn hide(&self, view: Presentation) -> Result<(), ()>;
    fn show(&self, view: Presentation) -> Result<(), ()>;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Error)]
pub(in crate::desktop) enum SwitchError {
    #[error("source presentation could not be hidden")]
    HideSource,
    #[error("target presentation could not be shown")]
    ShowTarget,
    #[error("failed target presentation could not be hidden")]
    CleanupTarget,
    #[error("source presentation could not be restored")]
    RestoreSource,
}

/// 原呈现成功隐藏后才显示目标；失败目标清理成功后才恢复原呈现。
/// 清理失败时保守保留该呈现状态，不再显示第二个窗口。
pub(super) fn switch(
    surfaces: &impl Surfaces,
    state: &Cell<Presentation>,
    target: Presentation,
) -> Result<(), SwitchError> {
    let source = state.get();
    if source == target {
        return Ok(());
    }
    surfaces.hide(source).map_err(|_| SwitchError::HideSource)?;
    state.set(Presentation::Hidden);
    if surfaces.show(target).is_ok() {
        state.set(target);
        return Ok(());
    }
    if surfaces.hide(target).is_err() {
        state.set(target);
        return Err(SwitchError::CleanupTarget);
    }
    if surfaces.show(source).is_ok() {
        state.set(source);
        return Err(SwitchError::ShowTarget);
    }
    if surfaces.hide(source).is_err() {
        state.set(source);
    }
    Err(SwitchError::RestoreSource)
}

/// 配置球模式不意味着立即显示球；主窗可见时保持主窗呈现。
pub(in crate::desktop) fn entry_view(current: Presentation, mode: EntryMode) -> Presentation {
    match (current, mode) {
        (Presentation::Main, _) => Presentation::Main,
        (_, EntryMode::Ball) => Presentation::Ball,
        (_, EntryMode::Tray) => Presentation::Hidden,
    }
}

/// 系统最小化只在球模式切换呈现；托盘模式维持原生任务栏行为。
pub(in crate::desktop) fn minimize_to_ball(
    current: Presentation,
    entry: Option<EntryMode>,
) -> bool {
    current == Presentation::Main && entry == Some(EntryMode::Ball)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::desktop) enum MinimizeAction {
    Ignore,
    Pause,
    Resume,
    RestoreMain,
    Collapse,
}

/// 保存阻塞时暂停并等待下一次状态核验；用户已恢复窗口则不执行过期的收起请求。
pub(in crate::desktop) fn minimize_action(
    current: Presentation,
    entry: Option<EntryMode>,
    minimized: Option<bool>,
    ready: bool,
    blocked: bool,
) -> MinimizeAction {
    if current != Presentation::Main {
        return MinimizeAction::Ignore;
    }
    match minimized {
        Some(false) => MinimizeAction::Resume,
        Some(true) if minimize_to_ball(current, entry) && !ready => MinimizeAction::RestoreMain,
        Some(true) if minimize_to_ball(current, entry) && !blocked => MinimizeAction::Collapse,
        _ => MinimizeAction::Pause,
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use super::*;

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    enum Operation {
        Hide(Presentation),
        Show(Presentation),
    }

    struct FakeSurfaces {
        main: Cell<bool>,
        ball: Cell<bool>,
        operations: RefCell<Vec<Operation>>,
        failures: RefCell<Vec<Operation>>,
    }

    impl FakeSurfaces {
        fn new(current: Presentation, failures: Vec<Operation>) -> Self {
            Self {
                main: Cell::new(current == Presentation::Main),
                ball: Cell::new(current == Presentation::Ball),
                operations: RefCell::new(Vec::new()),
                failures: RefCell::new(failures),
            }
        }

        fn set_visible(&self, view: Presentation, visible: bool) {
            match view {
                Presentation::Main => self.main.set(visible),
                Presentation::Ball => self.ball.set(visible),
                Presentation::Hidden => {}
            }
            assert!(!(self.main.get() && self.ball.get()));
        }

        fn should_fail(&self, operation: Operation) -> bool {
            let mut failures = self.failures.borrow_mut();
            if failures.first() == Some(&operation) {
                failures.remove(0);
                true
            } else {
                false
            }
        }
    }

    impl Surfaces for FakeSurfaces {
        fn hide(&self, view: Presentation) -> Result<(), ()> {
            let operation = Operation::Hide(view);
            self.operations.borrow_mut().push(operation);
            if self.should_fail(operation) {
                return Err(());
            }
            self.set_visible(view, false);
            Ok(())
        }

        fn show(&self, view: Presentation) -> Result<(), ()> {
            let operation = Operation::Show(view);
            self.operations.borrow_mut().push(operation);
            // 模拟部分创建后报错；必须清理它才能恢复原呈现。
            self.set_visible(view, true);
            if self.should_fail(operation) {
                return Err(());
            }
            Ok(())
        }
    }

    #[test]
    fn test_successful_switch_never_shows_ball_and_main_together() {
        for (source, target) in [
            (Presentation::Main, Presentation::Ball),
            (Presentation::Ball, Presentation::Main),
            (Presentation::Main, Presentation::Hidden),
            (Presentation::Hidden, Presentation::Main),
            (Presentation::Hidden, Presentation::Ball),
        ] {
            let surfaces = FakeSurfaces::new(source, vec![]);
            let state = Cell::new(source);
            assert_eq!(switch(&surfaces, &state, target), Ok(()));
            assert_eq!(state.get(), target);
            assert_eq!(
                *surfaces.operations.borrow(),
                [Operation::Hide(source), Operation::Show(target)]
            );
        }
    }

    #[test]
    fn test_failed_source_hide_keeps_original_and_never_shows_target() {
        let surfaces = FakeSurfaces::new(
            Presentation::Main,
            vec![Operation::Hide(Presentation::Main)],
        );
        let state = Cell::new(Presentation::Main);
        assert_eq!(
            switch(&surfaces, &state, Presentation::Ball),
            Err(SwitchError::HideSource)
        );
        assert_eq!(state.get(), Presentation::Main);
        assert!(surfaces.main.get());
        assert!(!surfaces.ball.get());
        assert_eq!(
            *surfaces.operations.borrow(),
            [Operation::Hide(Presentation::Main)]
        );
    }

    #[test]
    fn test_failed_target_is_cleaned_before_original_view_is_restored() {
        let surfaces = FakeSurfaces::new(
            Presentation::Main,
            vec![Operation::Show(Presentation::Ball)],
        );
        let state = Cell::new(Presentation::Main);
        assert_eq!(
            switch(&surfaces, &state, Presentation::Ball),
            Err(SwitchError::ShowTarget)
        );
        assert_eq!(state.get(), Presentation::Main);
        assert!(surfaces.main.get());
        assert!(!surfaces.ball.get());
        assert_eq!(
            *surfaces.operations.borrow(),
            [
                Operation::Hide(Presentation::Main),
                Operation::Show(Presentation::Ball),
                Operation::Hide(Presentation::Ball),
                Operation::Show(Presentation::Main),
            ]
        );
    }

    #[test]
    fn test_failed_target_cleanup_does_not_restore_a_second_visible_view() {
        let surfaces = FakeSurfaces::new(
            Presentation::Main,
            vec![
                Operation::Show(Presentation::Ball),
                Operation::Hide(Presentation::Ball),
            ],
        );
        let state = Cell::new(Presentation::Main);
        assert_eq!(
            switch(&surfaces, &state, Presentation::Ball),
            Err(SwitchError::CleanupTarget)
        );
        assert_eq!(state.get(), Presentation::Ball);
        assert!(surfaces.ball.get());
        assert!(!surfaces.main.get());
    }

    #[test]
    fn test_failed_restore_is_cleaned_without_leaving_two_visible_views() {
        for cleanup_fails in [false, true] {
            let mut failures = vec![
                Operation::Show(Presentation::Main),
                Operation::Show(Presentation::Ball),
            ];
            if cleanup_fails {
                failures.push(Operation::Hide(Presentation::Ball));
            }
            let surfaces = FakeSurfaces::new(Presentation::Ball, failures);
            let state = Cell::new(Presentation::Ball);
            assert_eq!(
                switch(&surfaces, &state, Presentation::Main),
                Err(SwitchError::RestoreSource)
            );
            assert_eq!(
                state.get(),
                if cleanup_fails {
                    Presentation::Ball
                } else {
                    Presentation::Hidden
                }
            );
            assert!(!surfaces.main.get());
        }
    }

    #[test]
    fn test_same_presentation_does_not_hide_or_recreate_its_window() {
        let surfaces = FakeSurfaces::new(Presentation::Main, vec![]);
        let state = Cell::new(Presentation::Main);
        assert_eq!(switch(&surfaces, &state, Presentation::Main), Ok(()));
        assert!(surfaces.operations.borrow().is_empty());
    }

    #[test]
    fn test_ball_mode_configuration_keeps_an_existing_main_window() {
        for mode in [EntryMode::Ball, EntryMode::Tray] {
            assert_eq!(entry_view(Presentation::Main, mode), Presentation::Main);
        }
        assert_eq!(
            entry_view(Presentation::Hidden, EntryMode::Ball),
            Presentation::Ball
        );
        assert_eq!(
            entry_view(Presentation::Ball, EntryMode::Tray),
            Presentation::Hidden
        );
    }

    #[test]
    fn test_system_minimize_collapses_only_a_main_window_in_ball_mode() {
        assert!(minimize_to_ball(Presentation::Main, Some(EntryMode::Ball)));
        for (current, entry) in [
            (Presentation::Main, Some(EntryMode::Tray)),
            (Presentation::Main, None),
            (Presentation::Ball, Some(EntryMode::Ball)),
            (Presentation::Hidden, Some(EntryMode::Ball)),
        ] {
            assert!(!minimize_to_ball(current, entry));
        }
    }

    #[test]
    fn test_minimize_waits_for_save_and_native_restore_cancels_the_pending_collapse() {
        let entry = Some(EntryMode::Ball);
        let current = Presentation::Main;
        assert_eq!(
            minimize_action(current, entry, Some(true), true, true),
            MinimizeAction::Pause
        );
        assert_eq!(
            minimize_action(current, entry, Some(true), true, false),
            MinimizeAction::Collapse
        );
        assert_eq!(
            minimize_action(current, entry, Some(false), true, false),
            MinimizeAction::Resume
        );
    }

    #[test]
    fn test_unavailable_settings_restore_ball_mode_main_instead_of_hiding_the_error() {
        assert_eq!(
            minimize_action(
                Presentation::Main,
                Some(EntryMode::Ball),
                Some(true),
                false,
                false
            ),
            MinimizeAction::RestoreMain
        );
        assert_eq!(
            minimize_action(
                Presentation::Main,
                Some(EntryMode::Tray),
                Some(true),
                true,
                false
            ),
            MinimizeAction::Pause
        );
        assert_eq!(
            minimize_action(
                Presentation::Hidden,
                Some(EntryMode::Ball),
                Some(true),
                true,
                false
            ),
            MinimizeAction::Ignore
        );
        assert_eq!(
            minimize_action(Presentation::Main, Some(EntryMode::Ball), None, true, false),
            MinimizeAction::Pause
        );
    }
}

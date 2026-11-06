//! show 被接受不代表 HWND 已创建。原生就绪后才还原/激活，不把创建延迟当作失败。

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(in crate::desktop) enum MainActivation {
    #[default]
    Idle,
    Waiting {
        restore_geometry: bool,
    },
    Restoring,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ActivationStep {
    Wait,
    Configure { restore_geometry: bool },
    Restore,
    Activate,
}

/// None 代表后端 show 已登记、HWND 未创建；只有实际配置错误才返回 Err。
pub(in crate::desktop) fn native_configuration(result: Option<Result<(), ()>>) -> Result<bool, ()> {
    result.transpose().map(|ready| ready.is_some())
}

impl MainActivation {
    pub(in crate::desktop) fn request(&mut self, restore_geometry: bool) {
        let restore_geometry = restore_geometry
            || matches!(
                self,
                Self::Waiting {
                    restore_geometry: true
                }
            );
        *self = Self::Waiting { restore_geometry };
    }

    pub(super) fn advance(
        &mut self,
        main: bool,
        native: bool,
        minimized: bool,
        visible: bool,
    ) -> ActivationStep {
        if !main {
            *self = Self::Idle;
            return ActivationStep::Wait;
        }
        if !native {
            return ActivationStep::Wait;
        }
        match *self {
            Self::Idle => ActivationStep::Wait,
            Self::Waiting { restore_geometry } => {
                *self = Self::Restoring;
                ActivationStep::Configure { restore_geometry }
            }
            Self::Restoring if minimized => ActivationStep::Restore,
            Self::Restoring if !visible => ActivationStep::Wait,
            Self::Restoring => {
                *self = Self::Idle;
                ActivationStep::Activate
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::state::{switch, Presentation, Surfaces};
    use super::*;
    use std::cell::Cell;

    struct DelayedSurfaces {
        main: Cell<bool>,
        ball: Cell<bool>,
        native_ball: Cell<bool>,
        fail_ball_configuration: bool,
    }

    impl Surfaces for DelayedSurfaces {
        fn hide(&self, view: Presentation) -> Result<(), ()> {
            match view {
                Presentation::Main => self.main.set(false),
                Presentation::Ball => {
                    self.ball.set(false);
                    self.native_ball.set(false);
                }
                Presentation::Hidden => {}
            }
            Ok(())
        }
        fn show(&self, view: Presentation) -> Result<(), ()> {
            match view {
                Presentation::Main => self.main.set(true),
                Presentation::Ball => {
                    let result =
                        self.native_ball
                            .get()
                            .then_some(if self.fail_ball_configuration {
                                Err(())
                            } else {
                                Ok(())
                            });
                    native_configuration(result)?;
                    self.ball.set(true);
                }
                Presentation::Hidden => {}
            }
            assert!(!(self.main.get() && self.ball.get()));
            Ok(())
        }
    }

    #[test]
    fn test_five_main_ball_switches_accept_delayed_hwnd_without_false_rollback() {
        let surfaces = DelayedSurfaces {
            main: Cell::new(true),
            ball: Cell::new(false),
            native_ball: Cell::new(false),
            fail_ball_configuration: false,
        };
        let state = Cell::new(Presentation::Main);
        let mut activation = MainActivation::Idle;
        for _ in 0..5 {
            assert_eq!(switch(&surfaces, &state, Presentation::Ball), Ok(()));
            assert_eq!(state.get(), Presentation::Ball);
            assert!(!surfaces.main.get());
            assert!(!surfaces.native_ball.get());
            surfaces.native_ball.set(true);
            assert_eq!(switch(&surfaces, &state, Presentation::Main), Ok(()));
            activation.request(true);
            assert_eq!(
                activation.advance(true, false, false, false),
                ActivationStep::Wait
            );
            assert_eq!(
                activation.advance(true, true, false, true),
                ActivationStep::Configure {
                    restore_geometry: true
                }
            );
            assert_eq!(
                activation.advance(true, true, false, true),
                ActivationStep::Activate
            );
        }
    }

    #[test]
    fn test_actual_ball_configuration_failure_still_rolls_back_to_main() {
        let surfaces = DelayedSurfaces {
            main: Cell::new(true),
            ball: Cell::new(false),
            native_ball: Cell::new(true),
            fail_ball_configuration: true,
        };
        let state = Cell::new(Presentation::Main);
        assert!(switch(&surfaces, &state, Presentation::Ball).is_err());
        assert_eq!(state.get(), Presentation::Main);
        assert!(surfaces.main.get());
    }

    #[test]
    fn test_production_events_resume_native_configuration_and_activate_only_after_readiness() {
        let ball = include_str!("../ball.rs");
        assert!(ball.contains("native_configuration(self.configure_ball_native())?"));
        assert!(ball.contains("self.ball_native_pending.set(!ready)"));
        let events = include_str!("../window.rs");
        assert!(events.contains("runtime.ball_native_pending.get()"));
        assert!(events.contains("runtime.complete_entry_native()"));
        let main_events = include_str!("../window/ui_events.rs");
        assert!(main_events.contains("runtime.main_activation_pending()"));
        assert!(main_events.contains("runtime.defer_main_activation()"));
        let native = include_str!("native.rs");
        let configure = native
            .split_once("ActivationStep::Configure { restore_geometry } =>")
            .unwrap()
            .1
            .split_once("ActivationStep::Restore =>")
            .unwrap()
            .0;
        assert!(configure.contains("set_minimized(false)"));
        assert!(!configure.contains("focus_window()"));
        let activate = native.split_once("ActivationStep::Activate =>").unwrap().1;
        assert!(activate.contains("focus_window()"));
        assert!(activate.contains("self.start_page_tasks()"));
    }

    #[test]
    fn test_pending_native_creation_is_not_a_target_failure_but_real_errors_propagate() {
        assert_eq!(native_configuration(None), Ok(false));
        assert_eq!(native_configuration(Some(Ok(()))), Ok(true));
        assert_eq!(native_configuration(Some(Err(()))), Err(()));
    }

    #[test]
    fn test_delayed_hwnd_never_consumes_restore_or_focus_intent() {
        let mut state = MainActivation::Idle;
        state.request(true);
        for _ in 0..5 {
            assert_eq!(
                state.advance(true, false, false, false),
                ActivationStep::Wait
            );
        }
        assert_eq!(
            state.advance(true, true, false, true),
            ActivationStep::Configure {
                restore_geometry: true
            }
        );
        assert_eq!(
            state.advance(true, true, false, true),
            ActivationStep::Activate
        );
        assert_eq!(state.advance(true, true, false, true), ActivationStep::Wait);
    }

    #[test]
    fn test_minimized_native_window_is_restored_before_one_activation() {
        let mut state = MainActivation::Idle;
        state.request(false);
        assert_eq!(
            state.advance(true, true, true, true),
            ActivationStep::Configure {
                restore_geometry: false
            }
        );
        assert_eq!(
            state.advance(true, true, true, true),
            ActivationStep::Restore
        );
        assert_eq!(
            state.advance(true, true, false, false),
            ActivationStep::Wait
        );
        assert_eq!(
            state.advance(true, true, false, true),
            ActivationStep::Activate
        );
        assert_eq!(state, MainActivation::Idle);
    }

    #[test]
    fn test_hiding_cancels_old_focus_and_reopen_requires_a_new_request() {
        let mut state = MainActivation::Idle;
        state.request(true);
        assert_eq!(
            state.advance(false, true, false, true),
            ActivationStep::Wait
        );
        assert_eq!(state.advance(true, true, false, true), ActivationStep::Wait);
        state.request(true);
        state.request(false);
        assert_eq!(
            state.advance(true, true, false, true),
            ActivationStep::Configure {
                restore_geometry: true
            }
        );
    }
}

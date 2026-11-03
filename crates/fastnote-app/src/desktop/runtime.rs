use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::{Rc, Weak};
use std::sync::Arc;

use crossbeam_channel::{bounded, Receiver};
use fastnote_core::{
    BallPosition, DesktopSetting, DesktopSettings, EntryMode, ResolvedGeometry, WindowGeometry,
};
use fastnote_platform::{
    install_event_handlers, NativeEvent, WindowsHotkey, WindowsThemeMonitor, WindowsTray,
};
use fastnote_ui::{
    BallMenuWindow, FloatingBall, ShellHandle, ShellUpdate, ToastKind, ToastRequest,
};
use slint::{ComponentHandle, Timer};

use crate::{clock::SystemClock, controller::AppController};

use super::gesture::BallDrag;
use super::presentation::Presentation;
use super::theme::ThemeState;
use super::worker::{Reply, SettingsWorker};

pub(super) enum Completion {
    Background,
    Preference { previous: DesktopSetting },
    Rollback,
    Close { exit: bool },
}

pub(super) struct PendingWrite {
    pub changes: Vec<DesktopSetting>,
    pub completion: Completion,
}

pub(super) struct Runtime {
    pub shell: ShellHandle,
    pub ball: FloatingBall,
    pub menu: BallMenuWindow,
    pub controller: RefCell<AppController<SystemClock>>,
    pub settings: RefCell<DesktopSettings>,
    pub theme: RefCell<ThemeState>,
    pub theme_monitor: RefCell<Option<WindowsThemeMonitor>>,
    pub worker: SettingsWorker,
    pub native: Receiver<NativeEvent>,
    pub tray: RefCell<Option<WindowsTray>>,
    pub hotkey: RefCell<Option<WindowsHotkey>>,
    pub entry: Cell<Option<EntryMode>>,
    pub ready: Cell<bool>,
    pub initialized: Cell<bool>,
    pub hidden_start: bool,
    pub started: std::time::Instant,
    pub presentation: Cell<Presentation>,
    pub busy: Cell<bool>,
    pub closing: Cell<bool>,
    pub finished: Cell<bool>,
    pub force_allowed: Cell<bool>,
    pub preference_repair: Cell<bool>,
    pub save_failed: Cell<bool>,
    pub geometry: Cell<Option<WindowGeometry>>,
    pub restore: Cell<Option<ResolvedGeometry>>,
    pub ball_position: RefCell<Option<BallPosition>>,
    pub geometry_timer: Timer,
    /// 已设置过大图标的 HWND；窗口重建后 HWND 变化，用于避免每次 Resized 重复设置。
    pub main_icon: Cell<isize>,
    pub main_caption: Cell<isize>,
    pub menu_open: Cell<bool>,
    pub drag: RefCell<Option<BallDrag>>,
    pub next_id: Cell<u64>,
    pub pending: RefCell<HashMap<u64, PendingWrite>>,
}

impl Runtime {
    pub(super) fn new(
        hidden: bool,
        started: std::time::Instant,
    ) -> Result<Rc<Self>, Box<dyn std::error::Error>> {
        let controller = AppController::new(SystemClock);
        let initial = controller.shell_view();
        let target = Rc::new(RefCell::new(Weak::<Self>::new()));
        let shell = ShellHandle::new(initial.clone(), {
            let target = target.clone();
            move |action| match target.borrow().upgrade() {
                Some(runtime) => runtime.handle_action(&action),
                None => ShellUpdate {
                    view: initial.clone(),
                    route_changed: false,
                    issue: None,
                },
            }
        })?;
        let weak_window = shell.component().as_weak();
        let wake: Arc<dyn Fn() + Send + Sync> = Arc::new(move || {
            if weak_window
                .upgrade_in_event_loop(|window| window.invoke_desktop_wakeup())
                .is_err()
            {
                tracing::debug!("UI loop unavailable during settings wakeup");
            }
        });
        let worker = SettingsWorker::spawn(None, wake.clone())?;
        let (native_tx, native_rx) = bounded(256);
        install_event_handlers(native_tx.clone(), wake.clone());
        let theme_monitor = WindowsThemeMonitor::new(native_tx, wake)
            .map_err(|error| {
                tracing::warn!(
                    phase = "system_theme_monitor",
                    native_code = error.native_code(),
                    "theme listener unavailable, checking theme on window events instead"
                );
            })
            .ok();
        let runtime = Rc::new(Self {
            shell,
            ball: FloatingBall::new()?,
            menu: BallMenuWindow::new()?,
            controller: RefCell::new(controller),
            settings: RefCell::new(DesktopSettings::default()),
            theme: RefCell::new(ThemeState::default()),
            theme_monitor: RefCell::new(theme_monitor),
            worker,
            native: native_rx,
            tray: RefCell::new(None),
            hotkey: RefCell::new(None),
            entry: Cell::new(None),
            ready: Cell::new(false),
            initialized: Cell::new(false),
            hidden_start: hidden,
            started,
            presentation: Cell::new(Presentation::Hidden),
            busy: Cell::new(false),
            closing: Cell::new(false),
            finished: Cell::new(false),
            force_allowed: Cell::new(false),
            preference_repair: Cell::new(false),
            save_failed: Cell::new(false),
            geometry: Cell::new(None),
            restore: Cell::new(None),
            ball_position: RefCell::new(None),
            geometry_timer: Timer::default(),
            main_icon: Cell::new(0),
            main_caption: Cell::new(0),
            menu_open: Cell::new(false),
            drag: RefCell::new(None),
            next_id: Cell::new(1),
            pending: RefCell::new(HashMap::new()),
        });
        *target.borrow_mut() = Rc::downgrade(&runtime);
        runtime.connect();
        runtime.refresh_system_theme();
        Ok(runtime)
    }

    pub(super) fn wakeup(self: &Rc<Self>) {
        while let Some(reply) = self.worker.try_reply() {
            match reply {
                Reply::Loaded(result) => self.loaded(result),
                Reply::Saved { id, result } => self.saved(id, result),
            }
        }
        while let Ok(event) = self.native.try_recv() {
            if self.finished.get() {
                break;
            }
            self.native_action(event);
        }
        self.sync_minimized_timers();
    }

    pub(super) fn set_busy(&self, value: bool) {
        self.busy.set(value);
        self.shell.component().set_desktop_busy(value);
    }

    pub(super) fn sync_settings(&self) {
        let settings = self.settings.borrow();
        self.shell
            .component()
            .set_entry_mode(settings.entry_mode.as_str().into());
        self.shell
            .component()
            .set_close_behavior(settings.close_behavior.as_str().into());
        drop(settings);
        self.sync_theme();
    }

    pub(super) fn notice(&self, kind: ToastKind, message: impl Into<String>) {
        self.shell.show_toast(ToastRequest {
            kind,
            message: message.into(),
            action: None,
        });
    }

    pub(super) fn queue(
        &self,
        changes: Vec<DesktopSetting>,
        completion: Completion,
        checkpoint: bool,
    ) -> bool {
        let id = self.next_id.get();
        self.next_id.set(id.saturating_add(1));
        if self.worker.save(id, changes.clone(), checkpoint).is_err() {
            self.notice(ToastKind::Error, "保存队列不可用，请稍后重试。");
            return false;
        }
        self.pending.borrow_mut().insert(
            id,
            PendingWrite {
                changes,
                completion,
            },
        );
        true
    }
}

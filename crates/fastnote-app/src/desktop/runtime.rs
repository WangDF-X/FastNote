pub(super) mod session;

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use crossbeam_channel::{bounded, Receiver};
use fastnote_core::{
    BallPosition, DesktopSetting, DesktopSettings, EntryMode, ResolvedGeometry, WindowGeometry,
};
use fastnote_platform::{
    install_event_handlers, NativeEvent, NativeWindowHandle, WindowsHotkey, WindowsThemeMonitor,
    WindowsTray,
};
use fastnote_ui::{BallMenuWindow, FloatingBall, RetainedFeedback, ToastKind, ToastRequest};
use slint::Timer;

use crate::tasks::{TaskSession, UiEpoch};
use crate::{
    clock::{ClockTracker, SystemClock},
    controller::AppController,
};

use super::effects::{DesktopFeedback, PendingEffects, UiEffect};
use super::gesture::BallDrag;
use super::lease::UiLease;
use super::presentation::Presentation;
use super::theme::ThemeState;
use super::ui_session::{with_session, UiSession};
use super::wakeup::{make_wakeup, register};
use super::window_frame::WindowFrameState;
use super::worker::{DbWorker, Reply};

pub(super) enum Completion {
    Background,
    Preference { previous: DesktopSetting },
    Rollback,
    TaskPreference { reveal_unscheduled: bool },
    Close { exit: bool },
}

pub(super) struct PendingWrite {
    pub changes: Vec<DesktopSetting>,
    pub completion: Completion,
}

pub(super) struct Runtime {
    pub ui: RefCell<Option<UiSession>>,
    pub ui_epoch: Cell<UiEpoch>,
    pub ui_lease: RefCell<UiLease>,
    pub hot_lease_timer: Timer,
    pub retained_feedback: RefCell<Option<RetainedFeedback>>,
    pub main_native: Cell<Option<NativeWindowHandle>>,
    /// 实际 hide 成功即记录，入口显示/重模型清理的耗时不能额外延长 120 秒。
    pub ui_hidden_at: Cell<Option<u64>>,
    pub pending_effects: RefCell<PendingEffects>,
    pub desktop_feedback: RefCell<DesktopFeedback>,
    pub ball: FloatingBall,
    pub menu: BallMenuWindow,
    pub controller: RefCell<AppController<SystemClock>>,
    pub settings: RefCell<DesktopSettings>,
    pub task_preferences_error: RefCell<Option<String>>,
    pub theme: RefCell<ThemeState>,
    pub theme_monitor: RefCell<Option<WindowsThemeMonitor>>,
    pub worker: DbWorker,
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
    /// 原生外沿的最近配置；窗口重建时清空，不能只按 HWND 数值去重。
    pub main_frame: Cell<Option<WindowFrameState>>,
    pub menu_open: Cell<bool>,
    pub ball_native_pending: Cell<bool>,
    pub menu_native_pending: Cell<bool>,
    pub drag: RefCell<Option<BallDrag>>,
    pub next_id: Cell<u64>,
    pub pending: RefCell<HashMap<u64, PendingWrite>>,
    pub tasks: RefCell<TaskSession>,
    pub calendar: RefCell<super::calendar::CalendarSession>,
    /// 无会话导航及后续任务事件只失效，进入今日页时再查询。
    pub today_dirty: Cell<bool>,
    /// 当前详情在无会话时只记录失效，保存确认仍由内核处理。
    pub detail_dirty: Cell<bool>,
    /// Q-SHELL-10：复用既有日期检测，随内核常驻而非随页面停止。
    pub task_day_timer: Timer,
    /// 无 UI 时仍保留单个 UTC/本地日期观察，检测跨日、时区和时钟回拨。
    pub task_clock: RefCell<ClockTracker>,
    pub task_syncing: Cell<bool>,
    pub notes: RefCell<super::notes::NoteSession>,
    pub note_syncing: Cell<bool>,
    pub note_save_timer: Timer,
    pub journal: RefCell<super::journal::JournalSession>,
    pub journal_syncing: Cell<bool>,
    pub journal_save_timer: Timer,
}

impl Runtime {
    pub(super) fn new(
        hidden: bool,
        started: std::time::Instant,
    ) -> Result<Rc<Self>, Box<dyn std::error::Error>> {
        let controller = AppController::new(SystemClock);
        let wake = make_wakeup();
        let worker = DbWorker::spawn(None, wake.clone())?;
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
            ui: RefCell::new(None),
            ui_epoch: Cell::new(UiEpoch::default()),
            ui_lease: RefCell::new(UiLease::default()),
            hot_lease_timer: Timer::default(),
            retained_feedback: RefCell::new(None),
            main_native: Cell::new(None),
            ui_hidden_at: Cell::new(None),
            pending_effects: RefCell::new(PendingEffects::default()),
            desktop_feedback: RefCell::new(DesktopFeedback::default()),
            ball: FloatingBall::new()?,
            menu: BallMenuWindow::new()?,
            controller: RefCell::new(controller),
            settings: RefCell::new(DesktopSettings::default()),
            task_preferences_error: RefCell::new(None),
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
            main_frame: Cell::new(None),
            menu_open: Cell::new(false),
            ball_native_pending: Cell::new(false),
            menu_native_pending: Cell::new(false),
            drag: RefCell::new(None),
            next_id: Cell::new(1),
            pending: RefCell::new(HashMap::new()),
            tasks: RefCell::new(TaskSession::new()),
            calendar: RefCell::new(super::calendar::CalendarSession::default()),
            today_dirty: Cell::new(true),
            detail_dirty: Cell::new(true),
            task_day_timer: Timer::default(),
            task_clock: RefCell::new(ClockTracker::default()),
            task_syncing: Cell::new(false),
            notes: RefCell::new(super::notes::NoteSession::default()),
            note_syncing: Cell::new(false),
            note_save_timer: Timer::default(),
            journal: RefCell::new(super::journal::JournalSession::default()),
            journal_syncing: Cell::new(false),
            journal_save_timer: Timer::default(),
        });
        register(&runtime);
        runtime.connect_kernel();
        runtime.start_kernel_tasks();
        runtime.refresh_system_theme();
        if !hidden {
            runtime.ensure_ui()?;
        }
        tracing::info!(
            phase = "kernel_initialized",
            ui_present = runtime.with_ui(|_| ()).is_some(),
            "application kernel assembled"
        );
        Ok(runtime)
    }

    /// 临时访问主窗会话，不向内核泄漏组件所有权；缺失时不自动建窗。
    pub(super) fn with_ui<R>(&self, f: impl FnOnce(&UiSession) -> R) -> Option<R> {
        with_session(&self.ui, f)
    }

    pub(super) fn wakeup(self: &Rc<Self>) {
        self.check_ui_lease();
        while let Some(reply) = self.worker.try_reply() {
            match reply {
                Reply::Loaded(result) => self.loaded(result),
                Reply::Saved { id, result } => self.saved(id, result),
                Reply::Executed { id, result } => self.task_executed(id, result),
                Reply::Queried { id, result } => self.task_queried(id, result),
                Reply::NoteExecuted { id, result } => self.note_executed(id, result),
                Reply::NoteQueried { id, result } => self.note_queried(id, result),
                Reply::JournalExecuted { id, result } => self.journal_executed(id, result),
                Reply::JournalQueried { id, result } => self.journal_queried(id, result),
            }
        }
        while let Ok(event) = self.native.try_recv() {
            if self.finished.get() {
                break;
            }
            self.native_action(event);
        }
        self.sync_minimized_timers();
        self.check_ui_lease();
    }

    pub(super) fn set_busy(&self, value: bool) {
        self.busy.set(value);
        self.emit_ui(UiEffect::SetBusy(value));
        self.sync_task_preferences();
        self.sync_task_editor();
    }

    pub(super) fn sync_settings(&self) {
        let (entry_mode, close_behavior) = {
            let settings = self.settings.borrow();
            (settings.entry_mode, settings.close_behavior)
        };
        let _ = self.with_ui(|ui| {
            ui.shell
                .component()
                .set_entry_mode(entry_mode.as_str().into());
            ui.shell
                .component()
                .set_close_behavior(close_behavior.as_str().into());
            ui.shell
                .component()
                .set_week_start(self.settings.borrow().week_start.as_str().into());
        });
        self.sync_theme();
        self.sync_task_preferences();
        self.sync_task_editor();
    }

    pub(super) fn notice(&self, kind: ToastKind, message: impl Into<String>) {
        self.emit_ui(UiEffect::Toast(ToastRequest {
            kind,
            message: message.into(),
            action: None,
        }));
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

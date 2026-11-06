//! 最小诊断出口：只记录稳定诊断字段，后台写入，不把正文/任意 message 写入日志。

mod diagnostic;
mod writer;

use std::fmt;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::SystemTime;

use crossbeam_channel::{bounded, Sender};
use tracing::{
    field::{Field, Visit},
    span::{Attributes, Id, Record},
    Event, Metadata, Subscriber,
};

use diagnostic::LogError;
use writer::LogWriter;

pub(crate) use diagnostic::storage_error;

enum Command {
    Line(SystemTime, String),
    Stop,
}

pub(crate) struct LogGuard {
    sender: Sender<Command>,
    stopped: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

/// 诊断不可用不阻断窗口；数据库错误仍通过 UI 显示，控制台给出明确原因。
pub(crate) fn start() -> Option<LogGuard> {
    match start_inner() {
        Ok(guard) => Some(guard),
        Err(error) => {
            error.report();
            None
        }
    }
}

fn start_inner() -> Result<LogGuard, LogError> {
    let root = std::env::var_os("LOCALAPPDATA")
        .filter(|value| !value.is_empty())
        .map(|root| PathBuf::from(root).join("FastNote/logs"))
        .ok_or_else(|| {
            LogError::new(
                "resolve_log_path",
                None,
                std::io::Error::new(std::io::ErrorKind::InvalidInput, "LOCALAPPDATA unavailable"),
            )
        })?;
    let mut writer = LogWriter::new(root.clone(), 5 * 1024 * 1024)?;
    // 启动时实际打开日志文件，不能仅以目录存在判断文件日志可用。
    writer.prepare(SystemTime::now().into())?;
    let (sender, receiver) = bounded(256);
    let stopped = Arc::new(AtomicBool::new(false));
    let thread = thread::Builder::new()
        .name("fastnote-log".to_owned())
        .spawn(move || {
            while let Ok(command) = receiver.recv() {
                match command {
                    Command::Stop => break,
                    Command::Line(time, line) => {
                        if let Err(error) = writer.write(time.into(), &line) {
                            error.report();
                        }
                    }
                }
            }
        })
        .map_err(|error| LogError::new("start_log_thread", Some(&root), error))?;
    let guard = LogGuard {
        sender: sender.clone(),
        stopped: stopped.clone(),
        thread: Some(thread),
    };
    tracing::subscriber::set_global_default(Diagnostics {
        sender,
        stopped,
        next_span: AtomicU64::new(1),
    })
    .map_err(|_| {
        LogError::new(
            "register_log_subscriber",
            Some(&root),
            std::io::Error::new(
                std::io::ErrorKind::AlreadyExists,
                "tracing subscriber already set",
            ),
        )
    })?;
    Ok(guard)
}

impl Drop for LogGuard {
    fn drop(&mut self) {
        self.stopped.store(true, Ordering::Release);
        // 全局 Subscriber 持有 Sender，不能只靠 drop 断开；明确发送 Stop 后 join。
        let _ = self.sender.send(Command::Stop);
        if let Some(thread) = self.thread.take() {
            if thread.join().is_err() {
                eprintln!("FastNote：日志线程未正常结束。");
            }
        }
    }
}

struct Diagnostics {
    sender: Sender<Command>,
    stopped: Arc<AtomicBool>,
    next_span: AtomicU64,
}

impl Subscriber for Diagnostics {
    fn enabled(&self, metadata: &Metadata<'_>) -> bool {
        !self.stopped.load(Ordering::Acquire)
            && *metadata.level() <= tracing::Level::INFO
            && metadata.target().starts_with("fastnote_")
    }
    fn new_span(&self, _: &Attributes<'_>) -> Id {
        Id::from_u64(self.next_span.fetch_add(1, Ordering::Relaxed).max(1))
    }
    fn record(&self, _: &Id, _: &Record<'_>) {}
    fn record_follows_from(&self, _: &Id, _: &Id) {}
    fn enter(&self, _: &Id) {}
    fn exit(&self, _: &Id) {}
    fn event(&self, event: &Event<'_>) {
        if !self.enabled(event.metadata()) {
            return;
        }
        let mut fields = SafeFields::default();
        event.record(&mut fields);
        let line = format!(
            "{} {}{}",
            event.metadata().level(),
            event.metadata().target(),
            fields.0
        );
        let _ = self.sender.try_send(Command::Line(SystemTime::now(), line));
    }
}

#[derive(Default)]
struct SafeFields(String);

impl SafeFields {
    fn allowed(name: &str) -> bool {
        matches!(
            name,
            "error_code"
                | "log_id"
                | "elapsed_ms"
                | "elapsed_us"
                | "schema_version"
                | "setting_key"
                | "entry_mode"
                | "error_kind"
                | "io_kind"
                | "native_code"
                | "phase"
                | "request_id"
                | "query_scope"
                | "ui_present"
                | "session_generation"
                | "page_generation"
                | "lease_generation"
                | "deadline_tick_ms"
                | "lifecycle_state"
                | "release_reason"
                | "component_released"
                | "old_hwnd_present"
                | "working_set_bytes"
                | "private_bytes"
                | "handle_count"
                | "gdi_objects"
                | "user_objects"
                | "system_load_percent"
                | "over_budget"
                | "loaded_rows"
        )
    }
    fn add(&mut self, name: &str, value: &str) {
        if !Self::allowed(name) {
            return;
        }
        self.0.push_str(&format!(" {name}="));
        self.0.extend(
            value
                .chars()
                .filter(|character| !character.is_control())
                .take(256),
        );
    }
}

impl Visit for SafeFields {
    fn record_str(&mut self, field: &Field, value: &str) {
        self.add(field.name(), value);
    }
    fn record_debug(&mut self, field: &Field, value: &dyn fmt::Debug) {
        if Self::allowed(field.name()) {
            self.add(field.name(), &format!("{value:?}"));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_diagnostics_whitelist_never_records_body_or_arbitrary_messages() {
        let mut fields = SafeFields::default();
        fields.add("message", "假正文，不能记录");
        fields.add("task_title", "假标题，不能记录");
        fields.add("path", "假路径，仅终端可显示");
        fields.add("log_id", "fake-id\n");
        fields.add("error_code", "E_STORAGE_IO");
        assert!(!fields.0.contains("假"));
        assert!(!fields.0.contains('\n'));
        assert!(fields.0.contains("fake-id"));
        assert!(fields.0.contains("E_STORAGE_IO"));
    }

    #[test]
    fn test_lifecycle_diagnostics_keep_only_request_scope_and_construction_metadata() {
        let mut fields = SafeFields::default();
        fields.add("request_id", "7");
        fields.add("query_scope", "today");
        fields.add("ui_present", "false");
        fields.add("task_description", "假正文，不能记录");
        fields.add("page_title", "假标题，不能记录");
        assert!(fields.0.contains("request_id=7"));
        assert!(fields.0.contains("query_scope=today"));
        assert!(fields.0.contains("ui_present=false"));
        assert!(!fields.0.contains("假"));
    }
}

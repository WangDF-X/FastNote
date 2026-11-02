use std::io::{self, Write};
use std::path::{Path, PathBuf};

use fastnote_store::StoreError;
use thiserror::Error;

/// 日志自身的失败也携带阶段、路径和关联 ID，不输出原始错误字符串。
#[derive(Debug, Error)]
pub(super) enum LogError {
    #[error("Log operation {phase} failed ({source})")]
    Operation {
        phase: &'static str,
        path: Option<PathBuf>,
        #[source]
        source: Box<StoreError>,
    },
}

impl LogError {
    /// 附加安全的失败上下文，复用存储错误的分类与日志 ID。
    pub(super) fn new(phase: &'static str, path: Option<&Path>, source: io::Error) -> Self {
        Self::Operation {
            phase,
            path: path.map(Path::to_owned),
            source: Box::new(StoreError::from(source)),
        }
    }

    /// 直接输出到终端，不依赖尚未初始化或已经失败的文件日志。
    pub(super) fn report(&self) {
        let Self::Operation {
            phase,
            path,
            source,
        } = self;
        // 诊断出口失败不能再次依赖 tracing 或阻断应用。
        let _ = write_terminal(
            &mut io::stderr().lock(),
            "诊断日志不可用",
            path.as_deref(),
            phase,
            source,
        );
    }
}

/// 设置错误始终送到终端；文件日志未初始化时也保留与 UI 相同的日志 ID。
pub(crate) fn storage_error(path: Option<&Path>, phase: &'static str, error: &StoreError) {
    let _ = write_terminal(
        &mut io::stderr().lock(),
        "设置存储操作失败",
        path,
        phase,
        error,
    );
    // 路径只用于本机终端诊断，不扩大持久日志的字段白名单。
    tracing::error!(
        phase,
        error_code = error.code().as_str(),
        log_id = error.log_id(),
        native_code = error.native_code(),
        error_kind = error.diagnostic_kind(),
        io_kind = io_kind(error),
        "settings storage failed"
    );
}

fn io_kind(error: &StoreError) -> String {
    match error {
        StoreError::File { source, .. } => format!("{:?}", source.kind()),
        _ => "n/a".to_owned(),
    }
}

fn write_terminal(
    output: &mut impl Write,
    subject: &'static str,
    path: Option<&Path>,
    phase: &'static str,
    error: &StoreError,
) -> io::Result<()> {
    let path = path.map_or_else(
        || "unavailable".to_owned(),
        |path| format!("{:?}", path.to_string_lossy()),
    );
    let line = format!(
        "FastNote：{subject}。path={path} phase={phase} error_code={} error_kind={} io_kind={} native_code={} log_id={:?}\n",
        error.code().as_str(),
        error.diagnostic_kind(),
        io_kind(error),
        error.native_code(),
        error.log_id(),
    );
    output.write_all(line.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(path: Option<&Path>, phase: &'static str, error: &StoreError) -> String {
        let mut output = Vec::new();
        write_terminal(&mut output, "测试诊断", path, phase, error).unwrap();
        String::from_utf8(output).unwrap()
    }

    #[test]
    fn test_terminal_diagnostic_keeps_path_phase_native_code_and_ui_log_id() {
        let error = StoreError::from(io::Error::from_raw_os_error(5));
        let output = line(
            Some(Path::new(r"C:\fake-app-data\FastNote\fastnote.db")),
            "open_database",
            &error,
        );
        assert!(output.contains("fake-app-data"));
        assert!(output.contains("fastnote.db"));
        assert!(output.contains("phase=open_database"));
        assert!(output.contains("error_kind=file"));
        assert!(output.contains("io_kind=PermissionDenied"));
        assert!(output.contains("native_code=5"));
        assert!(output.contains(error.log_id()));
    }

    #[test]
    fn test_terminal_diagnostic_never_prints_io_or_sqlite_source_messages() {
        let errors = [
            StoreError::from(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "假正文，不能出现在终端",
            )),
            StoreError::from(rusqlite::Error::SqliteFailure(
                rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_CANTOPEN),
                Some("假标题、SQL 和绑定值，不能输出".to_owned()),
            )),
        ];
        for error in errors {
            let output = line(None, "open_database", &error);
            assert!(output.contains("path=unavailable"));
            assert!(output.contains(error.log_id()));
            assert!(!output.contains("假"));
            assert!(!output.contains("绑定值"));
        }
    }

    #[test]
    fn test_terminal_diagnostic_escapes_control_characters_in_path_and_log_id() {
        let error = StoreError::Integrity {
            log_id: "fake-id\n\u{1b}[31m".to_owned(),
        };
        let output = line(
            Some(Path::new("fake\n\u{1b}[31m\tdirectory")),
            "load_settings",
            &error,
        );
        assert_eq!(output.lines().count(), 1);
        assert!(!output.trim_end().chars().any(char::is_control));
    }

    #[test]
    fn test_log_failure_retains_context_and_source_without_tracing() {
        let failure = LogError::new(
            "create_log_directory",
            Some(Path::new("fake-logs")),
            io::Error::from_raw_os_error(5),
        );
        let LogError::Operation {
            phase,
            path,
            source,
        } = failure;
        let output = line(path.as_deref(), phase, &source);
        assert!(output.contains("path=\"fake-logs\""));
        assert!(output.contains("phase=create_log_directory"));
        assert!(output.contains("native_code=5"));
        assert!(output.contains(source.log_id()));
    }
}

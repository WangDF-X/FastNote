use global_hotkey::{hotkey::HotKey, GlobalHotKeyManager};

use super::PlatformError;

/// 全局唤起热键；在运行 Win32 消息循环的 UI 线程创建和销毁。
pub struct WindowsHotkey {
    manager: GlobalHotKeyManager,
    hotkey: Option<HotKey>,
}

impl WindowsHotkey {
    /// 注册一个非空唤起组合；失败不会影响其它入口。
    pub fn register(value: &str) -> Result<Self, PlatformError> {
        let hotkey = value
            .parse::<HotKey>()
            .map_err(|_| PlatformError::InvalidHotkey)?;
        let manager = GlobalHotKeyManager::new()?;
        manager.register(hotkey)?;
        Ok(Self {
            manager,
            hotkey: Some(hotkey),
        })
    }

    /// 注册成功的 ID，用于过滤本应用之外的事件。
    pub fn id(&self) -> Option<u32> {
        self.hotkey.map(|hotkey| hotkey.id())
    }

    /// 显式注销；只有成功后清除句柄，失败时 Drop 再尝试。
    pub fn unregister(&mut self) -> Result<(), PlatformError> {
        if let Some(hotkey) = self.hotkey {
            self.manager.unregister(hotkey)?;
            self.hotkey = None;
        }
        Ok(())
    }
}

impl Drop for WindowsHotkey {
    fn drop(&mut self) {
        if self.unregister().is_err() {
            tracing::warn!("global hotkey unregister failed");
        }
    }
}

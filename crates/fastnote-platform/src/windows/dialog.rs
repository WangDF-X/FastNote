//! Windows 原生图片选择器；只返回用户确认的本地路径，不读取文件内容。

use std::path::PathBuf;

use windows::core::w;
use windows::Win32::Foundation::RPC_E_CHANGED_MODE;
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize, CLSCTX_INPROC_SERVER,
    COINIT_APARTMENTTHREADED,
};
use windows::Win32::UI::Shell::Common::COMDLG_FILTERSPEC;
use windows::Win32::UI::Shell::{
    FileOpenDialog, IFileOpenDialog, FOS_FILEMUSTEXIST, FOS_PATHMUSTEXIST, SIGDN_FILESYSPATH,
};

use super::NativeWindowHandle;

/// 打开系统文件选择器，只允许 PNG/JPEG；取消或失败返回 None。
pub fn pick_image_file(owner: Option<NativeWindowHandle>) -> Option<PathBuf> {
    // 对话框要求 STA；UI 线程可能已初始化 COM，成功返回（含 S_FALSE）才配对 Uninitialize。
    let initialized = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
    if initialized == RPC_E_CHANGED_MODE {
        // 当前线程已处于其他 apartment：在独立 STA 线程打开对话框，避免改变主线程 COM 状态。
        let handle = std::thread::spawn(move || {
            let hr = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
            if hr.is_err() {
                return None;
            }
            let result = pick(owner);
            unsafe { CoUninitialize() };
            result
        });
        return handle.join().ok().flatten();
    }
    if initialized.is_err() {
        return None;
    }
    let result = pick(owner);
    unsafe { CoUninitialize() };
    result
}

fn pick(owner: Option<NativeWindowHandle>) -> Option<PathBuf> {
    let dialog: IFileOpenDialog =
        unsafe { CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER) }.ok()?;
    let filters = [
        COMDLG_FILTERSPEC {
            pszName: w!("图片 (*.png;*.jpg;*.jpeg)"),
            pszSpec: w!("*.png;*.jpg;*.jpeg"),
        },
        COMDLG_FILTERSPEC {
            pszName: w!("PNG (*.png)"),
            pszSpec: w!("*.png"),
        },
        COMDLG_FILTERSPEC {
            pszName: w!("JPEG (*.jpg;*.jpeg)"),
            pszSpec: w!("*.jpg;*.jpeg"),
        },
    ];
    unsafe {
        let options = dialog.GetOptions().ok()?;
        dialog
            .SetOptions(options | FOS_FILEMUSTEXIST | FOS_PATHMUSTEXIST)
            .ok()?;
        dialog.SetFileTypes(&filters).ok()?;
        // 用户取消时返回错误码 0x800704C7；与真正的失败一样按 None 处理，不额外提示。
        if dialog.Show(owner.map(|handle| handle.hwnd())).is_err() {
            return None;
        }
        let item = dialog.GetResult().ok()?;
        let name = item.GetDisplayName(SIGDN_FILESYSPATH).ok()?;
        let path = name.to_string().ok()?;
        CoTaskMemFree(Some(name.0 as *const _));
        Some(PathBuf::from(path))
    }
}

use std::mem::size_of;

use ::windows::core::Error;
use ::windows::Win32::Foundation::{SetLastError, ERROR_SUCCESS, HWND};
use ::windows::Win32::System::ProcessStatus::{
    K32GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS_EX,
};
use ::windows::Win32::System::SystemInformation::{
    GetTickCount64, GlobalMemoryStatusEx, MEMORYSTATUSEX,
};
use ::windows::Win32::System::Threading::{
    GetCurrentProcess, GetCurrentProcessId, GetGuiResources, GetProcessHandleCount,
    GET_GUI_RESOURCES_FLAGS, GR_GDIOBJECTS, GR_USEROBJECTS,
};
use ::windows::Win32::UI::WindowsAndMessaging::GetWindowThreadProcessId;

use super::{NativeWindowHandle, PlatformError};

/// Windows 启动以来的单调毫秒数，包含睡眠；不会受本地时间/时区调整影响。
pub fn uptime_milliseconds() -> u64 {
    // SAFETY: 无参数、无指针、无句柄所有权的只读系统时钟调用。
    unsafe { GetTickCount64() }
}

/// 进程本身的只读资源样本。工作集与专用提交分别计量，均为原始字节。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProcessResources {
    /// Windows 总 Working Set（并非专用工作集或 GPU 内存）。
    pub working_set_bytes: u64,
    /// Private Bytes，包含已换出的私有提交。
    pub private_bytes: u64,
    /// 当前进程的内核句柄数。
    pub handles: u32,
    /// GDI 对象数；不能用它代替 GPU/渲染资源计量。
    pub gdi_objects: u32,
    /// USER 对象数，包含当前进程的窗口及其它 USER 对象。
    pub user_objects: u32,
    /// Windows 报告的系统物理内存负载百分比。
    pub system_load_percent: u32,
    /// 当前系统可用物理内存字节数。
    pub available_physical_bytes: u64,
    /// 当前系统总物理内存字节数。
    pub total_physical_bytes: u64,
}

/// 在调用线程读取当前进程/系统计数，不启动线程、不申请内存压力或清空工作集。
pub fn process_resources() -> Result<ProcessResources, PlatformError> {
    let mut memory = PROCESS_MEMORY_COUNTERS_EX {
        cb: size_of::<PROCESS_MEMORY_COUNTERS_EX>() as u32,
        ..Default::default()
    };
    let mut system = MEMORYSTATUSEX {
        dwLength: size_of::<MEMORYSTATUSEX>() as u32,
        ..Default::default()
    };
    let mut handles = 0;
    // SAFETY: 当前进程伪句柄只借用，不能 CloseHandle；两个缓冲区大小/布局精确对应
    // Windows API，指针独占且在同步调用期间有效，API 不保留任何栈指针。
    unsafe {
        let process = GetCurrentProcess();
        if !K32GetProcessMemoryInfo(process, std::ptr::from_mut(&mut memory).cast(), memory.cb)
            .as_bool()
        {
            return Err(Error::from_thread().into());
        }
        GlobalMemoryStatusEx(&mut system)?;
        GetProcessHandleCount(process, &mut handles)?;
        Ok(ProcessResources {
            working_set_bytes: memory.WorkingSetSize as u64,
            private_bytes: memory.PrivateUsage as u64,
            handles,
            gdi_objects: gui_objects(GR_GDIOBJECTS)?,
            user_objects: gui_objects(GR_USEROBJECTS)?,
            system_load_percent: system.dwMemoryLoad,
            available_physical_bytes: system.ullAvailPhys,
            total_physical_bytes: system.ullTotalPhys,
        })
    }
}

fn gui_objects(flag: GET_GUI_RESOURCES_FLAGS) -> Result<u32, PlatformError> {
    // SAFETY: 只读取当前进程；清零 LastError 以区分合法的零对象与读取失败。
    unsafe {
        SetLastError(ERROR_SUCCESS);
        let count = GetGuiResources(GetCurrentProcess(), flag);
        let error = ::windows::Win32::Foundation::GetLastError();
        if count == 0 && error != ERROR_SUCCESS {
            Err(Error::from_thread().into())
        } else {
            Ok(count)
        }
    }
}

/// 只读检查旧 HWND 是否仍属于指定进程；不发送消息、不操作或销毁窗口。
pub fn window_belongs_to_process(handle: NativeWindowHandle) -> bool {
    let mut process = 0;
    // SAFETY: HWND 按不透明值传入，失效句柄由 Windows 返回零；独占输出指针只在
    // 同步调用内使用，不解引用窗口内存。PID 用于排除其它进程复用 HWND 的情况。
    unsafe {
        GetWindowThreadProcessId(HWND(handle.0 as *mut _), Some(&mut process)) != 0
            && process == GetCurrentProcessId()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_native_sample_reads_current_process_only_and_exposes_separate_byte_counters() {
        let before = uptime_milliseconds();
        let sample = process_resources().unwrap();
        assert!(uptime_milliseconds() >= before);
        assert!(sample.working_set_bytes > 0);
        assert!(sample.private_bytes > 0);
        assert!(sample.total_physical_bytes > 0);
        assert!(sample.available_physical_bytes <= sample.total_physical_bytes);
        assert!(sample.system_load_percent <= 100);
    }

    #[test]
    fn test_invalid_native_window_probe_does_not_create_or_operate_a_window() {
        assert!(!window_belongs_to_process(NativeWindowHandle(0)));
    }
}

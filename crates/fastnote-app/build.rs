//! 把品牌图标编译进 Windows exe 资源。
//!
//! 资源管理器、任务栏、Alt+Tab 和标题栏在窗口没有显式图标时都会回退到
//! exe 的第一个图标资源，所以这里只需要把 `ui/branding/fastnote.ico`
//! 编成资源并交给链接器。图标本身不在这里生成——唯一来源是
//! `ui/branding/` 下的原稿，见该目录的 README。
//!
//! 不引入新依赖：直接调用 Windows SDK 自带的 `rc.exe`。找不到时构建会
//! 失败并提示用 `RC` 环境变量指定路径。

use std::path::{Component, Path, PathBuf};
use std::process::Command;

fn main() {
    // 图标是仓库里的静态资源，不是生成的中间产物。
    println!("cargo:rerun-if-changed=../../ui/branding/fastnote.ico");
    println!("cargo:rerun-if-env-changed=RC");

    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    // MinGW 的链接器不认 rc.exe 产出的 .res，这里只覆盖 MSVC。
    if std::env::var("CARGO_CFG_TARGET_ENV").as_deref() != Ok("msvc") {
        println!("cargo:warning=exe icon resource is only wired for the MSVC toolchain");
        return;
    }

    let out_dir = PathBuf::from(std::env::var("OUT_DIR").expect("cargo sets OUT_DIR"));
    let script = out_dir.join("app-icon.rc");
    let resource = out_dir.join("app-icon.res");

    std::fs::write(&script, resource_script()).expect("write app-icon.rc");
    compile_resource(&script, &resource);

    // 只作用于可执行文件；测试/示例不需要带图标资源。
    println!("cargo:rustc-link-arg-bins={}", resource.display());
}

/// 生成资源脚本。路径用绝对路径，避免 rc.exe 的当前目录与 Cargo 不一致。
fn resource_script() -> String {
    let icon = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../ui/branding/fastnote.ico");
    let icon = normalize(&icon);
    assert!(
        icon.is_file(),
        "brand icon not found: {} (run ui/branding/tools/build_icons.py)",
        icon.display()
    );
    let escaped = icon.display().to_string().replace('\\', "\\\\");
    format!("1 ICON \"{escaped}\"\n")
}

/// 解析 `.` 和 `..`，但不访问文件系统，避免 canonicalize 的 `\\\\?\\` 前缀。
fn normalize(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::ParentDir => {
                normalized.pop();
            }
            Component::CurDir => {}
            other => normalized.push(other.as_os_str()),
        }
    }
    normalized
}

fn compile_resource(script: &Path, resource: &Path) {
    let compiler = find_resource_compiler();
    let output = Command::new(&compiler)
        .arg("/nologo")
        .arg(format!("/fo{}", resource.display()))
        .arg(script)
        .output()
        .unwrap_or_else(|error| panic!("failed to run {}: {error}", compiler.display()));
    if !output.status.success() {
        panic!(
            "{} failed ({}):\n{}{}",
            compiler.display(),
            output.status,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

fn find_resource_compiler() -> PathBuf {
    if let Some(path) = std::env::var_os("RC").map(PathBuf::from) {
        assert!(
            path.is_file(),
            "RC={} does not point to an existing rc.exe",
            path.display()
        );
        return path;
    }
    if let Some(found) = find_on_path() {
        return found;
    }
    if let Some(found) = find_in_windows_kits() {
        return found;
    }
    panic!(
        "rc.exe not found; install the Windows SDK or set the RC environment variable \
         to the full path of rc.exe"
    );
}

fn find_on_path() -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|directory| directory.join("rc.exe"))
        .find(|candidate| candidate.is_file())
}

fn find_in_windows_kits() -> Option<PathBuf> {
    let base =
        std::env::var_os("ProgramFiles(x86)").or_else(|| std::env::var_os("ProgramFiles"))?;
    let bin = Path::new(&base).join("Windows Kits").join("10").join("bin");

    let host = match std::env::consts::ARCH {
        "x86_64" => "x64",
        "aarch64" => "arm64",
        "x86" => "x86",
        _ => return None,
    };

    let mut versions: Vec<PathBuf> = std::fs::read_dir(&bin)
        .ok()?
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .collect();
    // 版本目录按 SDK 约定是 10.0.NNNNN.0，定长数字段，字符串倒序即最新。
    versions.sort();

    for version in versions.iter().rev() {
        let candidate = version.join(host).join("rc.exe");
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

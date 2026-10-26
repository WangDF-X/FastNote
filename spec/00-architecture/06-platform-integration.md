# ARCH-06 平台集成

> 文档编号：ARCH-06 ｜ 模块：00 总览与架构 ｜ 状态：已定稿 v0.1 ｜ 更新：2026-10-23
> 上游：[ARCH-01](01-tech-and-runtime.md)、[product.md](../../product.md) §1 ｜ 下游：[02-desktop-entry](../02-desktop-entry/)、[08-settings](../08-settings/)

---

## 1. 目的与范围

定义 FastNote 在 Windows、macOS、Linux 上的系统集成方式、能力差异、降级策略与打包要求。产品行为以本文为准；具体入口交互见 [02-desktop-entry](../02-desktop-entry/)。

**范围内**：悬浮球窗口、系统托盘、全局快捷键、开机启动、单实例、主题检测、文件对话框、窗口/多屏/DPI、打包签名。
**范围外**：业务功能、UI 视觉（ARCH-05）、自动更新（第一版无网络）。

---

## 2. 平台支持基线

| 平台 | 最低版本 | 支持级别 | 说明 |
| --- | --- | --- | --- |
| Windows | Windows 10 1809+（x64 / arm64） | 完整 | 主要开发与验证平台。 |
| macOS | 12 Monterey+（Universal） | 完整 | 无 Dock 图标，菜单栏/悬浮球常驻。 |
| Linux X11 | Ubuntu 22.04+ / 主流发行版 | 完整 | 托盘依赖 AppIndicator。 |
| Linux Wayland | 同上 | 尽力支持 | 悬浮球定位/置顶与全局快捷键受限，见 §5。 |

> **设计决策 D-21：** 三平台功能一致优先于交互完全一致。系统级入口形态允许遵循平台习惯（如 macOS 菜单栏），但业务能力、数据与设置项必须一致。

---

## 3. 能力抽象

### 3.1 平台门面

```rust
pub trait PlatformFacade: Send + Sync {
    fn capabilities(&self) -> PlatformCapabilities;
    fn entry(&self) -> &dyn EntryPlatform;       // 悬浮球 / 托盘
    fn hotkeys(&self) -> &dyn HotkeyPlatform;    // 全局快捷键
    fn autostart(&self) -> &dyn AutostartPlatform;
    fn theme(&self) -> &dyn ThemePlatform;
    fn shell(&self) -> &dyn ShellPlatform;       // 打开目录、文件对话框、剪贴板
}

pub struct PlatformCapabilities {
    pub os: PlatformOs,                  // Windows / MacOs / LinuxX11 / LinuxWayland
    pub floating_ball: bool,             // 可创建透明置顶无边框窗口并定位
    pub tray: bool,                      // 系统托盘/状态栏可用
    pub global_hotkey: bool,             // 可注册全局快捷键
    pub autostart: bool,
    pub theme_detection: bool,
    pub transparent_window: bool,
    pub always_on_top: bool,
    pub skip_taskbar: bool,
    pub native_file_dialog: bool,
    pub tray_reason: Option<String>,     // 不可用原因（用于设置页说明）
}
```

- 启动时探测一次，能力变化（如 Linux 托盘组件后装）需重启生效。
- UI 对不支持的能力：**显示为禁用 + 原因说明**，而不是隐藏，避免用户困惑。

### 3.2 入口接口

```rust
pub trait EntryPlatform: Send + Sync {
    fn show_ball(&self, opts: BallOptions) -> Result<()>;
    fn hide_ball(&self) -> Result<()>;
    fn update_ball(&self, opts: BallOptions) -> Result<()>; // 尺寸/透明度/位置
    fn show_tray(&self, menu: EntryMenu) -> Result<()>;
    fn hide_tray(&self) -> Result<()>;
    fn update_tray_menu(&self, menu: EntryMenu) -> Result<()>;
    fn set_tray_tooltip(&self, text: &str) -> Result<()>;
    fn is_ball_visible(&self) -> bool;
    fn is_tray_visible(&self) -> bool;
}
```

- 悬浮球与托盘**互斥显示**（由 `general.entry_mode` 决定），切换时先销毁旧入口再创建新入口。
- 入口只负责唤起与菜单，不承载任务编辑（product §1.3）。

---

## 4. 平台差异矩阵

| 能力 | Windows | macOS | Linux X11 | Linux Wayland |
| --- | --- | --- | --- | --- |
| 悬浮球：透明/无边框 | 是（DWM 合成） | 是（NSWindow 透明） | 是（需合成器） | 视合成器 |
| 悬浮球：置顶 | 是（`HWND_TOPMOST`） | 是（`NSFloatingWindowLevel`） | 是（`_NET_WM_STATE_ABOVE`） | 通常不支持强制 |
| 悬浮球：自由定位 | 是 | 是 | 是 | 否（合成器决定位置） |
| 悬浮球：不占任务栏 | 是（tool window） | 是（`LSUIElement`） | 是（skip taskbar） | 视合成器 |
| 系统托盘 | 是（通知区域） | 是（NSStatusItem） | 是（SNI/AppIndicator；GNOME 需扩展） | 是（SNI） |
| 托盘左键打开主界面 | 是 | 是（自定义 action） | 是 | 是 |
| 全局快捷键 | 是（`RegisterHotKey`） | 是（Carbon 热键） | 是（`XGrabKey`） | **否**（第一版不实现 Portal） |
| 开机启动 | 是（HKCU Run） | 是（SMAppService/LaunchAgent） | 是（`.desktop` autostart） | 是 |
| 跟随系统主题 | 是（注册表） | 是（`NSAppearance`） | 是（Portal/GTK） | 是（Portal） |
| 单实例 | 是（命名互斥 + 本地 socket） | 是（本地 socket） | 是（本地 socket） | 是 |
| 原生文件对话框 | 是 | 是 | Portal（回退 GTK） | Portal |
| 多屏/混合 DPI | 是（Per-Monitor V2） | 是 | 是 | 部分 |

### 4.1 Windows

- **悬浮球**：独立顶层窗口，`WS_EX_TOOLWINDOW | WS_EX_TOPMOST`，透明背景；拖动通过捕获指针 + `SetWindowPos`；不进入 Alt+Tab。
- **托盘**：`Shell_NotifyIcon`；左键单击打开主界面，右键弹出原生菜单；任务栏重启（Explorer 崩溃）后需自动重新添加图标。
- **全局快捷键**：`RegisterHotKey`；`Win+*` 组合可能被系统占用；注册失败按 §6 处理。
- **开机启动**：`HKCU\Software\Microsoft\Windows\CurrentVersion\Run`，值为 `"<install>\fastnote.exe" --hidden`，不请求管理员权限。
- **主题**：读取 `HKCU\...\Themes\Personalize\AppsUseLightTheme`；监听 `WM_SETTINGCHANGE`。
- **DPI**：Per-Monitor V2；窗口位置按逻辑坐标存储，恢复时按目标显示器缩放换算。

### 4.2 macOS

- **悬浮球**：`NSPanel`（nonactivating、floating level、`canJoinAllSpaces`），无标题栏；不抢焦点。
- **菜单栏**：`NSStatusItem`；**左键单击打开主界面**，右键/Control-点击弹出菜单（自定义 action，而非 `setMenu:`，以满足产品「点击打开主界面」）。
- **全局快捷键**：Carbon `RegisterEventHotKey`，无需辅助功能权限（需 Spike 验证）。
- **开机启动**：macOS 13+ 使用 `SMAppService.mainApp`；12 使用 `LaunchAgent` plist，参数 `--hidden`。
- **Dock**：`LSUIElement=1`，无 Dock 图标；应用窗口激活时提供最小应用菜单（含退出 `⌘Q`）。
- **主题**：`NSApp.effectiveAppearance`，监听 `AppleInterfaceThemeChangedNotification`。
- **签名**：Developer ID + 公证（Notarization）+ Hardened Runtime；未签名包首次启动会被 Gatekeeper 阻止。

### 4.3 Linux（X11）

- **悬浮球**：无边框工具窗口 + `_NET_WM_WINDOW_TYPE_UTILITY` + `_NET_WM_STATE_ABOVE` + skip taskbar/pager；拖动使用 `XMoveWindow` 或窗口管理器移动。
- **托盘**：StatusNotifierItem（通过 `tray-icon` + libayatana-appindicator）；GNOME 默认不显示，需 AppIndicator 扩展；检测失败时提示原因。
- **全局快捷键**：X11 `XGrabKey`；与窗口管理器快捷键冲突时注册失败。
- **开机启动**：写入 `~/.config/autostart/fastnote.desktop`，`Exec=<path> --hidden`。
- **主题**：优先 `org.freedesktop.portal.Settings` 的 `color-scheme`；回退 GTK `gtk-theme` 名称判断；再回退浅色。
- **文件对话框**：优先 XDG Desktop Portal（`rfd` 的 portal 后端）；无 Portal 时回退 GTK 后端；两者都不可用时允许手动输入路径（仅备份/导出的保存路径）。

### 4.4 Linux（Wayland）

| 限制 | 影响 | 处理 |
| --- | --- | --- |
| 客户端不能设置窗口绝对位置 | 悬浮球无法固定在指定位置，可能被合成器放置 | 首次启动检测 Wayland：允许创建悬浮球，但提示「位置由桌面环境管理」；建议切换状态栏模式 |
| 置顶/跳任务栏不保证 | 球可能被遮挡或出现在任务栏 | 能力探测为 false 时隐藏「置顶」相关行为说明 |
| 无标准全局快捷键 | 三个全局快捷键不可用 | 设置页禁用并说明；保留应用内快捷键、托盘菜单、悬浮球入口 |
| 托盘依赖 SNI | GNOME 需扩展 | 同 X11；不可用时回退悬浮球或主窗口 |
| 截图/无障碍 API 受限 | 无影响（第一版不使用） | — |

> **设计决策 D-22：** 第一版不在 Wayland 上实现 XDG Portal GlobalShortcuts。原因：需要用户逐次授权、各合成器支持不一、无法保证「开箱即用」；与其半可用，不如明确禁用并给出替代入口。若后续用户需求强烈，可在第二版接入。

---

## 5. 入口窗口实现要点

### 5.1 窗口形态

| 属性 | 值 |
| --- | --- |
| 尺寸 | 逻辑 `ball_size + 16`（含 8px 阴影留白），默认 80×80 |
| 背景 | 透明；中心绘制圆形球体（48–96 可调） |
| 边框 | 1px 半透明白/黑描边，保证深浅色背景可见 |
| 置顶 | 是（能力允许时） |
| 任务栏/Dock | 不显示 |
| 焦点 | 不抢焦点（点击前不激活）；点击后唤起主界面 |
| 位置 | 默认主屏右下（距右 24、距任务栏上沿 48）；记忆上次位置 |
| 透明度 | 默认 90%，闲置 3s 后应用；hover 恢复 100%；最小 30% |
| 阴影 | 使用 8px 透明边距 + 柔和阴影，避免窗口阴影 API 的平台差异 |
| 动画 | 仅 hover/press 的 120ms 缩放（1.0→1.05），无持续动画 |

### 5.2 拖动与定位

1. 指针按下记录起点与窗口位置。
2. 移动超过 4px 判定为拖动（否则为点击）。
3. 按物理像素移动窗口；换算使用当前显示器缩放因子。
4. 释放时吸附到工作区边缘（距边缘 ≤ 16px 时贴边），保存 `{x, y, monitor}`。
5. 恢复时校验目标显示器是否存在、位置是否在工作区内；否则回到默认位置。
6. 拖动过程中不更新设置，释放后写入一次，避免高频写库。

### 5.3 点击行为

| 操作 | 行为 |
| --- | --- |
| 左键单击（非拖动） | 打开/聚焦主界面；主界面未打开时进入今日任务；已打开则保持当前页面 |
| 右键单击 | 显示快捷菜单（今日任务、日历、小记、日记、计划板、设置、分隔线、退出） |
| 双击 | 等同两次单击（第二次为聚焦，无额外动作） |
| 中键 | 无操作 |
| 悬停 | 提升不透明度、显示 tooltip「FastNote」 |

- 菜单使用 Slint 自绘浮层（避免透明窗口上原生菜单定位问题），需支持键盘导航与点击外部关闭。
- 菜单位置靠近球体，超出屏幕时自动翻转。

### 5.4 托盘实现要点

- Windows：16/20/24px 多尺寸 `.ico`；macOS：18×18 Template Image（自动适配深浅色）；Linux：22/24px PNG + SVG。
- 左键单击打开主界面；右键弹出原生菜单（`muda`）。
- 菜单内容固定顺序：

```text
打开主界面
──────────
今日任务 / 日历 / 小记 / 日记 / 计划板
──────────
设置
切换到悬浮球（或“切换到状态栏”）
开机启动   ✓
──────────
退出 FastNote
```

- 菜单打开时不阻塞主线程；点击项通过事件投递到 UI 线程。
- Windows Explorer 重启后重新注册托盘图标（监听 `TaskbarCreated` 消息）。
- 托盘 tooltip 固定为 `FastNote`（不动态显示任务数，避免频繁更新与平台限制）。

### 5.5 入口不可用时的降级

| 情况 | 降级 |
| --- | --- |
| 托盘不可用（Linux 无 SNI） | 保持悬浮球；设置页说明；若同时球不可用，见下。 |
| 悬浮球不可用（Wayland 定位/透明失败） | 已选球模式时提示并临时使用托盘；用户可确认切换。 |
| 两者都不可用 | 保持主窗口；关闭行为强制为「退出应用」并提示，避免应用隐身。 |
| 全局快捷键注册失败 | 应用正常运行；设置页该项显示错误；Toast 提示一次；可用托盘/球/应用内快捷键。 |

---

## 6. 全局快捷键平台细节

| 平台 | 机制 | 权限 | 已知冲突 |
| --- | --- | --- | --- |
| Windows | `RegisterHotKey` | 无 | `Ctrl+Alt+*` 可能被输入法/显卡驱动占用；`Win+*` 多被系统占用。 |
| macOS | Carbon `RegisterEventHotKey` | 无（待 Spike 确认） | `⌃⌥Space` 可能被输入法切换占用；菜单栏应用较多。 |
| Linux X11 | `XGrabKey` | 无 | 与窗口管理器快捷键冲突；部分组合需 NumLock 变体一并 grab。 |
| Linux Wayland | 不注册 | — | 设置页禁用并说明。 |

- 默认组合：唤起 `Ctrl+Alt+Space`，新建任务 `Ctrl+Alt+N`，新建小记 `Ctrl+Alt+M`。
- 注册顺序：先注销旧组合，再注册新组合；新组合失败则恢复旧组合并报错，避免全部失效。
- 冲突处理：注册失败时 inline 显示「该组合已被其他程序或系统占用，请更换」，不阻止其他两项。
- 记录状态：`HotkeyRegistration { show_app: bool, new_task: bool, new_note: bool }`，启动与修改后广播。

---

## 7. 开机启动

| 平台 | 实现 | 验证方式 |
| --- | --- | --- |
| Windows | HKCU Run 键 | 写后读回，值匹配才视为成功。 |
| macOS | `SMAppService`（13+）/ LaunchAgent（12） | 查询注册状态；失败回滚。 |
| Linux | `~/.config/autostart/fastnote.desktop` | 文件存在且 `Exec` 包含 `--hidden`。 |

- 自启参数固定 `--hidden`：只显示常驻入口，不弹主窗口（D-14）。
- 应用移动安装路径后，自启项需在下次启动时自动修正。
- 设置切换失败：开关回滚到原状态 + 错误提示（不静默失败）。
- 便携版/未安装运行时不提供自启（路径不稳定）；UI 禁用并说明。

---

## 8. 单实例、唤醒与退出

- 启动时尝试获取数据目录下的实例锁（本地 socket + 锁文件）。
- 已存在实例：发送 `Show` 消息（携带可选路由，如 `--route=today`），本进程退出码 0。
- 恢复备份后重启：以 `--restored` 启动，应用正常进入默认路由，并在首帧后展示一次性「已从备份恢复」Toast；该参数不改变入口模式与路由。
- 原实例收到消息：显示主窗口并聚焦；若主窗口正在关闭动画中，等待完成后显示。
- 锁文件内容记录 PID 与启动时间，用于清理陈旧锁；不得因锁文件残留导致无法启动（超时后强制接管）。
- 系统会话结束（Windows `WM_QUERYENDSESSION`、macOS `applicationShouldTerminate`、Linux SIGTERM）：冲刷编辑内容、checkpoint、退出。
- 崩溃后重启：不显示崩溃对话框；日志记录，用户数据依赖 WAL 恢复。

---

## 9. 主题、文件与系统壳

### 9.1 主题检测

| 平台 | 深色判断 | 变化监听 |
| --- | --- | --- |
| Windows | `AppsUseLightTheme = 0` | `WM_SETTINGCHANGE`（`ImmersiveColorSet`） |
| macOS | `NSApp.effectiveAppearance.name` 含 `Dark` | `AppleInterfaceThemeChangedNotification` |
| Linux | Portal `color-scheme == 1`；回退 GTK 主题名含 `dark` | Portal `SettingChanged`；不可用时窗口获得焦点时检查 |

- 检测失败 → 浅色；用户手动选择 `light/dark` 时忽略系统。
- 主题切换不重启应用、不重建数据。

### 9.2 文件与目录

- 「打开数据目录」：Windows `explorer.exe <dir>`；macOS `open <dir>`；Linux `xdg-open <dir>`。
- 文件选择/保存使用原生对话框（`rfd`）；默认目录为上次使用目录（会话内记忆，不落库）。
- 剪贴板：复制小记正文、复制任务标题/链接；使用 `arboard`。
- 第一版**必须支持系统通知**（用户 2026-10-25 确认），任务与小记的定时提醒见 [REM-01](../11-reminders/01-reminders-and-notifications.md)。不依赖托盘模式，不自动把所有逾期任务转成系统通知。

### 9.3 系统通知适配

| 平台 | 适配方向 | 权限与验证 |
| --- | --- | --- |
| Windows | 原生 Toast，安装包注册稳定 AppUserModelID/激活身份 | 通知关闭时显示原因与系统设置入口；验证 per-user MSI 安装后的点击回调，不以开发模式成功代替安装包验收 |
| macOS | UserNotifications，本地即时通知 | 首次主动设置提醒时申请权限；签名 bundle、LSUIElement 场景均验收授权与点击定位 |
| Linux | Freedesktop Notifications（会话 D-Bus），按桌面能力降级 | 检测通知服务、actions 支持；无服务时显式应用内提醒，Wayland 不因全局快捷键禁用而禁用通知 |

- 平台仅负责 `NotificationStatus` 探测、请求授权、即时提交与点击回调；不注册应用完全退出后执行的系统计划，也不引入后台常驻服务。
- 系统勿扰/专注、权限和桌面实现决定实际呈现；提交成功不等同用户已看见，不绕过系统设置、不抢焦点、不强制声音。
- 点击通知：任务定位统一详情，小记定位编辑器；实体删除提示已不存在。回调能力缺失时保留系统通知并明确限制。
- 在 UI 主线程外进行可能阻塞的 IPC，复用平台事件通道回传；不得阻塞 DB worker，失败无无限重试。隐私正文策略与补发汇总详见 REM-01。

---

## 10. 多屏、DPI 与窗口状态

1. 所有 UI 尺寸使用逻辑像素；渲染由 Slint/winit 按 scale factor 处理。
2. 主窗口保存逻辑坐标 `{x,y,w,h}`，恢复时按坐标推断所在显示器并校正；悬浮球额外保存 `monitor` 标识。显示器标识不可用时按相对位置（屏幕宽高百分比）恢复。
3. 恢复窗口时确保标题栏与至少 80×80 区域在可见工作区内；否则居中到主屏。
4. 混合 DPI 拖动：跨越显示器时按新 scale factor 重算尺寸与位置，避免窗口跳变。
5. 全屏应用/演示模式：悬浮球保持在置顶层级（平台允许时）；不自动隐藏（用户可切到托盘模式）。
6. 虚拟桌面（Windows/macOS Spaces）：悬浮球默认在所有桌面可见（macOS `canJoinAllSpaces`）；托盘图标全局可见。

---

## 11. 打包、签名与分发

| 平台 | 产物 | 签名 | 运行时依赖 | 卸载行为 |
| --- | --- | --- | --- | --- |
| Windows | `.msi`（per-user，默认） | Authenticode（推荐） | VC++ 运行库（静态链接可免） | 保留用户数据；可选删除自启项 |
| macOS | `.dmg` / `.app` | Developer ID + 公证 | 无 | 删除自启项；保留 `~/Library/Application Support/FastNote` |
| Linux | AppImage + `.deb` | 无（校验和发布） | AppImage 内置 AppIndicator；deb 声明依赖 | 保留 `~/.local/share/fastnote` |

- 安装包不写入系统目录（Windows 可选 per-machine 版本，但默认 per-user）。
- 卸载时**默认保留数据**，并在卸载界面说明数据位置。
- 版本升级：覆盖安装；启动时自动迁移数据库；不做静默降级。
- 自动更新：第一版无（D-06）；用户手动下载安装包。

---

## 12. 预研任务（开发第 1 周）

| 编号 | 目标 | 通过标准 |
| --- | --- | --- |
| SPIKE-01 | 三平台悬浮球：透明、无边框、置顶、不占任务栏、拖动、位置记忆 | 三平台（Wayland 记录限制）均可稳定拖动与点击；无明显闪烁 |
| SPIKE-02 | Slint 中文 IME | 拼音组合输入、候选框定位、删除/替换正常；组合期间不触发快捷键 |
| SPIKE-03 | Linux 托盘 | Ubuntu GNOME（含扩展）/KDE 显示图标与菜单；无 SNI 时能检测并提示 |
| SPIKE-04 | 全局快捷键三平台 | 默认组合注册成功；冲突可检测；注销后不残留 |
| SPIKE-05 | macOS 无 Dock + 菜单栏 + LSUIElement | 启动无 Dock 图标；菜单栏点击/右键行为符合 §5.4 |
| SPIKE-06 | 原生文件对话框 | Windows/macOS 正常；Linux Portal 与 GTK 回退至少一条路径可用 |
| SPIKE-07 | 三平台系统通知（发布门禁） | 安装包身份、首次授权/拒绝恢复、隐藏驻留通知、点击定位、勿扰/无服务降级；GNOME/KDE 与 macOS LSUIElement 均留证 |

> **待确认 Q-03：** macOS 上 `LSUIElement=1` 且无 Dock 图标时，用户如何发现应用正在运行？当前默认：菜单栏图标 + 首次启动主窗口 + 设置页说明；不额外提供 Dock 图标选项。
> **待确认 Q-04：** Linux 下是否随包提供 AppIndicator 依赖（deb）或仅依赖系统安装。当前默认：AppImage 内置；deb 声明依赖并提示 GNOME 扩展。

---

## 13. 验收标准

| 编号 | 验收标准 |
| --- | --- |
| AC-PLAT-01 | 三平台均可创建常驻入口并打开主界面；入口不占任务栏/Dock。 |
| AC-PLAT-02 | 悬浮球可拖动、位置重启后保持；跨屏/改分辨率后不跑出屏幕。 |
| AC-PLAT-03 | 托盘左键打开主界面，右键菜单包含全部规定项并可执行。 |
| AC-PLAT-04 | 全局快捷键注册状态在设置页准确展示；冲突时不崩溃、可更换。 |
| AC-PLAT-05 | 开机启动开关写入与回滚正确，自启时不弹主窗口。 |
| AC-PLAT-06 | 单实例生效，第二次启动唤醒原实例并聚焦。 |
| AC-PLAT-07 | 系统主题切换后 1s 内应用跟随（设置为「跟随系统」时）。 |
| AC-PLAT-08 | Wayland 下能力探测正确，不支持的项禁用且说明原因。 |
| AC-PLAT-09 | 入口不可用时可降级到剩余可用形态，应用不会「隐身」。 |
| AC-PLAT-10 | 三平台安装包可安装/卸载，卸载默认保留数据。 |
| AC-PLAT-11 | 中文输入法在日记/小记中可正常组合输入（三平台）。 |
| AC-PLAT-12 | 关闭主窗口后按设置为「驻留入口」时进程仍在，入口可再次打开主界面。 |

---

## 14. 待确认问题

以下为本文件中所有未决问题的索引（正文见对应章节）：

| 编号 | 位置 | 问题摘要 | 当前默认处理 |
| --- | --- | --- | --- |
| Q-03 | §12 | macOS LSUIElement=1 无 Dock 图标时用户如何感知应用在运行 | 菜单栏图标 + 首次启动主窗 + 设置说明 |
| Q-04 | §12 | Linux 是否随包内置 AppIndicator 依赖 | AppImage 内置；deb 声明依赖并提示 GNOME 扩展 |

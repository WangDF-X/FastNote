# FastNote 品牌图标资源

概念：**纸 + 墨 + 折角 + F**。一块米白底板上放一张暖灰的实心便签，右上角沿 45° 折起一角，
纸面上镂空出字母 F。彩色只有折角那一小块蓝（占图形面积约 5%）。

这条路线同时解决三件事：实心纸块让 16px 下仍有分量；镂空 F 直接把品牌名写进图形；
折角给"笔记"一个具体的物理动作，而不是又一个待办勾选框。

## 为什么最终不是 SVG

| 用途 | 需要的格式 | 原因 |
|---|---|---|
| exe 文件图标 / 任务栏 / Alt+Tab | **ICO** | Win32 只认 `RT_GROUP_ICON` 资源，Windows 不会去解析 SVG |
| 运行中窗口图标 | `MainWindow.icon` 使用 SVG；`ICON_BIG` 单独使用 exe 资源 | Slint/winit 设置 `ICON_SMALL`，任务栏大图标由平台层补齐 |
| 托盘图标 | 位图（RGBA 或 `.ico` 路径） | `tray-icon` 只接受像素数据 |
| 应用内界面（侧边栏品牌区等） | **SVG 可用** | Slint 的 `@image-url()` 直接支持 SVG |

所以 SVG 是设计母版，位图 / ICO 是运行时产物。

## 色彩

| 名称 | Hex | 用途 |
|---|---|---|
| 米白底 Tile | `#E9DFC9` | 圆角底板；折角缺口露出的就是它 |
| 纸块 Body | `#2C2823` | 实心纸块的暖灰墨色（不是纯黑） |
| 纸色 Paper | `#F7F0DE` | 镂空 F、折角的"纸厚"边 |
| 品牌蓝 Blue | `#2E6BE6` | 折角；与 `palette.slint` 的 accent 同值 |
| 反白底 Dark | `#17181B` | 反白版底板（深色界面里相当于消失） |
| 反白蓝 | `#4A86FF` | 反白版折角（深色底上提亮，避免发闷） |
| 单色墨 Mono | `#2C2823` | 单色版唯一墨色，其余部分挖空 |

纸块用暖灰而不是纯黑，是因为纯黑会把整个图标压成一块墨疙瘩，暖灰保留对比但去掉了生硬感。
蓝色是图标里唯一的彩色，不要扩大它的面积。

不使用渐变、发光、玻璃质感、描边阴影。**唯一允许的"特效"是折痕一侧那道柔性暗部**（见下节）。

## 折角结构

折角不是叠在纸上的一个三角形，而是按"纸角折走 → 翻回纸面"的真实几何画的。五条规则缺一条就会退化：

1. **缺口**：纸块右上角沿 45° 折痕折走，缺口里露出底板 `#E9DFC9`。
2. **折角 = 折痕的镜像**：折角是那个被折走的直角三角形关于折痕的镜像，落在纸面内。
   所以它的两条自由边就是纸原来的上边和右边。
3. **纸厚边**：折角的两条自由边描一道纸色细边（`#F7F0DE`，宽 9，圆头），表示纸的厚度。
   实心纸块上缺这道边，折角会立刻退化成"贴上去的蓝色花瓣"。
4. **柔性暗部**：折痕一侧压一道 `feGaussianBlur stdDeviation=9`、`opacity 0.38`、宽 26 的暗部，
   并且用 `clipPath` 裁在折角内，让它只在折角上变暗，不溢出。
5. **折痕不加描边**：折痕不是纸边。在折痕上画线会让折角变成平贴的三角形。

**几何参数（母版）**：纸块 `x 312–740 / y 236–786`，圆角 70；折痕 `(580,236) → (740,396)`；
折角内角 `(580,396)` 圆角 `r=30`。以上坐标是缩放前的基准，四版统一用
`translate(512 512) scale(1.2561) translate(-526 -511)` 把纸块以自身中心放大、
落到画布中心（底板不动）。放大后纸块占底板宽度的 3/5，即 **边:块:边 = 1:3:1**；
不放大时留白过多，图形显得空。

> 踩过的坑：折角自由边上的圆角必须用真实弧。若写得像
> `A30 30 … (710,396) → (580,366)`，两点距离 133 > 直径 60，SVG 会按规范把半径等比放大到 66，
> 蓝色区域被弧线咬掉一大块，折角就成了"蓝色水滴"。圆角点应紧邻直角顶点：
> `L 610 396 A 30 30 0 0 1 580 366`。

## 字母 F

F 是**镂空**的（露出纸色），斜体：`skewX(-11)`，基线保持水平——不是整块旋转。

- 三条矩形：竖笔 `x396 y444 w56 h256`，上横 `w188 h56`，中横 `y562 w147 h56`，圆角 5。
- 位置固定在折角下方的左下区域：与纸左边留 84px、与纸底边留 86px、与折角下沿留 48px（缩放前坐标）。
- **"快"由两件事表达**：字母前倾 + 折痕那条 45° 对角线。不用速度线、不用斜切笔画端、
  不用蓝色光标块——这些方案在 16px 下都只会变成噪音，试过并否掉了。

## 文件

| 文件 | 用途 |
|---|---|
| `fastnote-app-icon.svg` | 主母版（40px 以上），唯一权威几何来源 |
| `fastnote-app-icon-small.svg` | 小尺寸版（16–32px），见下节 |
| `fastnote-app-icon-mono.svg` | 单色版（印刷、雕刻、单色场景）：只有一种墨，F 与纸厚边全部挖空 |
| `fastnote-app-icon-reversed.svg` | 反白版（深色界面、深色宣传图、深色托盘） |
| `fastnote.ico` | 16/20/24/32/40/48/64/128/256 多尺寸 Windows 图标 |
| `png/fastnote-*.png` | 16–1024 位图；`fastnote-mono-512.png`、`fastnote-reversed-512.png` 为变体 |
| `tools/build_icons.py` | 由 1024px 母版位图重新生成 `.ico` 与位图 |

## 尺寸分层

`16 / 20 / 24 / 32` 使用小尺寸版；`40` 及以上使用主母版。这是有意为之，不是渲染错误。

小尺寸版与母版同结构、同配色，只做四处针对像素的调整：

1. 纸块与母版一样放大 1.2561（两者必须取同一个比例，否则 32→40 会看到大小跳变）；
2. 折角放大到 190（母版 160），否则 16px 下蓝角只剩一个像素；
3. 字母笔画加粗到宽度的 0.34（母版 0.30），否则 F 糊成一块；
4. 去掉折痕柔性暗部（那几个像素只会变成脏点），纸厚边加粗到 13。

- 最小使用尺寸：16px
- 安全区：图形距画布边缘不小于画布宽度的 10%（左右 17.5%，上下 10%，母版已内置）

## 用法约束

- 不要把折角改成其他颜色，也不要删掉折角：它是唯一的彩色锚点。
- 不要把折角简化成"从角上切一刀再填个三角形"：缺口、镜像折角、纸厚边、柔性暗部、无描边折痕
  这五件事缺一件，折角就会退化成平贴的色块。**尤其不要在折痕上加描边**——折痕不是纸边。
- 不要把 F 填成蓝色或蓝色描边；F 只能是纸色镂空（单色版为透明挖空）。
- 不要在纸面上再加第二行字、图标或速度线。纸面只放 F 和折角。
- 不要给底板加投影、渐变或玻璃质感；暗部只允许出现在折痕一侧。
- 允许的反白：深色底用 `-reversed`，单色印刷用 `-mono`，不要手工改颜色。
  单色版是"一种墨 + 挖空"，换成两种墨会把折角压平。
- 与文字并排时，图标高度 = 文字行高的 1.2–1.4 倍。

## 重新生成

1. 用 Edge 无头模式把 4 个 SVG 渲染成 1024px 透明 PNG（Chrome / Chromium 同样命令）：

   ```powershell
   $edge = "C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe"
   & $edge --headless=new --disable-gpu --hide-scrollbars `
     --default-background-color=00000000 --window-size=1024,1024 `
     --screenshot="renders\fastnote-app-icon.png" `
     "file:///E:/FindWork/Project/FastNote/ui/branding/fastnote-app-icon.svg"
   ```

   渲染页需 `<body>` 无外边距、`img` 铺满 1024×1024，否则会留白或缩放。
   另外三个 SVG 同理，文件名分别是 `fastnote-app-icon-small.png`、`-mono.png`、`-reversed.png`。

2. 打包：

   ```powershell
   python ui\branding\tools\build_icons.py --src ui\branding\renders
   ```

   需要 Pillow。脚本会覆盖 `fastnote.ico` 与 `png/`。

## 接入现状

| 使用位置 | 接入方式 | 资源 |
|---|---|---|
| exe / 资源管理器文件图标 | `crates/fastnote-app/build.rs` 调用 Windows SDK 的 `rc.exe`，把图标编进 exe 资源 | `fastnote.ico`（16–256 九档） |
| 窗口标题栏 | 自绘 `TitleBar` 显示 16px 品牌图标；原生回退保留 `MainWindow.icon` / `ICON_SMALL`；按有效主题切换 | 浅色 `fastnote-app-icon-small.svg`（原生回退为主母版）；深色 `fastnote-app-icon-reversed.svg` |
| 任务栏 / Alt+Tab | `fastnote-platform::set_window_big_icon` 从 exe 资源加载 ico 并设置 `ICON_BIG` | `fastnote.ico` |
| 侧边栏品牌区（24px） | `ShellPalette.brand-icon-small`，按有效主题切换 | 浅色 `fastnote-app-icon-small.svg`；深色 `fastnote-app-icon-reversed.svg` |
| 悬浮球（32px） | `ShellPalette.brand-icon-small`，按有效主题切换 | 浅色 `fastnote-app-icon-small.svg`；深色 `fastnote-app-icon-reversed.svg` |
| 系统托盘 | `include_bytes!` + `png` 解码成 32px RGBA | `png/fastnote-32.png` |

- 2026-11-03 用户确认：主题切换同时让侧边栏、标题栏与悬浮球切换既有彩色/反色图标；不重新设计或生成资产。exe、任务栏/Alt+Tab 和系统托盘仍使用原图，不随应用主题切换。

- exe 图标不引入新依赖，只调用 `rc.exe`。脚本按 `RC` 环境变量 → `PATH` → Windows SDK 的顺序查找，
  找不到时构建失败并给出提示，不会静默生成没有图标的 exe。
- 改完图标重新构建即可生效；`build.rs` 已监听 `fastnote.ico` 的变更。
- 托盘没有复用 `fastnote.ico`：`tray-icon` 0.24 的 `Icon::from_rgba` 只接受单张位图，
  而 32px 正好是 Windows 托盘的默认边长。

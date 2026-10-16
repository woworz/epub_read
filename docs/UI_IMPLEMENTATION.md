# 第 2 版「清简留白」实施记录

日期：2026-10-16。分支：`feature/ui-minimal-reading`。核心代码已修改；本轮代码整理为独立分支提交，Windows x64 release 已交叉构建；真实 Windows 运行与最终视觉验收待完成。

## 已实现

- `src/theme.rs`：统一白天、纸张、夜间界面配色、间距、字体尺寸及交互状态；使用 MIT 许可的 Phosphor Regular 本地图标字体子集。没有新增运行时网络访问或 Cargo 依赖。
- `src/fonts.rs`：稳定的系统界面字体族与独立的正文、粗体字体族；切换正文排版不再同时改变工具栏和设置控件字体。
- `src/settings.rs`：新用户使用近白底色、宋体 22、1.8 行距、22 段距、680 正文宽度及默认收起目录。已有 JSON 中显式保存的值继续有效；恢复排版保留目录、透明度、自动显隐和老板键。
- `src/ui.rs`：三段式工具栏；居中阅读栏、章首书名小字及左对齐标题；通栏进度条和独立的章节信息、切章、百分比区域；按需侧栏、窄窗口临时目录；常用排版与高级窗口设置分组；适配小窗口的空状态。
- `src/render.rs`：章首装饰纳入虚拟布局坐标，复用书内已有标题，缺少标题时使用元数据回退；长正文按阅读栏宽度换行。继续按段落和段内偏移恢复位置。
- `src/picker.rs`：继承界面样式，限制小窗口中的对话框尺寸、提供滚动，并使用同一套目录图标。

同时修复了测试暴露的定位优先级问题：图片加载导致重排时，明确的键盘滚动请求不再被旧锚点覆盖；End 按重排后的实际章末定位。

应用仍采用原有 eframe/egui、glow、Windows 原生标题栏、EPUB 解析与本地存储机制。未修改 `src/native.rs`、原生显隐回调、全局热键注册、vendored eframe 或 Cargo 配置/锁文件。

## 检查结果及边界

| 检查 | 结果 | 范围 |
| --- | --- | --- |
| `cargo fmt --check` | 通过 | 应用源码格式 |
| `git diff --check` | 通过 | 已跟踪文件改动的空白检查 |
| `cargo check --locked --offline --tests --target x86_64-pc-windows-msvc` | 通过，无警告 | Windows 目标应用及测试的类型检查；见下方资源编译说明 |
| 隔离 Linux 回归 | **46 通过，0 失败** | 实际业务/UI 源码、真实 egui 输入处理；Win32 调用使用测试桩，字体替换为 Noto CJK |
| Linux 原生 glow 渲染截图 | 已取得并检查 | 阅读、设置、夜间、目录、小窗口目录及空状态，100% 密度 |
| Windows MSVC release exe | **已交叉构建并检查 PE** | Rust 1.99.0 + LLD + 真实 Windows SDK/CRT；图标与清单实际编译，详见 WINDOWS_BUILD.md |
| Windows 系统字体、高 DPI、透明度及显隐 | 待验收 | 无真实 Win32 窗口执行证据 |

Windows 类型检查通过 `work/check-bin/rc.exe` 临时空操作脚本跳过了图标资源编译，仅用于 `cargo check`，**没有修改生产 build.rs，也没有生成或验证可发布 exe**。

Linux 隔离入口在忽略的 `work/headless/`，通过文件引用编译实际的 EPUB、字体、排版、设置、存储、主题、UI 与回归源码。该入口额外启用 Linux x11/glow，并将 Win32 可见性/透明度调用替换为测试桩；存储替换使用 Unix rename；`WINDIR` 指向测试字体目录中的 Noto CJK 链接。上述替代均不进入生产源码，也不构成 Windows 原生功能测试。

```bash
source .tools/activate.sh
WINDIR="$PWD/work/fonts" CARGO_TARGET_DIR="$PWD/target/headless" \
  cargo test --offline --manifest-path work/headless/Cargo.toml \
  --target x86_64-unknown-linux-gnu --lib
```

此次新增 7 项回归，覆盖旧设置与恢复排版、窄窗口目录偏好及窗口放大后的临时状态清理、目录重排的锚点与进度轨道稳定性、最小窗口主题/设置、章首不重复且不跳过顶部、最小窗口打开按钮的实际点击，以及长正文不横向溢出。

既有测试继续覆盖滚动条拖动、进度拖动、键盘翻页、图片重排、章末滚动与保存恢复。相应交互测试改用实际滑轨矩形、真实轨道端点和中点一个轨道像素的定位容差；平滑滚动分别检查立即生效与稳定后的滚动量，避免依赖旧界面的全局滑块宽度或首帧滚动速度。

## 视觉证据

设计真值：[第 2 版参考图](design/ui-minimal-reference.png)。以下截图是隔离 Linux 入口的真实 egui/glow 渲染，使用 Noto CJK 替代字体，并非 Windows 实机截图。

- [1487 × 1058 主阅读界面](design/linux/reading.png)
- [1100 × 800 排版面板](design/linux/settings.png)
- [1100 × 800 夜间状态](design/linux/night.png)
- [1100 × 800 目录展开](design/linux/toc.png)
- [500 × 350 临时目录](design/linux/compact-toc.png)
- [500 × 350 空状态](design/linux/empty.png)
- [同尺寸设计/实现对照，左侧设计，右侧实现](design/linux/comparison.png)
- [章首局部对照](design/linux/header-comparison.png)

截图入口使用无持久化的专用样书和单章节元数据，故导航按钮置灰、全书进度为 0%；生产界面读取实际 EPUB 数据。该入口通过外部 X11 映射启动测试窗口，未改动生产 eframe 的窗口调度代码。图形启动与捕获属于开发验证，无法代表 Windows 的窗口显隐行为。

后续已实际生成 Windows release exe，完成 PE、资源与导入依赖检查，详情见 [Windows 构建记录](WINDOWS_BUILD.md)。前文临时 rc 脚本仅用于之前的类型检查，实际 release 构建使用 LLVM RC 编译资源。

最终视觉状态记录在 [design-qa.md](../design-qa.md)，当前为 `blocked`，主要待验证 Windows 字体和实际设备密度下的字形、换行、控件及原生窗口表现。

## Windows 上仍需完成

1. 在 MSVC/Windows SDK 环境执行原生 `cargo test --locked`、必要时复建 release，并运行已生成 exe 的 EPUB、存储自检。
2. 使用系统微软雅黑/宋体，对照第 2 版在 1100 × 800、1440 × 1024、500 × 350 及 100% / 150% / 200% DPI 下检查实际截图，校准标题字重和正文换行。
3. 验证打开/拖入 EPUB、真实多级目录与锚点、设置更改、重启恢复、沉浸/全屏，以及透明度、鼠标自动显隐、老板键。
4. 记录 Windows 视觉复核证据并解除 `design-qa.md` 的阻塞后，再作为可发布版本交付。

# 便携 EPUB 阅读器

Windows 10/11 x64 离线阅读器。运行 `epub-reader.exe` 即可，无安装程序、.NET、VC++ 动态运行库、WebView2 或浏览器内核。字体从 Windows 系统字体目录读取。

## 使用

- 文件菜单或 Ctrl+O 打开 EPUB；支持拖拽，以及 `epub-reader.exe "D:\图书\示例.epub"`。可在 Windows 的“打开方式”中手动选择 exe；程序不注册文件关联。
- 左侧目录可搜索、折叠、展开和跳转到小节锚点。底部显示当前章节和可拖动的全书进度。
- 设置中调节字体、字号、细/常规/粗、行距、段距、边距、正文最大宽度和主题颜色。
- 右键段落或 Ctrl+D 收藏整段，选择四种高亮颜色并填写一行备注。“我的标记”可跳转、编辑、删除和导出 Markdown / TXT。
- 使用程序内文件选择器；顶部支持粘贴文件夹或完整文件路径，单击文件后按“打开”，也可双击文件。导出到现有文件会要求确认覆盖。

## 快捷键

| 快捷键 | 操作 |
| --- | --- |
| Ctrl+O | 打开 EPUB |
| 滚轮、↑ / ↓ | 滚动正文；短章或章末继续向下滚轮进入下一章，章首向上滚轮回到上一章 |
| PageUp / PageDown | 向上 / 向下翻页 |
| 空格 / Shift+空格 | 向下 / 向上翻页 |
| Home / End | 当前章开头 / 结尾 |
| Ctrl+← / Ctrl+→ | 上一章 / 下一章 |
| Ctrl+G / F2 | 显示目录并聚焦搜索 |
| F4 | 显示 / 隐藏目录 |
| Ctrl+滚轮、Ctrl++ / Ctrl+- | 调节正文 12–48px 字号 |
| Ctrl+, | 排版设置 |
| Ctrl+D | 收藏 / 取消当前点击段落；滚动后默认可见顶部段落 |
| F10 | 沉浸阅读 / 恢复工具栏 |
| F11 | 全屏 |
| Esc | 关闭设置、退出全屏 |

## 数据与迁移

所有自动保存的数据都在 **exe 所在目录** 的 `data` 中，与工作目录无关：

```text
epub-reader.exe
data/
  settings.json                 # 排版、窗口、面板、上次书籍路径
  <EPUB文件内容的SHA-256>.json   # 阅读位置、高亮、备注
```

保存采用“写临时文件 → 同步 → Windows 原子替换”。修改约 800ms 后自动保存，正常退出也保存。整个程序文件夹可直接复制迁移。EPUB 本身不复制进 data；移动或改名后重新打开书籍，会用内容哈希关联原数据。上次书籍的绝对路径失效时，会提示重新打开。

程序不会自行写注册表或 AppData，不使用 eframe 持久化功能，没有联网和遥测代码。导出文件写入用户明确选择的位置。请把 exe 放在可写目录；只读位置不会悄悄改用 AppData。

## 构建

构建阶段需要 Rust stable MSVC 工具链与 MSVC C++ / Windows SDK。运行阶段不需要这些工具。当前构建使用 Rust 1.99.0、x86_64-pc-windows-msvc、eframe/egui 0.31.1 glow。

在已配置 MSVC 的环境运行：

```bat
build.bat
```

也可在 x64 Native Tools 命令行中执行：

```bat
cargo build --release --locked --target x86_64-pc-windows-msvc
```

`.cargo/config.toml` 已设置 `-C target-feature=+crt-static`。Release 使用 `opt-level="z"`、LTO、单个 codegen unit、panic abort 和 strip。`build.rs` 用 Windows SDK 的 `rc.exe` 嵌入小图标与 PerMonitorV2 DPI 清单。`build.bat` 将结果复制为项目根目录的 `epub-reader.exe`。

本开发工作区已在 `.tools` 中准备本地构建工具，build.bat 会自动使用；源码交付压缩包不包含这些大体积工具和 target 缓存。首次在其他电脑构建需联网下载 Cargo 依赖。

## 无窗口自检

```bat
epub-reader.exe --selftest "D:\图书\示例.epub"
epub-reader.exe --selftest-store
```

`--selftest` 输出书名、作者、章节数、多级目录、各章段落块数/图片数和耗时。段落块包含标题、正文、图片和分隔线。`--selftest-store` 执行标记写入、读取、校验、清理。成功退出码 0，失败 1，不创建窗口。

Release 是 GUI 子系统。控制台直接调用时 shell 可能不等待；在 CMD 可用 `start /wait "" epub-reader.exe --selftest-store`。重定向输出使用 UTF-8；Python subprocess 可可靠读取输出与退出码。

`fixtures` 提供 EPUB2 / EPUB3 自包含样书；`tools/make_fixtures.py` 和 `tools/smoke.py` 只用于开发，不是运行依赖。

## 范围与限制

仅支持未加密的 EPUB。正文支持标题、段落、粗斜体、换行、PNG/JPEG/GIF/WebP 图片和分隔线，忽略脚本及复杂 CSS。GIF/WebP 仅显示静态首帧。SVG、公式、音视频、复杂表格、竖排和固定版式未实现。无系统字体中的冷僻字形无法凭空补齐。

全书进度按“章节等权 + 章内段落位置”估算，并非原书印刷页码。位置保存为章节、段落和段内高度比例；相同排版精确恢复，重排后保持段落和相对偏移。首轮整章排版仍同步进行，缓存当前章的排版结果，只绘制可见段落。glow 需要可用的系统 OpenGL 图形驱动。

本阶段完成编译、自检和启动检查；完整功能、极端 EPUB、干净系统与多显示器测试留待下一阶段。详情见 [开发报告](DEVELOPMENT_REPORT.md)。

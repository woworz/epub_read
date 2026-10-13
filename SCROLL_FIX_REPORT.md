# 滚动恢复 bug 修复报告

日期：2026-10-13。仅修改滚动相关的底栏交互与重绘，未重构阅读器、未增加运行依赖。

## 复现步骤与根因

1. 打开 `fixtures/scroll-regression.epub`（两章，分别 301、1001 个段落块）。
2. 保存第一章 `paragraph=3, offset=0.01` 的位置，全书显示约 0.5%；退出并重开。
3. 在正文连续小幅滚轮，拖右侧滚动条，或按翻页键；观察随后空闲帧是否拉回、底栏进度是否变化。
4. 无窗口复现使用 241 段长章，恢复到第 120 段、段内 0.3，再渲染包括底栏、目录和正文的完整 UI 帧。

根因在 `src/ui.rs` 的 `Reader::footer()`：进度 Slider 的 `fixed_decimals(1)` 配合默认 `SliderClamping::Always`。本地 egui 0.31.1 的 `Slider::add_contents()` 每帧调用 `set_value(old_value)`，后者按小数位舍入；舍入也会 `mark_changed()`。阅读器将这个变化当成用户跳转，调用 `go()`，重新设置 `jump`，由 `Reader::reader()` 再次应用恢复偏移。

所以 `jump.take()` 本来已经正确消费一次，但底栏在之后的帧中不断重新生成它。仅渲染正文的旧测试不会触发此路径。修复前新增复现测试实际失败：滚轮帧偏移 27007.336，紧接着空闲帧被拉回 26962.754；初始段内位置也由 0.3 被舍入成 0.5。

`render.rs::Layout::prepare()` 已对整章全部段落排版，累计全部段高、段距和上下留白；`Reader::reader()` 分配整章高度后只绘制可见段落。高度总和和默认 ScrollArea 的范围不是主因，不需要替换虚拟化或滚动条。

## 改动

- 进度 Slider 使用 `SliderClamping::Edits`，保持一位小数显示和用户编辑时的范围限制，普通帧不舍入并回写阅读位置。
- 将章节名放到滑轨后方，避免跨章拖动时章节名长度变化导致滑轨横向移动、跳转目标漂移。这是本次进度拖动验证发现的相关问题。
- 恢复请求消费、阅读位置变化、边界滚轮切章时请求下一帧重绘，底栏与章节显示及时更新。
- `jump.take()`、`scroll_request.take()`、egui 默认滚动状态、图片重排锚点补偿与保存格式均沿用原实现。
- 两个几何观测字段仅在 `cfg(test)` 下编译，用于测试真实 egui 滑轨拖动，不进入 release。

## 测试与验证

`cargo test --locked`：**10 passed，0 failed**（原有 3 项，新增 7 项），包括：

- 241 段中段恢复后连续 8 次小幅 +12px 滚动，保留完整输入量，空闲帧不得回退，位置和百分比增加。
- 恢复只消费一次；20 个空闲帧不改变精确位置、不生成新恢复请求、不重建排版缓存。
- 全部段高总和、相邻段落高度索引、ScrollArea 实际 content_size 一致；上方图片从 60px 修正到 800px 时补偿 740px，保留当前段落和段内比例。
- 真实 egui 滚动条鼠标拖动到 0%、50%、100%，偏移匹配范围；释放后保持；底端视口触及真实章末；章末继续滚轮进入下一章。
- PageUp/PageDown、空格/Shift+空格、上下方向键、Home/End 均生效且下一帧不复位；目录锚点、段落跳转、字号/行距/宽度重排后仍可滚动。
- 真实进度 Slider 跨章拖到 25%、75%、100%、0%，位置变化正确；之后滚轮仍生效。
- 切书保存、退出保存、重新构造阅读器恢复位置后滚动仍生效。

`cargo fmt -- --check` 通过。`build.bat` 的 locked release 构建通过，Rust 1.99.0 / x86_64-pc-windows-msvc；源码与根目录 `epub-reader.exe` 已交付。

Release `--selftest` 在 EPUB2、EPUB3、滚动回归样书上均退出 0；`--selftest-store` 退出 0。日志见 `fixtures/scroll-fix-selftests.json`。

GUI 环境可以启动：修复前已实际打开隔离样书，观察 0.5% 恢复位置与滚轮现象，并在聊天中显示窗口截图。启动修复版进行验证时，Computer Use 报告用户按了物理 Escape 键停止操作；已遵守停止要求。**修复后的真实窗口滚轮、拖动、键盘与截图验证未完成**，不能将无窗口输入测试称为真实 GUI 验收。上述完整 egui UI 的多帧逻辑测试与代码审查提供了替代证据。

## 体积与便携约束

- 修复前：**2,392,064 bytes（2.28125 MiB）**。
- 修复后：**2,392,064 bytes（2.28125 MiB）**，差值 **0 bytes / 0%**。
- exe SHA-256：`D25694185E075FF2EAC255C1F3F6D30775C7C208141CE553651A24BA1B53974C`。
- `dumpbin /dependents` 前后均为同一组 14 个 Windows 系统 DLL，无 VCRUNTIME、MSVCP、ucrtbase、.NET、WebView2。原 `+crt-static` 构建配置保持不变。
- Cargo.toml、Cargo.lock、构建配置和存储路径代码未修改；运行仍只需单 exe，自动数据仍写在 exe 同目录的 `data`。开发样书、日志及测试辅助脚本不是运行依赖。
- 依赖记录：`fixtures/scroll-fix-baseline-dependencies.txt`、`fixtures/scroll-fix-dll-dependencies.txt`、`fixtures/scroll-fix-dll-imports.txt`、`fixtures/scroll-fix-cargo-tree.txt`。

## 相关限制

原有全书百分比仍按章节等权及段落位置估算。末屏保存的是视口顶部段落，显示进度可能略小于 100%；拖动 100% 和滚动条底端确实到章末，本次未改变进度算法或保存语义。egui 对非常长的章节有最小滑块显示长度，视觉比例会受其限制；实际滚动范围与总内容高度一致，本次保留默认滚动条。

未扩展字符级锚点、图片尺寸预扫描、分页、极长章节首次排版性能或多实例数据合并等范围。

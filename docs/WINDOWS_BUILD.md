# Windows x64 release 构建记录

日期：2026-10-16。分支：`feature/ui-minimal-reading`，基点 `4dd66bc`。构建包含当前未提交的第 2 版 UI 代码，不是已签名或已发布的 GitHub Release。

## 产物

- 项目根目录：`epub-reader.exe`。
- 便携目录：`dist/windows-x64/`，包含 exe、使用说明、第三方许可、构建信息和 SHA256SUMS。
- 完整便携包：`dist/epub-reader-windows-x64-ui-minimal.zip`。
- 校验文件：`dist/SHA256SUMS.txt`。

exe 大小：2,477,568 字节（约 2.36 MiB）。

exe SHA-256：

```text
aef2d66ba4ae74def348baf147b676e69957cf71143149e968869d8df735faae
```

本地构建输入摘要：`680259afc27938d45b172f06c192b904a175b9c52bd00200f994be787d25f435`。它覆盖 Cargo 文件、build.rs、src、assets 和 vendored eframe 文件；详细元数据见产物包 `build-info.json`。

## 实际构建方式

Linux 主机交叉编译，保持生产目标 `x86_64-pc-windows-msvc`，使用原有 release 设置和静态 CRT。没有更换为 GNU 目标，也没有使用之前类型检查的空操作 rc 脚本。

- Rust：1.99.0；链接器：Rust 自带 LLD 23.1.1。
- 资源编译器：LLVM RC 19.1.7，实际编译 `assets/app.rc`、图标及 DPI 清单。app.rc 没有预处理宏，因此使用 `-no-preprocess`，仍完整生成 Microsoft 二进制资源文件。
- Windows SDK：10.0.26100；MSVC desktop CRT 库：14.44.35226。
- SDK/CRT 由 xwin 0.10.0 从微软 VS 17 清单获取，清单构建号 17.14.37806.7。工具均保存在忽略的 `.tools/windows-cross/` 中。
- xwin 下载 SHA-256 已核对：`d870eb4b2f390878af6da1ccd3cf321d22fcb72720984853b4be732ae597fc88`。

本工作区中复建：

```bash
source .tools/activate.sh
export PATH="$PWD/.tools/windows-cross/bin:$PATH"
export CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_LINKER="$PWD/.tools/windows-cross/bin/link-msvc.sh"
export CARGO_TARGET_DIR="$PWD/target/windows-release"
cargo build --release --locked --offline --target x86_64-pc-windows-msvc
```

`rc.exe` 包装器调用真实 LLVM RC；链接器包装器把 CRT、SDK um/ucrt 的 x64 库路径传给 LLD。没有修改生产 build.rs、Cargo.toml、Cargo.lock 或 Windows 原生 `build.bat`。

完整日志保存在本工作区的 `work/windows-build/release-build.log`。链接器对微软库缺少外部 PDB 发出了 LNK4099 调试信息警告；链接成功，该警告不表示缺少运行时 DLL，不影响本次 PE/导入验证。原生 Windows 构建仍可使用原有 build.bat。

## 已检查

- release 构建退出码 0，完整生成 exe，非仅类型检查。
- PE32+、AMD64、Windows GUI 子系统。
- 存在 RT_ICON、RT_GROUP_ICON、RT_MANIFEST 资源；清单含 `asInvoker` 与 `PerMonitorV2,PerMonitor`。
- 导入 DLL 均为系统依赖，未发现 vcruntime、msvcp、ucrtbase、MinGW 运行库导入：

```text
ADVAPI32.dll
api-ms-win-core-synch-l1-2-0.dll
bcryptprimitives.dll
COMCTL32.dll
dwmapi.dll
GDI32.dll
imm32.dll
KERNEL32.dll
ntdll.dll
ole32.dll
OPENGL32.dll
SHELL32.dll
SHLWAPI.dll
USER32.dll
uxtheme.dll
```

- exe 与压缩包已计算 SHA-256，压缩包 CRC 完整性检查通过，包内 exe 与根目录 exe 一致。
- 前一轮隔离 Linux 回归为 46/46 通过。本轮未改变应用逻辑，仅补齐真实 release 工具链、生成和检查产物。

## 验证边界

本次没有在真实 Windows 上运行 exe，也没有在 Windows 上执行原生单元测试或图形验收。系统字体、高 DPI、全局热键、透明度、鼠标自动显隐和驱动兼容性仍待实机检查；`design-qa.md` 继续保留 blocked。

`.tools/`、`work/`、`dist/` 及 exe/zip 产物不进入 Git 源码提交。构建步骤本身没有执行提交或发布；后续 Git 提交与下载分享由用户另行授权，见分支及发布记录。

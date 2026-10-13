"""打包源码和便携程序，排除工具链、构建缓存、个人 data。"""
from pathlib import Path
import os, json, subprocess, zipfile, hashlib

root = Path(__file__).resolve().parent.parent
exe = root / "epub-reader.exe"
size = exe.stat().st_size
report = root / "DEVELOPMENT_REPORT.md"
text = report.read_text(encoding="utf-8").replace("EXE_BYTES", f"{size:,}").replace("EXE_MIB", f"{size/1024**2:.3f}")
report.write_text(text, encoding="utf-8")

env = os.environ.copy()
cargo = root / ".tools/cargo/bin/cargo.exe"
if cargo.exists():
    env["CARGO_HOME"] = str(root / ".tools/cargo")
    env["RUSTUP_HOME"] = str(root / ".tools/rustup")
else:
    cargo = "cargo"
meta = json.loads(subprocess.check_output([str(cargo), "metadata", "--locked", "--offline", "--filter-platform", "x86_64-pc-windows-msvc", "--format-version", "1"], cwd=root, env=env))
active = {node["id"] for node in meta["resolve"]["nodes"]}
notices = ["第三方依赖许可说明\n此文件为静态链接依赖及构建依赖的许可证汇编。应用无需额外文件才能运行；分发时请保留相关许可说明。\n"]
for p in sorted(meta["packages"], key=lambda p:p["name"]):
    if p["id"] not in active or p["name"] == "epub-reader":
        continue
    notices.append(f"\n{'='*72}\n{p['name']} {p['version']}\nLicense: {p.get('license')}\nRepository: {p.get('repository')}\n")
    crate = Path(p["manifest_path"]).parent
    for f in sorted(crate.iterdir()):
        if f.is_file() and f.name.upper().startswith(("LICENSE", "COPYING", "NOTICE")):
            notices.append(f"\n--- {f.name} ---\n{f.read_text(encoding='utf-8', errors='replace')}\n")
notice = root / "THIRD_PARTY_NOTICES.txt"
notice.write_text("".join(notices), encoding="utf-8")
documents = ["README.md", "DEVELOPMENT_REPORT.md", "THIRD_PARTY_NOTICES.txt"]
with zipfile.ZipFile(root / "epub-reader-portable.zip", "w", zipfile.ZIP_DEFLATED, compresslevel=9) as z:
    for name in ["epub-reader.exe", *documents]:
        z.write(root / name, name)
source = [root / name for name in ["Cargo.toml", "Cargo.lock", "build.rs", "build.bat", ".gitignore", ".cargo/config.toml", *documents]]
for directory in ["src", "assets", "tools"]:
    source.extend(p for p in (root / directory).rglob("*") if p.is_file() and "__pycache__" not in p.parts)
source.extend(p for p in (root / "fixtures").iterdir() if p.is_file())
with zipfile.ZipFile(root / "epub-reader-source.zip", "w", zipfile.ZIP_DEFLATED, compresslevel=9) as z:
    for p in sorted(source):
        z.write(p, p.relative_to(root).as_posix())
sha = hashlib.sha256(exe.read_bytes()).hexdigest()
print(f"EXE: {size} bytes ({size/1024**2:.3f} MiB)\nSHA256: {sha}\nPortable ZIP: {(root/'epub-reader-portable.zip').stat().st_size} bytes\nSource ZIP: {(root/'epub-reader-source.zip').stat().st_size} bytes")

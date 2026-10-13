"""开发阶段轻量检查：启动窗口、等待首窗、正常关闭；不是完整 GUI 测试。"""
from pathlib import Path
import ctypes, json, shutil, subprocess, time
from ctypes import wintypes

root = Path(__file__).resolve().parent.parent
out = root / "fixtures" / "portable"
out.mkdir(exist_ok=True)
exe = out / "epub-reader.exe"
shutil.copy2(root / "epub-reader.exe", exe)
u = ctypes.WinDLL("user32", use_last_error=True)
u.GetWindowThreadProcessId.argtypes = [wintypes.HWND, ctypes.POINTER(wintypes.DWORD)]
u.GetWindowTextW.argtypes = [wintypes.HWND, wintypes.LPWSTR, ctypes.c_int]
u.PostMessageW.argtypes = [wintypes.HWND, wintypes.UINT, wintypes.WPARAM, wintypes.LPARAM]
callback_type = ctypes.WINFUNCTYPE(wintypes.BOOL, wintypes.HWND, wintypes.LPARAM)
u.EnumWindows.argtypes = [callback_type, wintypes.LPARAM]
results = []
for args in ([], [str(root / "fixtures/sample-epub3.epub")]):
    start_info = subprocess.STARTUPINFO()
    start_info.dwFlags |= subprocess.STARTF_USESHOWWINDOW
    start_info.wShowWindow = 0
    start = time.perf_counter()
    p = subprocess.Popen([str(exe), *args], startupinfo=start_info, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    window = None
    while time.perf_counter() - start < 10:
        found = []
        @callback_type
        def callback(hwnd, unused):
            pid = wintypes.DWORD()
            u.GetWindowThreadProcessId(hwnd, ctypes.byref(pid))
            if pid.value == p.pid:
                title = ctypes.create_unicode_buffer(1024)
                u.GetWindowTextW(hwnd, title, 1024)
                if "EPUB" in title.value:
                    found.append((hwnd, title.value))
            return True
        u.EnumWindows(callback, 0)
        if found:
            window = found[0]
            break
        if p.poll() is not None:
            raise RuntimeError(f"Startup crashed: {p.communicate()}")
        time.sleep(0.03)
    elapsed = (time.perf_counter() - start) * 1000
    if not window:
        p.kill()
        raise RuntimeError("No application window found")
    time.sleep(2)
    if p.poll() is not None:
        raise RuntimeError(f"Reader crashed: {p.communicate()}")
    u.PostMessageW(window[0], 0x10, 0, 0)
    stdout, stderr = p.communicate(timeout=10)
    if p.returncode != 0:
        raise RuntimeError(f"Exit failed: {p.returncode} {stderr!r}")
    results.append({"args": args, "first_window_ms": round(elapsed, 1), "exit_code": p.returncode,
                    "stdout": stdout.decode("utf-8", errors="replace"), "stderr": stderr.decode("utf-8", errors="replace")})
settings = json.loads((out / "data/settings.json").read_text(encoding="utf-8"))
assert settings["last_book"].endswith("sample-epub3.epub")
results.append({"settings_saved": True, "book_data_files": len(list((out / "data").glob("*.json"))) - 1})
(root / "fixtures/startup-results.json").write_text(json.dumps(results, indent=2, ensure_ascii=False), encoding="utf-8")
print(json.dumps(results, indent=2, ensure_ascii=False))

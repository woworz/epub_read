"""验证无窗口 CLI 模式的输出、UTF-8、退出码与存储清理。"""
from pathlib import Path
import subprocess
root = Path(__file__).resolve().parent.parent
logs = []
for args, expected in [
    (["--selftest", str(root / "fixtures/sample-epub3.epub")], 0),
    (["--selftest", str(root / "fixtures/sample-epub2.epub")], 0),
    (["--selftest", str(root / "fixtures/entities.epub")], 0),
    (["--selftest-store"], 0),
    (["--selftest", str(root / "fixtures/missing.epub")], 1),
    (["--selftest"], 1),
]:
    r = subprocess.run([str(root / "epub-reader.exe"), *args], capture_output=True, timeout=15)
    text = r.stdout.decode("utf-8") + r.stderr.decode("utf-8")
    assert r.returncode == expected, (args, r.returncode, text)
    if expected == 0 and args[0] == "--selftest" and Path(args[1]).name == "entities.epub":
        assert "实体与编码自检样书" in text and "章节数：4" in text
        assert "实体与编码正文校验通过" in text and "“实体章0”" in text
    elif expected == 0 and args[0] == "--selftest":
        assert "便携阅读器开发样书" in text and "章节数：2" in text
        assert "段落数 1001" in text and "图片数 1" in text and "  - 小节" in text
    if args[0] == "--selftest-store":
        assert "存储自检通过" in text
        assert not list((root / "data").glob("selftest-*.json"))
    logs.append(f"COMMAND {args!r}\nEXIT {r.returncode}\n{text}")
output = "\n".join(logs)
(root / "fixtures/selftest-results.txt").write_text(output, encoding="utf-8")
print(output)

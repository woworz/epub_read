"""开发回归样书：两章均可滚动，便于恢复与跨章进度拖动验证。"""
from pathlib import Path
import zipfile

root = Path(__file__).resolve().parent.parent
with zipfile.ZipFile(root / "fixtures/sample-epub3.epub") as source:
    with zipfile.ZipFile(root / "fixtures/scroll-regression.epub", "w", zipfile.ZIP_DEFLATED) as output:
        for name in source.namelist():
            data = source.read(name)
            if name == "OPS/one.xhtml":
                data = ('<html xmlns="http://www.w3.org/1999/xhtml"><body>'
                        '<h1>第一章：开始阅读</h1>' + ''.join(
                            f'<p id="p{i}">第 {i} 段：滚动回归验证。' +
                            '中文排版与阅读位置保存。' * 10 + '</p>'
                            for i in range(300)) + '</body></html>').encode("utf-8")
            output.writestr(name, data)
print("Generated fixtures/scroll-regression.epub (301 + 1001 blocks)")

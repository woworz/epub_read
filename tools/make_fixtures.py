"""仅供开发自检：生成自包含 EPUB3 / EPUB2 样书，无第三方 Python 包。"""
from pathlib import Path
import zipfile, zlib, struct

root = Path(__file__).resolve().parent.parent / "fixtures"
root.mkdir(exist_ok=True)
def chunk(name, data):
    return struct.pack(">I", len(data)) + name + data + struct.pack(">I", zlib.crc32(name + data))
png = b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", 120, 60, 8, 2, 0, 0, 0))
png += chunk(b"IDAT", zlib.compress(b"".join(b"\0" + bytes((65, 120, 100)) * 120 for _ in range(60)))) + chunk(b"IEND", b"")
for version in (3, 2):
    with zipfile.ZipFile(root / f"sample-epub{version}.epub", "w", zipfile.ZIP_DEFLATED) as z:
        z.writestr("mimetype", "application/epub+zip", compress_type=zipfile.ZIP_STORED)
        z.writestr("META-INF/container.xml", '<container xmlns="urn:oasis:names:tc:opendocument:xmlns:container"><rootfiles><rootfile full-path="OPS/book.opf" media-type="application/oebps-package+xml"/></rootfiles></container>')
        nav = '<item id="nav" href="nav.xhtml" media-type="application/xhtml+xml" properties="nav"/>' if version == 3 else '<item id="ncx" href="toc.ncx" media-type="application/x-dtbncx+xml"/>'
        z.writestr("OPS/book.opf", f'<package xmlns="http://www.idpf.org/2007/opf" version="{version}.0" unique-identifier="id"><metadata xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:title>便携阅读器开发样书</dc:title><dc:creator>开发自检</dc:creator><dc:identifier id="id">fixture</dc:identifier></metadata><manifest>{nav}<item id="one" href="one.xhtml" media-type="application/xhtml+xml"/><item id="two" href="two.xhtml" media-type="application/xhtml+xml"/><item id="pic" href="image.png" media-type="image/png"/></manifest><spine toc="ncx"><itemref idref="one"/><itemref idref="two"/></spine></package>')
        z.writestr("OPS/nav.xhtml", '<html xmlns="http://www.w3.org/1999/xhtml" xmlns:epub="http://www.idpf.org/2007/ops"><body><nav epub:type="toc"><ol><li><a href="one.xhtml">第一章：开始阅读</a><ol><li><a href="one.xhtml#section">小节：图片与排版</a></li></ol></li><li><a href="two.xhtml">第二章：长章节</a></li></ol></nav></body></html>')
        z.writestr("OPS/toc.ncx", '<ncx xmlns="http://www.daisy.org/z3986/2005/ncx/"><navMap><navPoint id="a"><navLabel><text>第一章：开始阅读</text></navLabel><content src="one.xhtml"/><navPoint id="b"><navLabel><text>小节：图片与排版</text></navLabel><content src="one.xhtml#section"/></navPoint></navPoint><navPoint id="c"><navLabel><text>第二章：长章节</text></navLabel><content src="two.xhtml"/></navPoint></navMap></ncx>')
        z.writestr("OPS/one.xhtml", '<html xmlns="http://www.w3.org/1999/xhtml"><head><title>测试</title><style>ignored</style><script>ignored</script></head><body><h1>第一章：开始阅读</h1><p>这是一段中文正文。Hello EPUB! <strong>加粗文字</strong>与<em>斜体文字</em>。<br/>这里是换行后的文字。</p><h2 id="section">小节：图片与排版</h2><p>通过右键菜单打开排版设置。修改字号之后，阅读位置仍保留。</p><img src="image.png"/><hr/><p>本书是程序开发阶段的自检样书。</p></body></html>')
        z.writestr("OPS/two.xhtml", '<html xmlns="http://www.w3.org/1999/xhtml"><body><h1>第二章：长章节</h1>' + ''.join(f'<p id="p{i}">第 {i} 段：虚拟化阅读示例。窗外有风，书中有路。Paragraph {i}. ' + '中文排版与位置保存。' * 12 + '</p>' for i in range(1, 1001)) + '</body></html>')
        z.writestr("OPS/image.png", png)
with zipfile.ZipFile(root / "entities.epub", "w", zipfile.ZIP_DEFLATED) as z:
    z.writestr("mimetype", "application/epub+zip", compress_type=zipfile.ZIP_STORED)
    z.writestr("META-INF/container.xml", ('<container xmlns="urn:oasis:names:tc:opendocument:xmlns:container"><rootfiles><rootfile full-path="OPS/book.opf" media-type="application/oebps-package+xml"/></rootfiles></container>').encode("utf-8-sig"))
    opf = '<package xmlns="http://www.idpf.org/2007/opf" version="3.0"><metadata xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:title>实体与编码自检样书</dc:title><dc:creator>开发自检 &eacute;</dc:creator></metadata><manifest><item id="nav" href="nav.xhtml" media-type="application/xhtml+xml" properties="nav"/><item id="ncx" href="toc.ncx" media-type="application/x-dtbncx+xml"/>'
    opf += ''.join(f'<item id="c{i}" href="c{i}.xhtml" media-type="application/xhtml+xml"/>' for i in range(4))
    opf += '</manifest><spine toc="ncx">' + ''.join(f'<itemref idref="c{i}"/>' for i in range(4)) + '</spine></package>'
    z.writestr("OPS/book.opf", b"\xfe\xff" + opf.encode("utf-16-be"))
    nav = '<html xmlns="http://www.w3.org/1999/xhtml" xmlns:epub="http://www.idpf.org/2007/ops"><body><nav epub:type="toc"><ol>'
    nav += ''.join(f'<li><a href="c{i}.xhtml">&ldquo;实体章{i}&rdquo;</a></li>' for i in range(4))
    nav += '</ol></nav></body></html>'
    z.writestr("OPS/nav.xhtml", b"\xfe\xff" + nav.encode("utf-16-be"))
    ncx = '<ncx xmlns="http://www.daisy.org/z3986/2005/ncx/"><navMap>'
    ncx += ''.join(f'<navPoint id="c{i}"><navLabel><text>&ldquo;实体章{i}&rdquo;</text></navLabel><content src="c{i}.xhtml"/></navPoint>' for i in range(4))
    ncx += '</navMap></ncx>'
    z.writestr("OPS/toc.ncx", b"\xff\xfe" + ncx.encode("utf-16-le"))
    chapter = '''<!DOCTYPE html [
<!ENTITY eacute "声明优先">
<!ENTITY local "自定义">
]><html xmlns="http://www.w3.org/1999/xhtml"><body>
<pre>命名：&ldquo;&rdquo;&#233;&nbsp;&lsquo;&rsquo;&times;&middot;&mdash;&ndash;&hellip;&copy;</pre>
<pre>XML：&amp; &lt; &gt; &quot; &apos;</pre>
<pre>数字：&#x4E2D; &#20013;</pre>
<pre>裸引用：AT&T &foo; &#bad;</pre>
<pre><![CDATA[CDATA：&rdquo; &foo; AT&T]]></pre>
<!-- 注释中的 &ldquo; &foo; AT&T 保持原样 -->
<pre>DTD：&eacute; &local;</pre>
<pre>非法数字：&#0; &#xD800; &#1114112; &#99999999999999999999;</pre>
</body></html>'''
    z.writestr("OPS/c0.xhtml", chapter.encode("utf-8-sig"))
    z.writestr("OPS/c1.xhtml", b"\xfe\xff" + '<html><body><p>UTF-16BE：中文 &ldquo;&eacute;&rdquo;</p></body></html>'.encode("utf-16-be"))
    z.writestr("OPS/c2.xhtml", b"\xff\xfe" + '<html><body><p>UTF-16LE：中文 &lsquo;&eacute;&rsquo;</p></body></html>'.encode("utf-16-le"))
    z.writestr("OPS/c3.xhtml", '<html><body><p>损坏UTF-8：'.encode("utf-8") + b"\xff" + ' 可继续阅读</p></body></html>'.encode("utf-8"))
print("Generated EPUB2/EPUB3 and entity fixtures")

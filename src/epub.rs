use crate::model::*;
use roxmltree::{Document, Node};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    fs::File,
    io::{Read, Seek},
    path::Path,
};
use zip::ZipArchive;
const MAX_ENTRY: u64 = 64 * 1024 * 1024;
pub fn hash(data: &[u8]) -> String {
    format!("{:x}", Sha256::digest(data))
}
pub struct Book {
    pub hash: String,
    pub title: String,
    pub author: String,
    pub chapters: Vec<Chapter>,
    pub toc: Vec<Toc>,
    archive: ZipArchive<File>,
    cache: HashMap<usize, Vec<Paragraph>>,
}
fn read_entry<R: Read + Seek>(zip: &mut ZipArchive<R>, path: &str) -> Result<Vec<u8>, String> {
    let f = zip
        .by_name(path)
        .map_err(|_| format!("EPUB 缺少文件：{path}"))?;
    if f.size() > MAX_ENTRY {
        return Err(format!("文件过大（限制 64MB）：{path}"));
    }
    let mut bytes = Vec::new();
    f.take(MAX_ENTRY + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() as u64 > MAX_ENTRY {
        return Err("解压内容过大".into());
    }
    Ok(bytes)
}
fn xml(bytes: &[u8]) -> Result<String, String> {
    let s = if bytes.starts_with(&[0xff, 0xfe]) {
        String::from_utf16_lossy(
            &bytes[2..]
                .chunks_exact(2)
                .map(|c| u16::from_le_bytes([c[0], c[1]]))
                .collect::<Vec<_>>(),
        )
    } else {
        String::from_utf8(bytes.to_vec())
            .map_err(|_| "EPUB XML 不是有效 UTF-8/UTF-16LE".to_string())?
    };
    Ok(s.replace("&nbsp;", "&#160;")
        .replace("&mdash;", "&#8212;")
        .replace("&ndash;", "&#8211;")
        .replace("&hellip;", "&#8230;")
        .replace("&copy;", "&#169;"))
}
fn parse(s: &str) -> Result<Document<'_>, String> {
    Document::parse_with_options(
        s,
        roxmltree::ParsingOptions {
            allow_dtd: true,
            ..Default::default()
        },
    )
    .map_err(|e| format!("XML/XHTML 解析失败：{e}"))
}
fn decode_path(s: &str) -> String {
    let mut out = Vec::new();
    let b = s.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let Ok(digits) = std::str::from_utf8(&b[i + 1..i + 3]) {
                if let Ok(v) = u8::from_str_radix(digits, 16) {
                    out.push(v);
                    i += 3;
                    continue;
                }
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into()
}
pub fn resolve(base: &str, href: &str) -> String {
    let href = decode_path(href.split('#').next().unwrap_or(""));
    let mut parts: Vec<&str> = if href.starts_with('/') {
        Vec::new()
    } else {
        base.rsplit_once('/')
            .map(|(a, _)| a.split('/').collect())
            .unwrap_or_default()
    };
    for part in href.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            _ => parts.push(part),
        }
    }
    parts.join("/")
}
fn content(n: Node<'_, '_>) -> String {
    n.descendants()
        .filter(|n| n.is_text())
        .filter_map(|n| n.text())
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}
fn tag(n: Node<'_, '_>, name: &str) -> bool {
    n.is_element() && n.tag_name().name() == name
}
impl Book {
    pub fn open(path: &Path) -> Result<Self, String> {
        let mut f = File::open(path).map_err(|e| format!("无法打开 {}：{e}", path.display()))?;
        let mut digest = Sha256::new();
        let mut buf = [0; 65536];
        loop {
            let n = f.read(&mut buf).map_err(|e| e.to_string())?;
            if n == 0 {
                break;
            }
            digest.update(&buf[..n]);
        }
        f.rewind().map_err(|e| e.to_string())?;
        let mut archive = ZipArchive::new(f).map_err(|e| format!("不是有效的 EPUB/ZIP：{e}"))?;
        let container = xml(&read_entry(&mut archive, "META-INF/container.xml")?)?;
        let doc = parse(&container)?;
        let opf_path = doc
            .descendants()
            .find(|n| tag(*n, "rootfile"))
            .and_then(|n| n.attribute("full-path"))
            .ok_or("container.xml 缺少 rootfile")?
            .to_string();
        let opf = xml(&read_entry(&mut archive, &opf_path)?)?;
        let doc = parse(&opf)?;
        let title = doc
            .descendants()
            .find(|n| tag(*n, "title"))
            .map(content)
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| {
                path.file_stem()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into()
            });
        let author = doc
            .descendants()
            .filter(|n| tag(*n, "creator"))
            .map(content)
            .collect::<Vec<_>>()
            .join("、");
        let mut manifest = HashMap::new();
        let mut nav = None;
        for n in doc.descendants().filter(|n| tag(*n, "item")) {
            if let (Some(id), Some(href)) = (n.attribute("id"), n.attribute("href")) {
                let p = resolve(&opf_path, href);
                if n.attribute("properties")
                    .unwrap_or("")
                    .split_whitespace()
                    .any(|p| p == "nav")
                {
                    nav = Some(p.clone());
                }
                manifest.insert(
                    id.to_string(),
                    (p, n.attribute("media-type").unwrap_or("").to_string()),
                );
            }
        }
        let spine = doc
            .descendants()
            .find(|n| tag(*n, "spine"))
            .ok_or("OPF 缺少 spine")?;
        let mut chapters = Vec::new();
        for n in spine.children().filter(|n| tag(*n, "itemref")) {
            if n.attribute("linear") == Some("no") {
                continue;
            }
            let id = n.attribute("idref").ok_or("spine 缺少 idref")?;
            let (p, _) = manifest
                .get(id)
                .ok_or_else(|| format!("manifest 缺少 {id}"))?;
            chapters.push(Chapter {
                id: id.into(),
                path: p.clone(),
                title: format!("第 {} 章", chapters.len() + 1),
            });
        }
        if chapters.is_empty() {
            return Err("EPUB 没有可阅读的章节".into());
        }
        let mut toc = Vec::new();
        if let Some(p) = nav {
            if let Ok(bytes) = read_entry(&mut archive, &p) {
                if let Ok(s) = xml(&bytes) {
                    if let Ok(d) = parse(&s) {
                        let root = d.descendants().find(|n| {
                            tag(*n, "nav")
                                && n.attributes().any(|a| {
                                    a.name() == "type"
                                        && a.value().split_whitespace().any(|s| s == "toc")
                                })
                        });
                        if let Some(root) = root {
                            if let Some(ol) = root.children().find(|n| tag(*n, "ol")) {
                                toc = nav_list(ol, &p, &chapters);
                            }
                        }
                    }
                }
            }
        }
        if toc.is_empty() {
            let ncx = spine
                .attribute("toc")
                .and_then(|id| manifest.get(id))
                .or_else(|| {
                    manifest
                        .values()
                        .find(|(_, t)| t == "application/x-dtbncx+xml")
                });
            if let Some((p, _)) = ncx {
                if let Ok(bytes) = read_entry(&mut archive, p) {
                    if let Ok(s) = xml(&bytes) {
                        if let Ok(d) = parse(&s) {
                            if let Some(root) = d.descendants().find(|n| tag(*n, "navMap")) {
                                toc = ncx_list(root, p, &chapters);
                            }
                        }
                    }
                }
            }
        }
        fn assign(nodes: &[Toc], chapters: &mut [Chapter]) {
            for n in nodes {
                if chapters[n.chapter].title.starts_with("第 ") {
                    chapters[n.chapter].title = n.title.clone();
                }
                assign(&n.children, chapters);
            }
        }
        assign(&toc, &mut chapters);
        if toc.is_empty() {
            toc = chapters
                .iter()
                .enumerate()
                .map(|(i, c)| Toc {
                    title: c.title.clone(),
                    chapter: i,
                    anchor: String::new(),
                    children: Vec::new(),
                })
                .collect();
        }
        Ok(Self {
            hash: format!("{:x}", digest.finalize()),
            title,
            author,
            chapters,
            toc,
            archive,
            cache: HashMap::new(),
        })
    }
    pub fn chapter(&mut self, index: usize) -> Result<&Vec<Paragraph>, String> {
        if !self.cache.contains_key(&index) {
            let path = self.chapters.get(index).ok_or("章节索引越界")?.path.clone();
            let s = xml(&read_entry(&mut self.archive, &path)?)?;
            let doc = parse(&s)?;
            let root = doc
                .descendants()
                .find(|n| tag(*n, "body"))
                .unwrap_or(doc.root_element());
            let mut out = Vec::new();
            let mut pending = Paragraph {
                kind: Kind::Text,
                runs: Vec::new(),
                anchor: String::new(),
            };
            walk(root, &path, &mut out, &mut pending, false, false, false);
            flush(&mut out, &mut pending);
            if out.is_empty() {
                out.push(Paragraph {
                    kind: Kind::Text,
                    runs: vec![Run {
                        text: "（本章没有可显示的正文）".into(),
                        ..Default::default()
                    }],
                    anchor: String::new(),
                });
            }
            // 只保留最近四章的内容，正文排版缓存另由渲染模块维护。
            if self.cache.len() >= 4 {
                if let Some(k) = self.cache.keys().copied().find(|k| *k != index) {
                    self.cache.remove(&k);
                }
            }
            self.cache.insert(index, out);
        }
        Ok(&self.cache[&index])
    }
    pub fn image(&mut self, path: &str) -> Result<Vec<u8>, String> {
        read_entry(&mut self.archive, path)
    }
}
fn destination(base: &str, href: &str, chapters: &[Chapter]) -> Option<(usize, String)> {
    let p = resolve(base, href);
    chapters.iter().position(|c| c.path == p).map(|i| {
        (
            i,
            decode_path(href.split_once('#').map(|(_, a)| a).unwrap_or("")),
        )
    })
}
fn nav_list(root: Node<'_, '_>, base: &str, chapters: &[Chapter]) -> Vec<Toc> {
    let mut out = Vec::new();
    for li in root.children().filter(|n| tag(*n, "li")) {
        let children = li
            .children()
            .find(|n| tag(*n, "ol"))
            .map(|n| nav_list(n, base, chapters))
            .unwrap_or_default();
        let label = li.children().find(|n| tag(*n, "a") || tag(*n, "span"));
        if let Some(a) = label {
            let dest = a
                .attribute("href")
                .and_then(|h| destination(base, h, chapters))
                .or_else(|| {
                    children
                        .first()
                        .map(|c: &Toc| (c.chapter, c.anchor.clone()))
                });
            if let Some((chapter, anchor)) = dest {
                out.push(Toc {
                    title: content(a),
                    chapter,
                    anchor,
                    children,
                });
            } else {
                out.extend(children);
            }
        }
    }
    out
}
fn ncx_list(root: Node<'_, '_>, base: &str, chapters: &[Chapter]) -> Vec<Toc> {
    let mut out = Vec::new();
    for n in root.children().filter(|n| tag(*n, "navPoint")) {
        let children = ncx_list(n, base, chapters);
        if let Some((chapter, anchor)) = n
            .children()
            .find(|n| tag(*n, "content"))
            .and_then(|n| n.attribute("src"))
            .and_then(|h| destination(base, h, chapters))
        {
            let title = n
                .children()
                .find(|n| tag(*n, "navLabel"))
                .map(content)
                .unwrap_or_default();
            out.push(Toc {
                title,
                chapter,
                anchor,
                children,
            });
        } else {
            out.extend(children);
        }
    }
    out
}
fn flush(out: &mut Vec<Paragraph>, p: &mut Paragraph) {
    if !p.runs.is_empty() && !p.text().trim().is_empty() {
        out.push(p.clone());
        p.anchor.clear();
    }
    p.runs.clear();
    p.kind = Kind::Text;
}
fn walk(
    n: Node<'_, '_>,
    base: &str,
    out: &mut Vec<Paragraph>,
    p: &mut Paragraph,
    bold: bool,
    italic: bool,
    pre: bool,
) {
    if n.is_text() {
        let raw = n.text().unwrap_or("");
        let text = if pre {
            raw.to_string()
        } else {
            let mut s = String::new();
            for c in raw.chars() {
                if c.is_whitespace() {
                    if !s.ends_with(' ') {
                        s.push(' ');
                    }
                } else {
                    s.push(c);
                }
            }
            s
        };
        if !text.is_empty() {
            p.runs.push(Run { text, bold, italic });
        }
        return;
    }
    if !n.is_element() {
        return;
    }
    let name = n.tag_name().name();
    if matches!(name, "script" | "style" | "head" | "noscript") {
        return;
    }
    let block = matches!(
        name,
        "p" | "div"
            | "section"
            | "article"
            | "blockquote"
            | "li"
            | "pre"
            | "h1"
            | "h2"
            | "h3"
            | "h4"
            | "h5"
            | "h6"
            | "tr"
            | "figure"
            | "figcaption"
    );
    if block {
        flush(out, p);
    }
    if let Some(id) = n.attribute("id") {
        if p.anchor.is_empty() {
            p.anchor = id.into();
        }
    }
    if name.len() == 2 && name.starts_with('h') {
        if let Ok(level) = name[1..].parse::<u8>() {
            p.kind = Kind::Heading(level);
        }
    }
    if name == "br" {
        p.runs.push(Run {
            text: "\n".into(),
            bold,
            italic,
        });
    } else if name == "img" || name == "image" {
        let src = n.attribute("src").or_else(|| {
            n.attributes()
                .find(|a| a.name() == "href")
                .map(|a| a.value())
        });
        if let Some(src) = src {
            flush(out, p);
            out.push(Paragraph {
                kind: Kind::Image(resolve(base, src)),
                runs: Vec::new(),
                anchor: n.attribute("id").unwrap_or("").into(),
            });
        }
    } else if name == "hr" {
        flush(out, p);
        out.push(Paragraph {
            kind: Kind::Rule,
            runs: Vec::new(),
            anchor: String::new(),
        });
    } else {
        for child in n.children() {
            walk(
                child,
                base,
                out,
                p,
                bold || matches!(name, "b" | "strong"),
                italic || matches!(name, "i" | "em"),
                pre || name == "pre",
            );
        }
    }
    if block {
        flush(out, p);
    }
}
pub fn selftest(path: &Path) -> Result<String, String> {
    let start = std::time::Instant::now();
    let mut book = Book::open(path)?;
    let mut s = format!(
        "书名：{}\n作者：{}\n章节数：{}\n目录：\n",
        book.title,
        book.author,
        book.chapters.len()
    );
    fn tree(nodes: &[Toc], depth: usize, s: &mut String) {
        for n in nodes {
            s.push_str(&format!(
                "{}- {} [章 {} #{}]\n",
                "  ".repeat(depth),
                n.title,
                n.chapter + 1,
                n.anchor
            ));
            tree(&n.children, depth + 1, s);
        }
    }
    tree(&book.toc, 0, &mut s);
    for i in 0..book.chapters.len() {
        let title = book.chapters[i].title.clone();
        let p = book.chapter(i)?;
        s.push_str(&format!(
            "章 {} {}：段落数 {}，图片数 {}\n",
            i + 1,
            title,
            p.len(),
            p.iter()
                .filter(|p| matches!(p.kind, Kind::Image(_)))
                .count()
        ));
    }
    s.push_str(&format!("解析耗时：{} ms\n", start.elapsed().as_millis()));
    Ok(s)
}

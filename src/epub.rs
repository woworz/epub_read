use crate::model::*;
use roxmltree::{Document, Node};
use sha2::{Digest, Sha256};
use std::{
    collections::{HashMap, HashSet},
    fs::File,
    io::{Read, Seek},
    path::Path,
};
use zip::ZipArchive;
const MAX_ENTRY: u64 = 64 * 1024 * 1024;
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
    let s = if bytes.starts_with(&[0xff, 0xfe]) || bytes.starts_with(&[0xfe, 0xff]) {
        let little_endian = bytes[0] == 0xff;
        let mut units = bytes[2..].chunks_exact(2);
        let mut s = String::from_utf16_lossy(
            &units
                .by_ref()
                .map(|c| {
                    if little_endian {
                        u16::from_le_bytes([c[0], c[1]])
                    } else {
                        u16::from_be_bytes([c[0], c[1]])
                    }
                })
                .collect::<Vec<_>>(),
        );
        if !units.remainder().is_empty() {
            s.push('\u{fffd}');
        }
        s
    } else {
        String::from_utf8_lossy(bytes.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(bytes))
            .into_owned()
    };
    Ok(normalize_entities(&s))
}

// XHTML 1.0 / HTML4 的 252 个命名实体，按名称排序；apos 单独按 XML 预定义处理。
const XHTML_ENTITIES: &[(&str, u32)] = &[
    ("AElig", 198),
    ("Aacute", 193),
    ("Acirc", 194),
    ("Agrave", 192),
    ("Alpha", 913),
    ("Aring", 197),
    ("Atilde", 195),
    ("Auml", 196),
    ("Beta", 914),
    ("Ccedil", 199),
    ("Chi", 935),
    ("Dagger", 8225),
    ("Delta", 916),
    ("ETH", 208),
    ("Eacute", 201),
    ("Ecirc", 202),
    ("Egrave", 200),
    ("Epsilon", 917),
    ("Eta", 919),
    ("Euml", 203),
    ("Gamma", 915),
    ("Iacute", 205),
    ("Icirc", 206),
    ("Igrave", 204),
    ("Iota", 921),
    ("Iuml", 207),
    ("Kappa", 922),
    ("Lambda", 923),
    ("Mu", 924),
    ("Ntilde", 209),
    ("Nu", 925),
    ("OElig", 338),
    ("Oacute", 211),
    ("Ocirc", 212),
    ("Ograve", 210),
    ("Omega", 937),
    ("Omicron", 927),
    ("Oslash", 216),
    ("Otilde", 213),
    ("Ouml", 214),
    ("Phi", 934),
    ("Pi", 928),
    ("Prime", 8243),
    ("Psi", 936),
    ("Rho", 929),
    ("Scaron", 352),
    ("Sigma", 931),
    ("THORN", 222),
    ("Tau", 932),
    ("Theta", 920),
    ("Uacute", 218),
    ("Ucirc", 219),
    ("Ugrave", 217),
    ("Upsilon", 933),
    ("Uuml", 220),
    ("Xi", 926),
    ("Yacute", 221),
    ("Yuml", 376),
    ("Zeta", 918),
    ("aacute", 225),
    ("acirc", 226),
    ("acute", 180),
    ("aelig", 230),
    ("agrave", 224),
    ("alefsym", 8501),
    ("alpha", 945),
    ("amp", 38),
    ("and", 8743),
    ("ang", 8736),
    ("aring", 229),
    ("asymp", 8776),
    ("atilde", 227),
    ("auml", 228),
    ("bdquo", 8222),
    ("beta", 946),
    ("brvbar", 166),
    ("bull", 8226),
    ("cap", 8745),
    ("ccedil", 231),
    ("cedil", 184),
    ("cent", 162),
    ("chi", 967),
    ("circ", 710),
    ("clubs", 9827),
    ("cong", 8773),
    ("copy", 169),
    ("crarr", 8629),
    ("cup", 8746),
    ("curren", 164),
    ("dArr", 8659),
    ("dagger", 8224),
    ("darr", 8595),
    ("deg", 176),
    ("delta", 948),
    ("diams", 9830),
    ("divide", 247),
    ("eacute", 233),
    ("ecirc", 234),
    ("egrave", 232),
    ("empty", 8709),
    ("emsp", 8195),
    ("ensp", 8194),
    ("epsilon", 949),
    ("equiv", 8801),
    ("eta", 951),
    ("eth", 240),
    ("euml", 235),
    ("euro", 8364),
    ("exist", 8707),
    ("fnof", 402),
    ("forall", 8704),
    ("frac12", 189),
    ("frac14", 188),
    ("frac34", 190),
    ("frasl", 8260),
    ("gamma", 947),
    ("ge", 8805),
    ("gt", 62),
    ("hArr", 8660),
    ("harr", 8596),
    ("hearts", 9829),
    ("hellip", 8230),
    ("iacute", 237),
    ("icirc", 238),
    ("iexcl", 161),
    ("igrave", 236),
    ("image", 8465),
    ("infin", 8734),
    ("int", 8747),
    ("iota", 953),
    ("iquest", 191),
    ("isin", 8712),
    ("iuml", 239),
    ("kappa", 954),
    ("lArr", 8656),
    ("lambda", 955),
    ("lang", 9001),
    ("laquo", 171),
    ("larr", 8592),
    ("lceil", 8968),
    ("ldquo", 8220),
    ("le", 8804),
    ("lfloor", 8970),
    ("lowast", 8727),
    ("loz", 9674),
    ("lrm", 8206),
    ("lsaquo", 8249),
    ("lsquo", 8216),
    ("lt", 60),
    ("macr", 175),
    ("mdash", 8212),
    ("micro", 181),
    ("middot", 183),
    ("minus", 8722),
    ("mu", 956),
    ("nabla", 8711),
    ("nbsp", 160),
    ("ndash", 8211),
    ("ne", 8800),
    ("ni", 8715),
    ("not", 172),
    ("notin", 8713),
    ("nsub", 8836),
    ("ntilde", 241),
    ("nu", 957),
    ("oacute", 243),
    ("ocirc", 244),
    ("oelig", 339),
    ("ograve", 242),
    ("oline", 8254),
    ("omega", 969),
    ("omicron", 959),
    ("oplus", 8853),
    ("or", 8744),
    ("ordf", 170),
    ("ordm", 186),
    ("oslash", 248),
    ("otilde", 245),
    ("otimes", 8855),
    ("ouml", 246),
    ("para", 182),
    ("part", 8706),
    ("permil", 8240),
    ("perp", 8869),
    ("phi", 966),
    ("pi", 960),
    ("piv", 982),
    ("plusmn", 177),
    ("pound", 163),
    ("prime", 8242),
    ("prod", 8719),
    ("prop", 8733),
    ("psi", 968),
    ("quot", 34),
    ("rArr", 8658),
    ("radic", 8730),
    ("rang", 9002),
    ("raquo", 187),
    ("rarr", 8594),
    ("rceil", 8969),
    ("rdquo", 8221),
    ("real", 8476),
    ("reg", 174),
    ("rfloor", 8971),
    ("rho", 961),
    ("rlm", 8207),
    ("rsaquo", 8250),
    ("rsquo", 8217),
    ("sbquo", 8218),
    ("scaron", 353),
    ("sdot", 8901),
    ("sect", 167),
    ("shy", 173),
    ("sigma", 963),
    ("sigmaf", 962),
    ("sim", 8764),
    ("spades", 9824),
    ("sub", 8834),
    ("sube", 8838),
    ("sum", 8721),
    ("sup", 8835),
    ("sup1", 185),
    ("sup2", 178),
    ("sup3", 179),
    ("supe", 8839),
    ("szlig", 223),
    ("tau", 964),
    ("there4", 8756),
    ("theta", 952),
    ("thetasym", 977),
    ("thinsp", 8201),
    ("thorn", 254),
    ("tilde", 732),
    ("times", 215),
    ("trade", 8482),
    ("uArr", 8657),
    ("uacute", 250),
    ("uarr", 8593),
    ("ucirc", 251),
    ("ugrave", 249),
    ("uml", 168),
    ("upsih", 978),
    ("upsilon", 965),
    ("uuml", 252),
    ("weierp", 8472),
    ("xi", 958),
    ("yacute", 253),
    ("yen", 165),
    ("yuml", 255),
    ("zeta", 950),
    ("zwj", 8205),
    ("zwnj", 8204),
];

fn literal_end(s: &str, start: usize) -> Option<usize> {
    let tail = &s[start..];
    for (prefix, suffix) in [("<!--", "-->"), ("<![CDATA[", "]]>"), ("<?", "?>")] {
        if tail.starts_with(prefix) {
            return Some(
                tail[prefix.len()..]
                    .find(suffix)
                    .map(|n| start + prefix.len() + n + suffix.len())
                    .unwrap_or(s.len()),
            );
        }
    }
    None
}

// 引号、注释里的 > / [ / ] 不参与判断，避免误认 DTD 的边界。
fn declaration_end(s: &str, start: usize) -> usize {
    let bytes = s.as_bytes();
    let mut i = start;
    let mut quote = None;
    let mut depth = 0usize;
    while i < bytes.len() {
        let c = bytes[i];
        if let Some(q) = quote {
            if c == q {
                quote = None;
            }
        } else if s[i..].starts_with("<!--") {
            i = literal_end(s, i).unwrap();
            continue;
        } else {
            match c {
                b'\'' | b'"' => quote = Some(c),
                b'[' => depth += 1,
                b']' => depth = depth.saturating_sub(1),
                b'>' if depth == 0 => return i + 1,
                _ => {}
            }
        }
        // 按字符推进，保证所有字符串切片都落在 UTF-8 边界上。
        i += s[i..].chars().next().unwrap().len_utf8();
    }
    s.len()
}

fn declared_entities(s: &str) -> HashSet<&str> {
    let mut names = HashSet::new();
    let mut i = 0;
    while i < s.len() {
        if let Some(end) = literal_end(s, i) {
            i = end;
        } else if s[i..].starts_with("<!DOCTYPE") {
            let end = declaration_end(s, i);
            let dtd = &s[i..end];
            let mut j = "<!DOCTYPE".len();
            let mut quote = None;
            let mut subset = false;
            while j < dtd.len() {
                let c = dtd.as_bytes()[j];
                if let Some(q) = quote {
                    if c == q {
                        quote = None;
                    }
                } else if let Some(end) = literal_end(dtd, j) {
                    j = end;
                    continue;
                } else if subset && dtd[j..].starts_with("<!ENTITY") {
                    let rest = &dtd[j + "<!ENTITY".len()..];
                    if rest.starts_with(char::is_whitespace) {
                        let rest = rest.trim_start();
                        // 参数实体不是正文中的 &name; 引用。
                        if !rest.starts_with('%') {
                            let name = rest.split_whitespace().next().unwrap_or("");
                            if !name.is_empty() {
                                names.insert(name);
                            }
                        }
                    }
                    j = declaration_end(dtd, j);
                    continue;
                } else {
                    match c {
                        b'\'' | b'"' => quote = Some(c),
                        b'[' => subset = true,
                        b']' => subset = false,
                        _ => {}
                    }
                }
                j += dtd[j..].chars().next().unwrap().len_utf8();
            }
            i = end;
        } else if s.as_bytes()[i] == b'<' {
            i = declaration_end(s, i);
        } else {
            i += s[i..].chars().next().unwrap().len_utf8();
        }
    }
    names
}

fn normalize_entities(s: &str) -> String {
    let declared = declared_entities(s);
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < s.len() {
        if let Some(end) = literal_end(s, i) {
            out.push_str(&s[i..end]);
            i = end;
            continue;
        }
        if s.as_bytes()[i] != b'&' {
            let c = s[i..].chars().next().unwrap();
            out.push(c);
            i += c.len_utf8();
            continue;
        }
        let start = i + 1;
        let end = s[start..]
            .find(|c: char| c.is_whitespace() || matches!(c, ';' | '&' | '<' | '>' | '\'' | '"'))
            .map(|n| start + n)
            .unwrap_or(s.len());
        if s.as_bytes().get(end) == Some(&b';') {
            let name = &s[start..end];
            if matches!(name, "amp" | "lt" | "gt" | "quot" | "apos") || declared.contains(name) {
                out.push_str(&s[i..=end]);
                i = end + 1;
                continue;
            }
            if let Some(number) = name.strip_prefix('#') {
                let (digits, radix) = number
                    .strip_prefix('x')
                    .map(|n| (n, 16))
                    .unwrap_or((number, 10));
                if !digits.is_empty() && digits.chars().all(|c| c.is_digit(radix)) {
                    let value = u32::from_str_radix(digits, radix).ok();
                    if value.is_some_and(|v| {
                        matches!(v, 9 | 10 | 13 | 0x20..=0xd7ff | 0xe000..=0xfffd | 0x10000..=0x10ffff)
                    }) {
                        out.push_str(&s[i..=end]);
                    } else {
                        out.push_str("&#65533;");
                    }
                    i = end + 1;
                    continue;
                }
            } else if let Ok(index) = XHTML_ENTITIES.binary_search_by_key(&name, |&(n, _)| n) {
                use std::fmt::Write;
                let _ = write!(out, "&#{};", XHTML_ENTITIES[index].1);
                i = end + 1;
                continue;
            }
        }
        // 未知引用与裸 & 保留显示原文，不让整章解析失败。
        out.push_str("&amp;");
        i += 1;
    }
    out
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
    let entities_fixture = book.title == "实体与编码自检样书";
    let mut fixture_text = String::new();
    for i in 0..book.chapters.len() {
        let title = book.chapters[i].title.clone();
        let p = book.chapter(i)?;
        if entities_fixture {
            for paragraph in p {
                fixture_text.push_str(&paragraph.text());
                fixture_text.push('\n');
            }
        }
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
    if entities_fixture {
        for expected in ENTITY_FIXTURE_EXPECTED {
            if !fixture_text.contains(expected) {
                return Err(format!("实体样书缺少期望正文：{expected}"));
            }
        }
        s.push_str("实体与编码正文校验通过\n");
    }
    s.push_str(&format!("解析耗时：{} ms\n", start.elapsed().as_millis()));
    Ok(s)
}

// 仅用于开发样书的 --selftest 正文断言，普通图书的输出保持原样。
const ENTITY_FIXTURE_EXPECTED: &[&str] = &[
    "命名：“”é\u{a0}‘’×·—–…©",
    "XML：& < > \" '",
    "数字：中 中",
    "裸引用：AT&T &foo; &#bad;",
    "CDATA：&rdquo; &foo; AT&T",
    "DTD：声明优先 自定义",
    "非法数字：� � � �",
    "UTF-16BE：中文 “é”",
    "UTF-16LE：中文 ‘é’",
    "损坏UTF-8：� 可继续阅读",
];

#[cfg(test)]
mod tests {
    use super::*;

    fn text(s: &str) -> String {
        let doc = parse(s).unwrap();
        doc.root_element()
            .descendants()
            .filter(|n| n.is_text())
            .filter_map(|n| n.text())
            .collect()
    }

    #[test]
    fn standard_entities_and_xml_references() {
        assert_eq!(XHTML_ENTITIES.len(), 252);
        assert!(XHTML_ENTITIES.windows(2).all(|w| w[0].0 < w[1].0));
        let s = normalize_entities(
            "<p>&rdquo;&ldquo;&eacute;&nbsp;&amp;&lt;&gt;&quot;&apos;&#123;&#x4E2D;</p>",
        );
        assert_eq!(
            s,
            "<p>&#8221;&#8220;&#233;&#160;&amp;&lt;&gt;&quot;&apos;&#123;&#x4E2D;</p>"
        );
        assert_eq!(text(&s), "”“é\u{a0}&<>\"'{中");
        for &(name, code) in XHTML_ENTITIES {
            let s = normalize_entities(&format!("<p>&{name};</p>"));
            assert_eq!(text(&s), char::from_u32(code).unwrap().to_string());
        }
    }

    #[test]
    fn unknown_and_bare_ampersands_are_literal() {
        let s = normalize_entities("<p a=\"AT&T &foo;\">AT&T &foo; &#bad; & &foo</p>");
        assert!(s.contains("AT&amp;T &amp;foo;"));
        assert_eq!(text(&s), "AT&T &foo; &#bad; & &foo");
        assert_eq!(
            parse(&s).unwrap().root_element().attribute("a"),
            Some("AT&T &foo;")
        );
    }

    #[test]
    fn literals_and_internal_dtd_are_preserved() {
        let s = "<!DOCTYPE p [<!-- <!ENTITY foo 'fake'> --> <!ENTITY eacute '声明优先'> <!ENTITY local '&eacute;'>]><p>&eacute;&local;&foo;<![CDATA[&rdquo; AT&T]]><!-- &ldquo; AT&T --></p>";
        let normalized = normalize_entities(s);
        assert!(normalized.contains("<p>&eacute;&local;&amp;foo;"));
        assert!(normalized.contains("<![CDATA[&rdquo; AT&T]]>"));
        assert!(normalized.contains("<!-- &ldquo; AT&T -->"));
        assert_eq!(text(&normalized), "声明优先声明优先&foo;&rdquo; AT&T");
        // 引号中的假声明不能影响正文中的实体转换。
        let s = "<!DOCTYPE p [<!ENTITY local \"<!ENTITY eacute 'fake'>\">]><p>&eacute;</p>";
        assert!(normalize_entities(s).ends_with("<p>&#233;</p>"));
        let s = "<!DOCTYPE p [<!ENTITY a· '自定义'>]><p>&a·;</p>";
        assert!(normalize_entities(s).contains("<p>&a·;</p>"));
        assert_eq!(text(&normalize_entities(s)), "自定义");
    }

    #[test]
    fn invalid_numeric_codepoints_are_replaced() {
        let s =
            normalize_entities("<p>&#0;&#xD800;&#1114112;&#99999999999999999999;&#1;&#xFFFF;</p>");
        assert_eq!(text(&s), "�".repeat(6));
    }

    #[test]
    fn bom_and_lossy_decoding() {
        let input = "<p>中文 &ldquo;é&rdquo;</p>";
        let expected = "中文 “é”";
        let mut utf8 = vec![0xef, 0xbb, 0xbf];
        utf8.extend_from_slice(input.as_bytes());
        assert_eq!(text(&xml(&utf8).unwrap()), expected);
        for (bom, little) in [([0xff, 0xfe], true), ([0xfe, 0xff], false)] {
            let mut bytes = bom.to_vec();
            for unit in input.encode_utf16() {
                bytes.extend_from_slice(&if little {
                    unit.to_le_bytes()
                } else {
                    unit.to_be_bytes()
                });
            }
            assert_eq!(text(&xml(&bytes).unwrap()), expected);
        }
        assert_eq!(text(&xml(b"<p>bad \xff text</p>").unwrap()), "bad � text");
        assert_eq!(xml(&[0xfe, 0xff, 0]).unwrap(), "�");
    }

    #[test]
    fn entity_fixture_body_selftest() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/entities.epub");
        assert!(selftest(&path).unwrap().contains("实体与编码正文校验通过"));
        let mut book = Book::open(&path).unwrap();
        assert_eq!(book.author, "开发自检 é");
        assert_eq!(book.toc[0].title, "“实体章0”");
        // nav 成功时 NCX 不会被选用，这里额外验证同样的解码与目录路径。
        let s = xml(&read_entry(&mut book.archive, "OPS/toc.ncx").unwrap()).unwrap();
        let doc = parse(&s).unwrap();
        let root = doc.descendants().find(|n| tag(*n, "navMap")).unwrap();
        let toc = ncx_list(root, "OPS/toc.ncx", &book.chapters);
        assert_eq!(toc.len(), 4);
        assert_eq!(toc[0].title, book.toc[0].title);
    }
}

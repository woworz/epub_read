use serde::{Deserialize, Serialize};

#[derive(Clone, Default)]
pub struct Run {
    pub text: String,
    pub bold: bool,
    pub italic: bool,
}
#[derive(Clone)]
pub enum Kind {
    Text,
    Heading(u8),
    Image(String),
    Rule,
}
#[derive(Clone)]
pub struct Paragraph {
    pub kind: Kind,
    pub runs: Vec<Run>,
    pub anchor: String,
}
impl Paragraph {
    pub fn text(&self) -> String {
        self.runs.iter().map(|r| r.text.as_str()).collect()
    }
    pub fn hash(&self) -> String {
        crate::epub::hash(self.text().as_bytes())
    }
}
#[derive(Clone)]
pub struct Chapter {
    pub id: String,
    pub path: String,
    pub title: String,
}
#[derive(Clone)]
pub struct Toc {
    pub title: String,
    pub chapter: usize,
    pub anchor: String,
    pub children: Vec<Toc>,
}
#[derive(Clone, Serialize, Deserialize, Default, Debug, PartialEq)]
#[serde(default)]
pub struct Location {
    pub chapter: usize,
    pub paragraph: usize,
    pub offset: f32,
}
#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
pub struct Mark {
    pub chapter_id: String,
    pub paragraph: usize,
    pub text_hash: String,
    pub text: String,
    pub color: usize,
    pub note: String,
}
#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
pub struct BookData {
    pub location: Location,
    pub marks: Vec<Mark>,
}

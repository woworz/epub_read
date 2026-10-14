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
}
#[derive(Clone)]
pub struct Chapter {
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
#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
pub struct BookData {
    pub location: Location,
}

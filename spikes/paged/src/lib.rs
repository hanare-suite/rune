//! Paged rich text in one `text_input`: a generated document of about 25,000 words that flows
//! into A4 pages, with a run mark for size and colour, styled paragraphs and a forced page
//! break. Throwaway: it exists to prove the field's page flow, not to be an editor.

use std::sync::Arc;

use rune::color::hex;
use rune::{Block, Color, Cx, El, Key, KeyEvent, Mark, MarkColor, MarkRange, MarkSize, PageFlow, ParagraphStyle, ScrollSpec, TextEdit, TextStyle, Tx, col, el, row, text, text_input};

/// A4 at 72 points an inch, with 2.5 cm margins.
pub const PAGE: (f32, f32) = (595.0, 842.0);
pub const MARGIN: f32 = 71.0;
/// The desk showing between two pages.
pub const DESK: f32 = 16.0;
pub const CONTENT: (f32, f32) = (PAGE.0 - 2.0 * MARGIN, PAGE.1 - 2.0 * MARGIN);
pub const WINDOW: (f32, f32) = (760.0, 1000.0);

pub const BODY: u16 = 0;
pub const HEADING: u16 = 1;
/// A chapter heading: it starts a new page.
pub const CHAPTER: u16 = 2;
pub const TITLE: u16 = 3;

/// Sections a chapter holds.
const SECTIONS: usize = 14;

pub fn flow() -> PageFlow {
    PageFlow::new(CONTENT.1, 2.0 * MARGIN + DESK)
}

pub fn body() -> TextStyle {
    TextStyle::new(10.5, 400.0).lh(14.0)
}

pub fn styles() -> Vec<ParagraphStyle> {
    vec![
        ParagraphStyle::new(body()).space(0.0, 6.0),
        ParagraphStyle::new(TextStyle::new(14.0, 600.0).lh(20.0)).space(10.0, 4.0),
        ParagraphStyle::new(TextStyle::new(18.0, 700.0).lh(24.0)).space(0.0, 10.0).page_break_before(),
        ParagraphStyle::new(TextStyle::new(26.0, 700.0).lh(32.0)).space(0.0, 14.0),
    ]
}

/// One paragraph: its text, its style, and its marks in bytes of its own text.
#[derive(Clone, Debug, PartialEq)]
pub struct Para {
    pub text: String,
    pub style: u16,
    pub marks: Vec<(usize, usize, Mark)>,
    pub words: usize,
}

impl Para {
    fn new(text: String, style: u16, marks: Vec<(usize, usize, Mark)>) -> Para {
        let words = text.split_whitespace().count();
        Para { text, style, marks, words }
    }
}

/// The document: paragraphs two documents can share, so a history step that holds a whole
/// document costs the paragraphs its edit changed and one pointer for each of the others.
#[derive(Clone, Default)]
pub struct Doc {
    pub paras: Vec<Arc<Para>>,
}

const WORDS: &[&str] = &[
    "Hafen", "Morgen", "Brücke", "leise", "Fenster", "Garten", "über", "der", "die", "das", "und", "mit", "einem", "Zug", "Regen", "Straße",
    "langsam", "Lampe", "Tisch", "Brief", "schreibt", "liest", "Papier", "Tinte", "Feder", "zwischen", "den", "alten", "Häusern", "wartet",
    "Nachbarin", "Markt", "Glocke", "Turm", "Schiff", "Kapitän", "Karte", "Küste", "Wind", "nach", "Norden", "Stunde", "Uhr", "schlägt",
    "sieben", "Kaffee", "Bäcker", "Brot", "warm", "Kinder", "laufen", "Schule", "Lehrerin", "Kreide", "Tafel", "Rechnung", "Zahl", "Ordner",
    "Amt", "Stempel", "Antrag", "Frist", "Montag", "Dienstag", "Winter", "Schnee", "Dach", "Rauch", "Ofen", "Holz", "Wald", "Weg", "Fluss",
    "Ufer", "Stein", "Moos", "Vogel", "ruft", "weit", "hinter", "dem", "Hügel", "Dorf", "Kirche", "Platz", "Brunnen", "Wasser", "klar",
    "kalt", "Abend", "Licht", "geht", "an", "in", "einer", "Küche", "Suppe", "Löffel", "Teller", "Gespräch", "Frage", "Antwort", "vielleicht",
    "später", "noch", "nicht", "immer", "wieder", "kommt", "bleibt", "sagt", "hört", "sieht", "kleine", "große", "neue", "gelbe", "stille",
];

struct Rng(u64);

impl Rng {
    fn next(&mut self, n: usize) -> usize {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 % n as u64) as usize
    }

    /// A sentence of `n` words, and where each word starts.
    fn sentence(&mut self, n: usize, out: &mut String, starts: &mut Vec<usize>) {
        for i in 0..n {
            if !out.is_empty() { out.push(' '); }
            starts.push(out.len());
            let w = WORDS[self.next(WORDS.len())];
            if i == 0 {
                let mut c = w.chars();
                if let Some(f) = c.next() { out.extend(f.to_uppercase()); out.push_str(c.as_str()); }
            } else {
                out.push_str(w);
            }
        }
        out.push('.');
    }
}

/// The same document every time: a title, then chapters of headed sections, about `words`
/// words in all. Every chapter after the first starts a page.
pub fn generate(words: usize) -> Doc {
    let mut rng = Rng(0x9e37_79b9_7f4a_7c15);
    let mut paras = vec![Arc::new(Para::new("Der Hafen am Morgen".into(), TITLE, Vec::new()))];
    let (mut count, mut chapter, mut body_no) = (4usize, 0usize, 0usize);
    while count < words {
        chapter += 1;
        let (mut t, mut s) = (format!("Kapitel {chapter}:"), Vec::new());
        rng.sentence(3, &mut t, &mut s);
        t.pop();
        count += 5;
        paras.push(Arc::new(Para::new(t, if chapter == 1 { HEADING } else { CHAPTER }, Vec::new())));
        for section in 0..SECTIONS {
            if count >= words { break; }
            if section > 0 {
                let (mut t, mut s) = (String::new(), Vec::new());
                let n = 2 + rng.next(3);
                rng.sentence(n, &mut t, &mut s);
                t.pop();
                count += s.len();
                paras.push(Arc::new(Para::new(t, HEADING, Vec::new())));
            }
            for _ in 0..4 + rng.next(3) {
                let (mut t, mut starts) = (String::new(), Vec::new());
                for _ in 0..3 + rng.next(5) {
                    let n = 6 + rng.next(14);
                    rng.sentence(n, &mut t, &mut starts);
                }
                count += starts.len();
                body_no += 1;
                // Runs of a few words: bold, a colour, a larger size.
                let run = |from: usize, len: usize| (starts[from.min(starts.len() - 1)], starts.get(from + len).map_or(t.len(), |&e| e - 1));
                let mut marks = Vec::new();
                if body_no % 3 == 0 { let (a, b) = run(2, 3); marks.push((a, b, Mark::Bold)); }
                if body_no % 4 == 1 { let (a, b) = run(9, 4); marks.push((a, b, Mark::Color(MarkColor::new(hex("#b3261e").unwrap())))); }
                if body_no % 5 == 2 { let (a, b) = run(14, 2); marks.push((a, b, Mark::Size(MarkSize::pt(15.0)))); }
                if body_no % 7 == 3 { let (a, b) = run(5, 2); marks.push((a, b, Mark::Italic)); }
                marks.retain(|m| m.0 < m.1);
                paras.push(Arc::new(Para::new(t, BODY, marks)));
            }
        }
    }
    Doc { paras }
}

impl Doc {
    pub fn words(&self) -> usize {
        self.paras.iter().map(|p| p.words).sum()
    }

    /// The field's value for this document, the caret at `caret`.
    pub fn to_edit(&self, caret: usize) -> TextEdit {
        let mut out = String::with_capacity(self.paras.iter().map(|p| p.text.len() + 1).sum());
        let (mut marks, mut blocks) = (Vec::new(), Vec::with_capacity(self.paras.len()));
        for (i, p) in self.paras.iter().enumerate() {
            if i > 0 { out.push('\n'); }
            let at = out.len();
            out.push_str(&p.text);
            marks.extend(p.marks.iter().map(|(a, b, m)| MarkRange::new(at + a..at + b, m.clone())));
            blocks.push(Block::Styled(p.style));
        }
        let mut edit = TextEdit::from_parts(out, marks, blocks);
        let caret = (0..=caret.min(edit.text.len())).rev().find(|&i| edit.text.is_char_boundary(i)).unwrap_or(0);
        edit.set_caret(caret);
        edit
    }

    /// The document `edit` holds, sharing every paragraph of `self` the edit left as it was:
    /// those before its first changed paragraph and after its last.
    pub fn after(&self, edit: &TextEdit) -> Doc {
        let text = edit.text.as_str();
        let style = |i: usize| match edit.blocks().get(i) { Some(Block::Styled(n)) => *n, _ => BODY };
        let lines: Vec<&str> = text.split('\n').collect();
        let same = |line: &str, i: usize, old: &Para| old.text == line && old.style == style(i);
        let head = lines.iter().zip(&self.paras).enumerate().take_while(|(i, (l, o))| same(l, *i, o)).count();
        let room = lines.len().min(self.paras.len()) - head;
        let tail = (0..room).take_while(|k| same(lines[lines.len() - 1 - k], lines.len() - 1 - k, &self.paras[self.paras.len() - 1 - k])).count();
        let mut paras = Vec::with_capacity(lines.len());
        paras.extend_from_slice(&self.paras[..head]);
        let mut at: usize = lines[..head].iter().map(|l| l.len() + 1).sum();
        for (i, line) in lines.iter().enumerate().take(lines.len() - tail).skip(head) {
            let (a, b) = (at, at + line.len());
            let marks = edit.marks().iter().filter(|m| m.start < b && a < m.end)
                .map(|m| (m.start.max(a) - a, m.end.min(b) - a, m.mark.clone())).collect();
            paras.push(Arc::new(Para::new((*line).to_owned(), style(i), marks)));
            at = b + 1;
        }
        paras.extend_from_slice(&self.paras[self.paras.len() - tail..]);
        Doc { paras }
    }
}

/// One undo model for the whole document, as the suite's shared history is: each step holds
/// the document after it.
pub struct History {
    steps: Vec<Doc>,
    at: usize,
}

impl History {
    pub fn new(doc: Doc) -> History {
        History { steps: vec![doc], at: 0 }
    }

    pub fn push(&mut self, doc: Doc) {
        self.steps.truncate(self.at + 1);
        self.steps.push(doc);
        self.at += 1;
    }

    pub fn undo(&mut self) -> Option<&Doc> {
        self.at = self.at.checked_sub(1)?;
        Some(&self.steps[self.at])
    }

    pub fn redo(&mut self) -> Option<&Doc> {
        if self.at + 1 >= self.steps.len() { return None; }
        self.at += 1;
        Some(&self.steps[self.at])
    }

    pub fn len(&self) -> usize {
        self.steps.len()
    }

    pub fn is_empty(&self) -> bool {
        self.steps.is_empty()
    }
}

pub struct State {
    pub edit: TextEdit,
    pub doc: Doc,
    pub history: History,
    pub words: usize,
}

impl State {
    pub fn new(doc: Doc) -> State {
        let words = doc.words();
        State { edit: doc.to_edit(0), history: History::new(doc.clone()), doc, words }
    }

    /// After the field changed: the document follows it, as one history step.
    pub fn changed(&mut self) {
        let doc = self.doc.after(&self.edit);
        self.words = doc.words();
        self.history.push(doc.clone());
        self.doc = doc;
    }

    fn restore(&mut self, doc: Doc) {
        let mut edit = doc.to_edit(self.edit.focus);
        edit.focused = self.edit.focused;
        self.words = doc.words();
        (self.edit, self.doc) = (edit, doc);
    }

    /// Cmd+Z and Shift+Cmd+Z go to the document's history, not the field's own.
    pub fn key(&mut self, k: &KeyEvent) -> bool {
        if k.key != Key::Command('z') { return false; }
        let doc = if k.mods.shift { self.history.redo().cloned() } else { self.history.undo().cloned() };
        if let Some(doc) = doc { self.restore(doc); }
        true
    }
}

fn c(s: &str) -> Color {
    hex(s).unwrap_or(Color::WHITE)
}

pub fn view(s: &State, cx: &mut Cx<State>) -> El {
    let flow = flow();
    // The field is whole pages tall: its height gives the page count the sheets are drawn for.
    let pages = cx.layout_size("editor").map_or(1, |size| flow.pages(size[1]));
    let field = text_input(&s.edit, body()).key("editor").w(CONTENT.0)
        .paragraph_styles(styles()).page_flow(flow)
        .color(c("#1b1b1f")).caret(c("#1b1b1f"), c("#2f6fed").alpha(0.32))
        .absolute(MARGIN, MARGIN)
        .on_edit(|s: &mut State| &mut s.edit)
        .on_input_key(|s: &mut State, k, _: &mut Tx| s.key(k))
        .on_input_change(|s: &mut State, _: &mut Tx| s.changed());
    let sheets = (0..pages).map(|i| {
        el().key(format!("page{}", i + 1)).size(PAGE.0, PAGE.1).fill(Color::WHITE).absolute(0.0, i as f32 * (PAGE.1 + DESK))
    });
    let paper = el().key("paper").size(PAGE.0, pages as f32 * (PAGE.1 + DESK) - DESK).children(sheets).child(field);
    col().w_full().h_full().fill(c("#e7e5e0"))
        .child(col().key("viewport").w_full().grow(1.0).min_h(0.0).scroll(ScrollSpec { stick_bottom: false })
            .child(col().w_full().py(24.0).items_center().child(paper)))
        .child(row().w_full().px(12.0).py(6.0).fill(c("#f6f5f2"))
            .child(text(format!("{pages} Seiten · {} Wörter", s.words)).key("status").font(TextStyle::new(12.0, 400.0)).color(c("#44444a"))))
}

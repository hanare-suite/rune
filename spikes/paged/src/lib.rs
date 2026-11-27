//! Paged rich text in one `text_input`: the fork's own check of what a word processor needs
//! from the field. A generated document of about 25,000 words flows into A4 pages; smaller
//! documents (`docs`) hold the run marks, paragraph attributes, page rules and locked atoms the
//! checks look at. The field's history is off: the document's own history owns undo, and
//! follows the field through the changes it reports.

use std::sync::Arc;

use rune::color::hex;
use rune::{
    Baseline, Block, Color, Cx, EditChange, EditChangeKind, El, Key, KeyEvent, Mark, MarkColor, MarkRange, PageFlow, ParagraphAlign, ParagraphAttrs,
    ParagraphInfo, ParagraphStyle, ScrollSpec, TextEdit, TextStyle, Tx, col, el, row, text, text_input,
};

pub mod docs;
pub use docs::generate;

/// A4 at 72 points an inch, with 2.5 cm margins.
pub const PAGE: (f32, f32) = (595.0, 842.0);
pub const MARGIN: f32 = 71.0;
/// The desk showing between two pages.
pub const DESK: f32 = 16.0;
pub const CONTENT: (f32, f32) = (PAGE.0 - 2.0 * MARGIN, PAGE.1 - 2.0 * MARGIN);
pub const WINDOW: (f32, f32) = (760.0, 1000.0);

pub const BODY: u16 = 0;
pub const HEADING: u16 = 1;
pub const CHAPTER: u16 = 2;
pub const TITLE: u16 = 3;
/// Body text with no room around it, so a page holds exactly 50 of its lines.
pub const TIGHT: u16 = 4;

/// The family a run takes from F5, and the font the app registers for it (`AppSpec::fonts`),
/// as Fude registers the fonts it bundles.
pub const OTHER_FAMILY: &str = "IBM Plex Mono";
pub const FONTS: &[&[u8]] = &[
    include_bytes!("../../../crates/rune/assets/Inter-Regular.ttf"),
    include_bytes!("../../../crates/rune/assets/Inter-Medium.ttf"),
    include_bytes!("../../../crates/rune/assets/Inter-SemiBold.ttf"),
    include_bytes!("../../../crates/rune/assets/IBMPlexMono-Regular.ttf"),
];

/// Widow and orphan control, which `main` turns off for the checks that show what it changes.
pub static KEEP_LINES: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(true);

pub fn flow() -> PageFlow {
    PageFlow::new(CONTENT.1, 2.0 * MARGIN + DESK).keep_lines(KEEP_LINES.load(std::sync::atomic::Ordering::Relaxed))
}

pub fn body() -> TextStyle {
    TextStyle::new(10.5, 400.0).lh(14.0)
}

pub fn styles() -> Vec<ParagraphStyle> {
    vec![
        ParagraphStyle::new(body()).space(0.0, 6.0),
        ParagraphStyle::new(TextStyle::new(14.0, 600.0).lh(20.0)).space(10.0, 4.0),
        ParagraphStyle::new(TextStyle::new(18.0, 700.0).lh(24.0)).space(0.0, 10.0),
        ParagraphStyle::new(TextStyle::new(26.0, 700.0).lh(32.0)).space(0.0, 14.0),
        ParagraphStyle::new(body()),
    ]
}

/// One paragraph: its text, its block, its attributes (its id among them), and its marks in
/// bytes of its own text.
#[derive(Clone, Debug, PartialEq)]
pub struct Para {
    pub text: String,
    pub block: Block,
    pub attrs: ParagraphAttrs,
    pub marks: Vec<(usize, usize, Mark)>,
    pub words: usize,
}

impl Para {
    pub fn new(text: impl Into<String>, style: u16) -> Para {
        let text = text.into();
        let words = text.split_whitespace().count();
        Para { text, block: Block::Paragraph, attrs: ParagraphAttrs::default().style(style), marks: Vec::new(), words }
    }

    pub fn attrs(mut self, f: impl FnOnce(ParagraphAttrs) -> ParagraphAttrs) -> Para {
        self.attrs = f(self.attrs);
        self
    }

    pub fn block(mut self, block: Block) -> Para {
        self.block = block;
        self
    }

    /// The field's paragraph `info`.
    pub fn read(edit: &TextEdit, info: &ParagraphInfo) -> Para {
        let (s, e) = (info.range.start, info.range.end);
        let marks = edit.marks().iter().filter(|m| m.start < e && s < m.end).map(|m| (m.start.max(s) - s, m.end.min(e) - s, m.mark.clone())).collect();
        let line = &edit.text[s..e];
        Para { text: line.to_owned(), block: info.block, attrs: info.attrs, marks, words: line.split_whitespace().count() }
    }

    /// Marks the first place `word` occurs.
    pub fn mark(self, word: &str, mark: Mark) -> Para {
        self.mark_at(word, word.len(), mark)
    }

    /// Marks the first `len` bytes of the first place `needle` occurs.
    pub fn mark_at(mut self, needle: &str, len: usize, mark: Mark) -> Para {
        if let Some(at) = self.text.find(needle) {
            self.marks.push((at, at + len, mark));
        }
        self
    }
}

/// The document: paragraphs two documents can share, so a history step that holds a whole
/// document costs the paragraphs its edit changed and one pointer for each of the others.
#[derive(Clone, Default)]
pub struct Doc {
    pub paras: Vec<Arc<Para>>,
}

impl Doc {
    /// `paras` with ids from 1 in order.
    pub fn of(paras: Vec<Para>) -> Doc {
        Doc { paras: paras.into_iter().enumerate().map(|(i, p)| Arc::new(p.attrs(|a| a.id(i as u64 + 1)))).collect() }
    }

    pub fn words(&self) -> usize {
        self.paras.iter().map(|p| p.words).sum()
    }

    /// The field's value for this document, the caret at `caret`: its history off, and new
    /// paragraphs numbered from `next_id`.
    pub fn to_edit(&self, caret: usize, next_id: u64) -> TextEdit {
        let mut out = String::with_capacity(self.paras.iter().map(|p| p.text.len() + 1).sum());
        let (mut marks, mut blocks, mut attrs) = (Vec::new(), Vec::with_capacity(self.paras.len()), Vec::with_capacity(self.paras.len()));
        for (i, p) in self.paras.iter().enumerate() {
            if i > 0 { out.push('\n'); }
            let at = out.len();
            out.push_str(&p.text);
            marks.extend(p.marks.iter().map(|(a, b, m)| MarkRange::new(at + a..at + b, m.clone())));
            blocks.push(p.block);
            attrs.push(p.attrs);
        }
        let mut edit = TextEdit::from_parts(out, marks, blocks);
        edit.set_paragraph_attrs(attrs);
        edit.set_paragraph_ids(next_id);
        edit.set_history(false);
        let caret = (0..=caret.min(edit.text.len())).rev().find(|&i| edit.text.is_char_boundary(i)).unwrap_or(0);
        edit.set_caret(caret);
        edit
    }

    /// The document after `changes`, which `edit` reported and already holds: each change's
    /// paragraphs are read again from the field, and every other paragraph is shared with
    /// `self`. A change of marks or attributes alone names the bytes it touched just as a
    /// change of text does.
    pub fn after(&self, edit: &TextEdit, changes: &[EditChange]) -> Doc {
        // The changes' union: bytes `a..old_end` of the text before the first became bytes
        // `a..new_end` of the text after the last. Each change's range is in the text the ones
        // before it left.
        let mut union = None::<(usize, usize, usize)>;
        for c in changes {
            let (a, mut old_end, mut new_end) = union.unwrap_or((c.range.start, c.range.start, c.range.start));
            if c.range.end > new_end {
                old_end += c.range.end - new_end;
                new_end = c.range.end;
            }
            union = Some((a.min(c.range.start), old_end, new_end + c.len - c.range.len()));
        }
        let Some((a, b, _)) = union else { return self.clone() };
        // The paragraphs of `self` those bytes touch, and the bytes before them.
        let (mut first, mut at) = (0, 0);
        let mut last = self.paras.len().saturating_sub(1);
        for (i, p) in self.paras.iter().enumerate() {
            let end = at + p.text.len();
            if at <= a { first = i; }
            if end >= b { last = i; break; }
            at = end + 1;
        }
        let tail = self.paras.len() - 1 - last;
        let infos = edit.paragraphs();
        let lines = infos.len();
        let mut paras = Vec::with_capacity(lines);
        paras.extend_from_slice(&self.paras[..first]);
        for info in &infos[first.min(lines)..lines.saturating_sub(tail).max(first.min(lines))] {
            paras.push(Arc::new(Para::read(edit, info)));
        }
        paras.extend_from_slice(&self.paras[self.paras.len() - tail..]);
        Doc { paras }
    }

    /// Whether the document holds what the field does, paragraph for paragraph: the changes
    /// it followed left nothing out.
    pub fn matches(&self, edit: &TextEdit) -> bool {
        let infos = edit.paragraphs();
        infos.len() == self.paras.len() && infos.iter().zip(&self.paras).all(|(info, p)| Para::read(edit, info) == **p)
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
    /// How many events changed the document, and what the last one reported.
    pub events: usize,
    pub last: String,
    /// Shows the line of facts the check scripts read (`PAGED_FACTS`).
    pub facts: bool,
}

impl State {
    pub fn new(doc: Doc) -> State {
        let (words, next_id) = (doc.words(), doc.paras.len() as u64 + 1);
        State { edit: doc.to_edit(0, next_id), history: History::new(doc.clone()), doc, words, events: 0, last: String::new(), facts: false }
    }

    /// After the field changed: the document follows it through the changes it reported, as
    /// one history step.
    pub fn changed(&mut self, changes: &[EditChange]) {
        if changes.is_empty() { return; }
        let doc = self.doc.after(&self.edit, changes);
        self.words = doc.words();
        self.history.push(doc.clone());
        self.doc = doc;
        self.events += 1;
        let kind = |k: EditChangeKind| match k {
            EditChangeKind::Typed => "typed",
            EditChangeKind::LineBreak => "break",
            EditChangeKind::Deleted => "deleted",
            EditChangeKind::Replaced => "replaced",
            EditChangeKind::Marks => "marks",
            EditChangeKind::Paragraphs => "paragraphs",
            _ => "other",
        };
        self.last = changes.iter().map(|c| format!("{}:{}..{}>{}", kind(c.kind), c.range.start, c.range.end, c.len)).collect::<Vec<_>>().join(",");
    }

    fn restore(&mut self, doc: Doc) {
        let mut edit = doc.to_edit(self.edit.focus, self.edit.next_paragraph_id());
        edit.focused = self.edit.focused;
        self.words = doc.words();
        (self.edit, self.doc) = (edit, doc);
    }

    /// The function keys stand in for a ribbon: each formats the selection.
    pub fn key(&mut self, k: &KeyEvent) -> bool {
        let e = &mut self.edit;
        let yellow = MarkColor::new(hex("#ffe14d").unwrap_or(Color::WHITE));
        match k.key {
            Key::Function(1) => e.format_paragraphs(|a| a.align = ParagraphAlign::Center),
            Key::Function(2) => e.format_paragraphs(|a| a.align = ParagraphAlign::Right),
            Key::Function(3) => e.format_paragraphs(|a| a.align = ParagraphAlign::Justified),
            Key::Function(4) => e.format_paragraphs(|a| a.align = ParagraphAlign::Left),
            Key::Function(5) => e.toggle_mark(Mark::family(OTHER_FAMILY)),
            Key::Function(6) => e.toggle_mark(Mark::Highlight(yellow)),
            Key::Function(7) => e.toggle_mark(Mark::Baseline(Baseline::Super)),
            Key::Function(8) => e.toggle_mark(Mark::Baseline(Baseline::Sub)),
            Key::Function(9) => e.format_paragraphs(|a| (a.indent, a.first_line_indent) = (36.0, 18.0)),
            Key::Function(10) => e.format_paragraphs(|a| a.page_break_before = !a.page_break_before),
            Key::Function(11) => e.toggle_block(Block::Bullet(0)),
            Key::Function(12) => e.insert_atom(docs::ATOM, docs::ATOM_HEIGHT),
            _ => return false,
        }
        let changes = self.edit.take_changes();
        self.changed(&changes);
        true
    }

    /// Undo and redo, from the document's history.
    fn undo(&mut self, redo: bool) {
        let doc = if redo { self.history.redo().cloned() } else { self.history.undo().cloned() };
        if let Some(doc) = doc { self.restore(doc); }
    }

    /// What the check scripts read of the paragraph at the caret and of the document.
    fn facts(&self) -> String {
        let a = self.edit.active_paragraph();
        let line = self.edit.selected_lines().start;
        let block = match self.edit.active_block() { Block::Bullet(l) => format!("bullet{l}"), Block::Numbered(l) => format!("numbered{l}"), _ => "paragraph".into() };
        let marks: Vec<String> = self.edit.active_marks().iter().map(|m| match m {
            Mark::Bold => "bold".into(),
            Mark::Family(f) => format!("family={f}"),
            Mark::Highlight(_) => "highlight".into(),
            Mark::Baseline(Baseline::Sub) => "sub".into(),
            Mark::Baseline(Baseline::Super) => "super".into(),
            _ => "mark".into(),
        }).collect();
        let (undo, redo) = self.edit.history_len();
        format!(
            "line={line} id={} style={} align={:?} indent={}/{} break={} atom={} block={block} marks=[{}] | paras={} next_id={} field_history={undo}/{redo} steps={} events={} last={}{}",
            a.id, a.style, a.align, a.indent, a.first_line_indent, a.page_break_before, a.atom.is_some(), marks.join(","),
            self.doc.paras.len(), self.edit.next_paragraph_id(), self.history.len(), self.events, self.last, self.paragraph_facts(),
        )
    }

    /// Every paragraph of a short document: its id, whether it is an atom, its block, the bold
    /// bytes, and whether the document followed the field through every change (`sync`).
    fn paragraph_facts(&self) -> String {
        let infos = self.edit.paragraphs();
        if infos.len() > 40 { return String::new(); }
        let list = |f: &dyn Fn(&ParagraphInfo) -> String| infos.iter().map(f).collect::<Vec<_>>().join(",");
        let bold: Vec<String> = self.edit.marks().iter().filter(|m| m.mark == Mark::Bold).map(|m| format!("{}..{}", m.start, m.end)).collect();
        format!(
            " ids=[{}] atoms=[{}] blocks=[{}] bold=[{}] sync={}",
            list(&|p| p.attrs.id.to_string()), list(&|p| u8::from(p.attrs.atom.is_some()).to_string()),
            list(&|p| match p.block { Block::Bullet(_) => "b".into(), Block::Paragraph => "p".into(), _ => "o".into() }),
            bold.join(","), self.doc.matches(&self.edit),
        )
    }
}

/// The app's own key handler. The field's history is off, so the field leaves Cmd+Z and
/// Shift+Cmd+Z to it, and they reach the document's history here.
pub fn app_key(s: &mut State, k: &KeyEvent, _: &mut Tx) -> bool {
    if k.key != Key::Command('z') { return false; }
    s.undo(k.mods.shift);
    true
}

fn c(s: &str) -> Color {
    hex(s).unwrap_or(Color::WHITE)
}

pub fn view(s: &State, cx: &mut Cx<State>) -> El {
    // The page count of the layout this very commit makes: the sheets and the count are never
    // a frame behind the text.
    let pages = cx.pages("editor").unwrap_or(1);
    let field = text_input(&s.edit, body()).key("editor").w(CONTENT.0)
        .paragraph_styles(styles()).page_flow(flow())
        .color(c("#1b1b1f")).caret(c("#1b1b1f"), c("#2f6fed").alpha(0.32))
        .absolute(MARGIN, MARGIN)
        .on_edit(|s: &mut State| &mut s.edit)
        .on_input_key(|s: &mut State, k, _: &mut Tx| s.key(k))
        .on_input_edits(|s: &mut State, changes, _: &mut Tx| s.changed(changes));
    let sheets = (0..pages).map(|i| {
        el().key(format!("page{}", i + 1)).size(PAGE.0, PAGE.1).fill(Color::WHITE).absolute(0.0, flow().page_top(i))
    });
    let paper = el().key("paper").size(PAGE.0, pages as f32 * (PAGE.1 + DESK) - DESK).children(sheets).child(field);
    let small = TextStyle::new(12.0, 400.0);
    col().w_full().h_full().fill(c("#e7e5e0"))
        .child(col().key("viewport").w_full().grow(1.0).min_h(0.0).scroll(ScrollSpec { stick_bottom: false })
            .child(col().w_full().py(24.0).items_center().child(paper)))
        .child(col().w_full().px(12.0).py(6.0).fill(c("#f6f5f2"))
            .child(row().child(text(format!("{pages} Seiten · {} Wörter", s.words)).key("status").font(small).color(c("#44444a"))))
            .children(s.facts.then(|| text(s.facts()).key("facts").font(TextStyle::new(10.0, 400.0)).color(c("#6b6b72")).w_full())))
}

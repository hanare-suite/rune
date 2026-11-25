//! Paragraph-level block attributes for `\n`-delimited lines.

use rune_core::tok::TextStyle;

/// Maximum nesting level (`0..=MAX_LIST_LEVEL`) for [`Block::Bullet`] and [`Block::Numbered`].
pub const MAX_LIST_LEVEL: u8 = 3;

/// Paragraph-level block attribute for a `\n`-delimited line.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Block {
    #[default]
    Paragraph,
    /// Bullet list item (`•`, `◦`, `▪`) at nesting level `0..=MAX_LIST_LEVEL`.
    Bullet(u8),
    /// Numbered list item (`1.`, `2.`, …) at nesting level `0..=MAX_LIST_LEVEL`.
    Numbered(u8),
    /// Indented quote paragraph with a vertical left bar.
    Quote,
    /// Monospace code block line with a background fill; inline marks do not apply.
    Code,
}

/// A paragraph's text style and the room around it. A field's paragraphs take the entry their
/// [`ParagraphAttrs::style`] names in `El::paragraph_styles`. More fields may come, so build one
/// from [`ParagraphStyle::new`].
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub struct ParagraphStyle {
    pub text: TextStyle,
    /// Points above the paragraph, dropped at the top of a page its lines flowed onto.
    pub space_before: f32,
    /// Points below it.
    pub space_after: f32,
}

impl std::hash::Hash for ParagraphStyle {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        (self.text, self.space_before.to_bits(), self.space_after.to_bits()).hash(state);
    }
}

impl ParagraphStyle {
    pub fn new(text: TextStyle) -> Self {
        Self { text, space_before: 0.0, space_after: 0.0 }
    }

    pub fn space(mut self, before: f32, after: f32) -> Self {
        (self.space_before, self.space_after) = (before, after);
        self
    }
}

/// Where a paragraph's lines sit between its indents.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum ParagraphAlign {
    #[default]
    Left,
    Center,
    Right,
    /// Every line but the last fills the width.
    Justified,
}

/// A paragraph's attributes: one per `\n`-delimited line of a field's text, beside its
/// [`Block`]. More fields may come, so build one from `ParagraphAttrs::default()` and the
/// methods here. It compares and hashes by its bits.
#[derive(Clone, Copy, Debug, Default)]
#[non_exhaustive]
pub struct ParagraphAttrs {
    /// Its entry in the field's `El::paragraph_styles`; the field's own style when there is no
    /// such entry.
    pub style: u16,
    pub align: ParagraphAlign,
    /// Points every line starts in from the left.
    pub indent: f32,
    /// Points the first line starts in from the others: negative hangs it out.
    pub first_line_indent: f32,
    /// Starts the paragraph on a new page of a field with `El::page_flow`.
    pub page_break_before: bool,
    /// A locked atom this many points tall: the paragraph's text is its label, the caret steps
    /// over it as one character, a selection covers it whole and deleting it is one edit.
    pub atom: Option<f32>,
    /// The app's own handle on the paragraph. A split gives the new paragraph the next id of
    /// `TextEdit::set_paragraph_ids`; 0 without one.
    pub id: u64,
}

impl ParagraphAttrs {
    pub fn style(mut self, style: u16) -> Self {
        self.style = style;
        self
    }

    pub fn align(mut self, align: ParagraphAlign) -> Self {
        self.align = align;
        self
    }

    /// The left indent, and the first line's on top of it.
    pub fn indent(mut self, left: f32, first_line: f32) -> Self {
        (self.indent, self.first_line_indent) = (left, first_line);
        self
    }

    pub fn page_break_before(mut self, on: bool) -> Self {
        self.page_break_before = on;
        self
    }

    /// A locked atom `height` points tall.
    pub fn atom(mut self, height: f32) -> Self {
        self.atom = Some(height);
        self
    }

    pub fn id(mut self, id: u64) -> Self {
        self.id = id;
        self
    }

    fn bits(&self) -> (u16, ParagraphAlign, u32, u32, bool, Option<u32>, u64) {
        (self.style, self.align, self.indent.to_bits(), self.first_line_indent.to_bits(), self.page_break_before, self.atom.map(f32::to_bits), self.id)
    }

    /// What the paragraph's own layout depends on: not its page break or id, which only place
    /// it or name it.
    #[doc(hidden)]
    pub fn layout_bits(&self) -> (u16, ParagraphAlign, u32, u32, bool) {
        (self.style, self.align, self.indent.to_bits(), self.first_line_indent.to_bits(), self.atom.is_some())
    }

    /// What the field's layout depends on: everything but the id.
    #[doc(hidden)]
    pub fn flow_bits(&self) -> impl std::hash::Hash {
        (self.layout_bits(), self.page_break_before, self.atom.map(f32::to_bits))
    }

    /// Finite numbers only, an atom at least a point tall.
    #[doc(hidden)]
    pub fn clean(mut self) -> Self {
        let finite = |v: f32| if v.is_finite() { v } else { 0.0 };
        self.indent = finite(self.indent).max(0.0);
        self.first_line_indent = finite(self.first_line_indent);
        self.atom = self.atom.map(|h| finite(h).max(1.0));
        self
    }
}

impl PartialEq for ParagraphAttrs {
    fn eq(&self, other: &Self) -> bool {
        self.bits() == other.bits()
    }
}

impl Eq for ParagraphAttrs {}

impl std::hash::Hash for ParagraphAttrs {
    fn hash<H: std::hash::Hasher>(&self, h: &mut H) {
        self.bits().hash(h);
    }
}

/// Lays a field's lines into pages `height` points tall with `gap` points between them
/// (`El::page_flow`).
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub struct PageFlow {
    pub height: f32,
    pub gap: f32,
    /// Keeps a paragraph's first two lines together and its last two (widow and orphan
    /// control), moving a line to the next page rather than leaving one alone. On by default.
    pub keep_lines: bool,
}

impl PageFlow {
    pub fn new(height: f32, gap: f32) -> Self {
        Self { height, gap, keep_lines: true }
    }

    /// Sets [`Self::keep_lines`].
    pub fn keep_lines(mut self, on: bool) -> Self {
        self.keep_lines = on;
        self
    }

    /// How many pages a field `field_height` points tall holds: its height is always whole
    /// pages and the gaps between them.
    pub fn pages(self, field_height: f32) -> usize {
        (((field_height + self.gap) / (self.height + self.gap).max(1.0)).round() as usize).max(1)
    }

    /// The top of page `page` (from 0) in points from the field's top.
    pub fn page_top(self, page: usize) -> f32 {
        page as f32 * (self.height + self.gap)
    }
}

impl Block {
    pub fn is_list(self) -> bool {
        matches!(self, Block::Bullet(_) | Block::Numbered(_))
    }

    pub fn list_level(self) -> Option<u8> {
        match self {
            Block::Bullet(l) | Block::Numbered(l) => Some(l),
            _ => None,
        }
    }

    pub fn with_level(self, level: u8) -> Self {
        let l = level.min(MAX_LIST_LEVEL);
        match self {
            Block::Bullet(_) => Block::Bullet(l),
            Block::Numbered(_) => Block::Numbered(l),
            other => other,
        }
    }

    /// Whether `self` and `other` share the same block kind (ignoring list nesting level).
    pub fn same_kind(self, other: Block) -> bool {
        matches!(
            (self, other),
            (Block::Paragraph, Block::Paragraph)
                | (Block::Bullet(_), Block::Bullet(_))
                | (Block::Numbered(_), Block::Numbered(_))
                | (Block::Quote, Block::Quote)
                | (Block::Code, Block::Code)
        )
    }

    /// This block's kind as one bit, for `TextEdit::allow_blocks`.
    #[doc(hidden)]
    pub fn kind_bit(self) -> u8 {
        1 << match self {
            Block::Paragraph => 0,
            Block::Bullet(_) => 1,
            Block::Numbered(_) => 2,
            Block::Quote => 3,
            Block::Code => 4,
        }
    }

    /// Left indentation in points (before scale) for this block type.
    #[doc(hidden)]
    pub fn indent_pt(self, font_size: f32) -> f32 {
        let unit = (font_size * 1.45).round().max(18.0);
        match self {
            Block::Paragraph => 0.0,
            Block::Bullet(l) | Block::Numbered(l) => unit * (l.min(MAX_LIST_LEVEL) as f32 + 1.0),
            Block::Quote => (font_size * 1.1).round().max(14.0),
            Block::Code => (font_size * 0.75).round().max(10.0),
        }
    }

    /// Left indent and right padding in device px reserved during line breaking for this block.
    pub(crate) fn pad_px(self, font_size: f32, scale: f32) -> [f32; 2] {
        let indent = (self.indent_pt(font_size) * scale).round();
        let right = if self == Block::Code { indent } else { 0.0 };
        [indent, right]
    }
}

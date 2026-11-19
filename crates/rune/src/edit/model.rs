//! The rich-text model's value types: inline marks, paragraph blocks and their palette.

use std::ops::Range;

use crate::color::{oklch, oklcha, Color};

/// Maximum nesting level (`0..=MAX_LIST_LEVEL`) for [`Block::Bullet`] and [`Block::Numbered`].
pub const MAX_LIST_LEVEL: u8 = 3;

/// An inline formatting mark stored alongside [`TextEdit::text`]. More marks may come, so a
/// `match` needs a `_` arm.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Mark {
    Bold,
    Italic,
    Underline,
    Strike,
    Code,
    Link(String),
    /// An atomic inline object (an `@file` or `@agent` mention, an attachment) carrying the
    /// app's id; its text is the label. The caret steps over it, deleting takes it whole, a
    /// selection that touches it covers it, and typing never extends it. A field draws it as a
    /// chip ([`El::pill_style`](crate::El::pill_style), [`El::pill_colors`](crate::El::pill_colors),
    /// [`El::pill_icon`](crate::El::pill_icon)). Insert one with [`TextEdit::insert_pill`].
    Pill(String),
    /// The run's font size in points, over the field's or its paragraph style's.
    Size(MarkSize),
    /// The run's text colour.
    Color(MarkColor),
}

/// A font size in points that compares and hashes by its bits, so a [`Mark`] can carry it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct MarkSize(u32);

impl MarkSize {
    pub fn pt(points: f32) -> Self {
        Self(points.max(1.0).to_bits())
    }

    pub fn get(self) -> f32 {
        f32::from_bits(self.0)
    }
}

/// A colour that compares and hashes by its bits, so a [`Mark`] can carry it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct MarkColor([u32; 4]);

impl MarkColor {
    pub fn new(c: Color) -> Self {
        Self([c.r, c.g, c.b, c.a].map(f32::to_bits))
    }

    pub fn get(self) -> Color {
        let [r, g, b, a] = self.0.map(f32::from_bits);
        Color { r, g, b, a }
    }
}

impl Mark {
    /// Whether `self` and `other` are the same kind of mark (`Link(_)` matches any `Link(_)`).
    pub fn same_kind(&self, other: &Mark) -> bool {
        match (self, other) {
            (Mark::Bold, Mark::Bold)
            | (Mark::Italic, Mark::Italic)
            | (Mark::Underline, Mark::Underline)
            | (Mark::Strike, Mark::Strike)
            | (Mark::Code, Mark::Code)
            | (Mark::Link(_), Mark::Link(_))
            | (Mark::Pill(_), Mark::Pill(_))
            | (Mark::Size(_), Mark::Size(_))
            | (Mark::Color(_), Mark::Color(_)) => true,
            _ => false,
        }
    }

    /// The URL when `self` is [`Mark::Link`].
    pub fn link_url(&self) -> Option<&str> {
        match self {
            Mark::Link(url) => Some(url),
            _ => None,
        }
    }

    /// The app's id when `self` is [`Mark::Pill`].
    pub fn pill_id(&self) -> Option<&str> {
        match self {
            Mark::Pill(id) => Some(id),
            _ => None,
        }
    }

    pub(crate) fn is_pill(&self) -> bool {
        matches!(self, Mark::Pill(_))
    }

    pub(crate) fn kind_order(&self) -> u8 {
        match self {
            Mark::Bold => 0,
            Mark::Italic => 1,
            Mark::Underline => 2,
            Mark::Strike => 3,
            Mark::Code => 4,
            Mark::Link(_) => 5,
            Mark::Pill(_) => 6,
            Mark::Size(_) => 7,
            Mark::Color(_) => 8,
        }
    }

    /// Whether one range of the kind carries one value, so a later mark replaces an earlier one
    /// where they overlap rather than joining it.
    pub(crate) fn is_valued(&self) -> bool {
        matches!(self, Mark::Link(_) | Mark::Size(_) | Mark::Color(_))
    }
}

/// How many kinds [`Mark::kind_order`] numbers.
pub(crate) const MARK_KINDS: usize = 9;

/// A non-empty byte range `[start, end)` in [`TextEdit::text`] carrying an inline [`Mark`].
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct MarkRange {
    pub start: usize,
    pub end: usize,
    pub mark: Mark,
}

impl MarkRange {
    pub fn new(range: Range<usize>, mark: Mark) -> Self {
        Self { start: range.start, end: range.end, mark }
    }

    pub fn range(&self) -> Range<usize> {
        self.start..self.end
    }
}

/// Paragraph-level block attribute for a `\n`-delimited line in [`TextEdit`].
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
    /// A paragraph in the field's `n`th [`ParagraphStyle`] ([`El::paragraph_styles`](crate::El::paragraph_styles)),
    /// the field's own style when it has no such entry.
    Styled(u16),
}

/// A paragraph's text style and the room around it, which the app supplies for
/// [`Block::Styled`]. More fields may come, so build one from [`ParagraphStyle::new`].
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub struct ParagraphStyle {
    pub text: crate::tok::TextStyle,
    /// Points above the paragraph, dropped at the top of a page its lines flowed onto.
    pub space_before: f32,
    /// Points below it.
    pub space_after: f32,
    /// Starts the paragraph on a new page of a field with [`El::page_flow`](crate::El::page_flow).
    pub page_break_before: bool,
}

impl std::hash::Hash for ParagraphStyle {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        (self.text, self.space_before.to_bits(), self.space_after.to_bits(), self.page_break_before).hash(state);
    }
}

impl ParagraphStyle {
    pub fn new(text: crate::tok::TextStyle) -> Self {
        Self { text, space_before: 0.0, space_after: 0.0, page_break_before: false }
    }

    pub fn space(mut self, before: f32, after: f32) -> Self {
        (self.space_before, self.space_after) = (before, after);
        self
    }

    pub fn page_break_before(mut self) -> Self {
        self.page_break_before = true;
        self
    }
}

/// Lays a field's lines into pages `height` points tall with `gap` points between them
/// ([`El::page_flow`](crate::El::page_flow)).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PageFlow {
    pub height: f32,
    pub gap: f32,
}

impl PageFlow {
    pub fn new(height: f32, gap: f32) -> Self {
        Self { height, gap }
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
                | (Block::Styled(_), Block::Styled(_))
        )
    }

    /// This block's kind as one bit, for [`TextEdit::allow_blocks`].
    pub(super) fn kind_bit(self) -> u8 {
        1 << match self {
            Block::Paragraph => 0,
            Block::Bullet(_) => 1,
            Block::Numbered(_) => 2,
            Block::Quote => 3,
            Block::Code => 4,
            Block::Styled(_) => 5,
        }
    }

    /// Left indentation in points (before scale) for this block type.
    pub(crate) fn indent_pt(self, font_size: f32) -> f32 {
        let unit = (font_size * 1.45).round().max(18.0);
        match self {
            Block::Paragraph | Block::Styled(_) => 0.0,
            Block::Bullet(l) | Block::Numbered(l) => unit * (l.min(MAX_LIST_LEVEL) as f32 + 1.0),
            Block::Quote => (font_size * 1.1).round().max(14.0),
            Block::Code => (font_size * 0.75).round().max(10.0),
        }
    }
}

/// A paragraph slice of [`TextEdit`], returned by [`TextEdit::paragraphs`].
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct ParagraphInfo {
    pub index: usize,
    pub range: Range<usize>,
    pub block: Block,
    /// True when this line was split from the previous line by `Shift+Return` (`<br>`).
    pub soft_break: bool,
}

/// Visual palette for rich marks and block attributes inside [`text_input`](crate::text_input).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RichStyle {
    pub code_fg: Option<Color>,
    pub code_bg: Color,
    pub code_block_bg: Color,
    pub link: Color,
    pub quote_bar: Color,
    pub marker: Option<Color>,
}

impl Default for RichStyle {
    fn default() -> Self {
        Self {
            code_fg: None,
            code_bg: oklcha(0.55, 0.02, 260.0, 0.16),
            code_block_bg: oklcha(0.5, 0.02, 260.0, 0.12),
            link: oklch(0.68, 0.16, 250.0),
            quote_bar: oklcha(0.6, 0.04, 255.0, 0.45),
            marker: None,
        }
    }
}

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
    /// A paragraph in the field's `n`th [`ParagraphStyle`] (`El::paragraph_styles`), the field's
    /// own style when it has no such entry.
    Styled(u16),
}

/// A paragraph's text style and the room around it, which the app supplies for
/// [`Block::Styled`]. More fields may come, so build one from [`ParagraphStyle::new`].
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub struct ParagraphStyle {
    pub text: TextStyle,
    /// Points above the paragraph, dropped at the top of a page its lines flowed onto.
    pub space_before: f32,
    /// Points below it.
    pub space_after: f32,
    /// Starts the paragraph on a new page of a field with `El::page_flow`.
    pub page_break_before: bool,
}

impl std::hash::Hash for ParagraphStyle {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        (self.text, self.space_before.to_bits(), self.space_after.to_bits(), self.page_break_before).hash(state);
    }
}

impl ParagraphStyle {
    pub fn new(text: TextStyle) -> Self {
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
/// (`El::page_flow`).
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

    /// This block's kind as one bit, for `TextEdit::allow_blocks`.
    #[doc(hidden)]
    pub fn kind_bit(self) -> u8 {
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
    #[doc(hidden)]
    pub fn indent_pt(self, font_size: f32) -> f32 {
        let unit = (font_size * 1.45).round().max(18.0);
        match self {
            Block::Paragraph | Block::Styled(_) => 0.0,
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

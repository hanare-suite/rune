//! The rich-text model's value types: inline marks, paragraph blocks and their palette.

use std::ops::Range;

use rune_core::color::{Color, oklch, oklcha};

pub use rune_text::{Baseline, Block, MAX_LIST_LEVEL, PageFlow, ParagraphAlign, ParagraphAttrs, ParagraphStyle};

/// An inline formatting mark stored alongside [`TextEdit::text`](super::TextEdit::text). More
/// marks may come, so a `match` needs a `_` arm.
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
    /// chip (`El::pill_style`, `El::pill_colors`, `El::pill_icon`). Insert one with
    /// [`TextEdit::insert_pill`](super::TextEdit::insert_pill).
    Pill(String),
    /// Inline text colour stored as lowercase `#rrggbb` (sRGB hex).
    ///
    /// Apply or remove it on the selection or pending typing style with
    /// [`TextEdit::set_color`](super::TextEdit::set_color), and query it with
    /// [`TextEdit::active_color`](super::TextEdit::active_color).
    Color(String),
    /// The run's font size in points, over the field's or its paragraph style's. A field keeps
    /// it as it keeps [`Mark::Color`]: only once it is allowed, as a document field's paragraph
    /// attributes and a [`TextEdit::from_parts`](super::TextEdit::from_parts) holding one do, or
    /// [`TextEdit::allow_marks`](super::TextEdit::allow_marks) naming one.
    Size(MarkSize),
    /// The run's font family by name, when the app registered it; the field's family otherwise.
    Family(std::sync::Arc<str>),
    /// A flat background behind the run, as a highlighter pen leaves: no rounding, no padding.
    Highlight(MarkColor),
    /// The run as a subscript or superscript.
    Baseline(Baseline),
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

impl Mark {
    /// [`Mark::Family`] of `name`.
    pub fn family(name: &str) -> Mark {
        Mark::Family(name.into())
    }

    /// Whether `self` and `other` are the same kind of mark (`Link(_)` matches any `Link(_)`,
    /// `Color(_)` matches any `Color(_)`).
    pub fn same_kind(&self, other: &Mark) -> bool {
        self.kind_order() == other.kind_order()
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

    /// The normalized lowercase `#rrggbb` hex colour when `self` is [`Mark::Color`].
    ///
    /// ```
    /// use rune_edit::Mark;
    /// assert_eq!(Mark::Color("#d93025".into()).color_hex(), Some("#d93025"));
    /// assert_eq!(Mark::Bold.color_hex(), None);
    /// ```
    pub fn color_hex(&self) -> Option<&str> {
        match self {
            Mark::Color(hex) => Some(hex),
            _ => None,
        }
    }

    /// Whether one range of the kind carries one value, so a later mark replaces an earlier one
    /// where they overlap rather than joining it.
    pub(crate) fn is_valued(&self) -> bool {
        matches!(self, Mark::Link(_) | Mark::Color(_) | Mark::Size(_) | Mark::Family(_) | Mark::Highlight(_) | Mark::Baseline(_))
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
            Mark::Color(_) => 7,
            Mark::Size(_) => 8,
            Mark::Family(_) => 9,
            Mark::Highlight(_) => 10,
            Mark::Baseline(_) => 11,
        }
    }
}

/// How many kinds [`Mark::kind_order`] numbers.
pub(crate) const MARK_KINDS: usize = 12;

/// A non-empty byte range `[start, end)` in [`TextEdit::text`](super::TextEdit::text) carrying an
/// inline [`Mark`].
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

/// A paragraph slice of [`TextEdit`](super::TextEdit), returned by
/// [`TextEdit::paragraphs`](super::TextEdit::paragraphs).
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct ParagraphInfo {
    pub index: usize,
    pub range: Range<usize>,
    pub block: Block,
    /// True when this line was split from the previous line by `Shift+Return` (`<br>`).
    pub soft_break: bool,
    /// Its style, alignment, indents, page break, atom and id.
    pub attrs: ParagraphAttrs,
}

/// Visual palette for rich marks and block attributes inside `text_input`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RichStyle {
    pub code_fg: Option<Color>,
    pub code_bg: Color,
    pub code_block_bg: Color,
    pub link: Color,
    pub quote_bar: Color,
    pub marker: Option<Color>,
}

/// A locked atom's box when the field sets no colours of its own (`El::atom_colors`): its
/// fill and its border.
#[doc(hidden)]
pub fn atom_colors() -> (Color, Color) {
    (oklcha(0.55, 0.02, 260.0, 0.10), oklcha(0.55, 0.02, 260.0, 0.45))
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

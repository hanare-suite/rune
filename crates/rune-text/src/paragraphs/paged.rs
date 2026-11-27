//! Page flow for a by-paragraph field (`El::page_flow`): where each paragraph goes on its page,
//! and which of its lines move to the next one.
//!
//! A paragraph that crosses a page's end keeps one layout. Its lines after the end are broken
//! again with the room to the next page's top above them (`line_break::rebreak_gapped`), so
//! everything that reads a layout's lines (the caret, a press, selection boxes, glyphs) finds
//! them on their page with no mapping of its own.

use super::*;
use crate::{PageFlow, ParagraphStyle};

/// A field's pages in device px. Page `i`'s top is its point position (`PageFlow::page_top`)
/// rounded once, and its bottom the point position of its end rounded the same way, so the
/// rounding never adds up from one page to the next and the app's sheets, drawn from the same
/// points, stay within half a pixel of the text on every page.
#[derive(Clone, Copy)]
struct Pages {
    flow: PageFlow,
    scale: f32,
    /// A page and the gap after it, unrounded.
    stride: f64,
}

impl Pages {
    fn of(spec: &TextInput<'_>, scale: f32) -> Option<Pages> {
        let flow = spec.flow?.page?;
        let flow = PageFlow::new(flow.height.max(1.0 / scale), flow.gap.max(0.0));
        Some(Pages { flow, scale, stride: f64::from((flow.height + flow.gap) * scale) })
    }

    fn top_of(self, page: usize) -> f64 {
        f64::from((self.flow.page_top(page) * self.scale).round())
    }

    fn bottom_of(self, page: usize) -> f64 {
        f64::from(((self.flow.page_top(page) + self.flow.height) * self.scale).round())
    }

    /// The page `y` is on, or the one before the gap it is in.
    fn index(self, y: f64) -> usize {
        let mut page = (y / self.stride).floor().max(0.0) as usize;
        while page > 0 && self.top_of(page) > y { page -= 1; }
        while self.top_of(page + 1) <= y { page += 1; }
        page
    }
}

/// The room around a paragraph, in device px, and whether it starts a page.
pub(super) struct Own {
    before: f64,
    pub(super) after: f64,
    page_break: bool,
}

/// What a field of paragraph styles and pages keeps as it lays its paragraphs out.
pub(super) struct Pager<'a> {
    pages: Option<Pages>,
    styles: &'a [ParagraphStyle],
    /// Where the last paragraph ended, before the room below it.
    ended: f64,
}

impl<'a> Pager<'a> {
    pub(super) fn new(spec: &TextInput<'a>, scale: f32) -> Pager<'a> {
        Pager { pages: Pages::of(spec, scale), styles: spec.flow.map_or(&[], |f| f.styles), ended: 0.0 }
    }

    /// A styled paragraph is shaped in its own style, with its own room around it; any other
    /// takes the field's. A field of paragraph styles or pages places its paragraphs at whole
    /// pixels, so no paragraph's lines are broken from a fraction of one.
    /// Gives the room, the text style and its line height in px.
    pub(super) fn own(&self, block: Block, field: TextStyle, scale: f32) -> (Own, TextStyle, f32) {
        let own = match block { Block::Styled(n) => self.styles.get(n as usize), _ => None };
        let style = own.map_or(field, |o| o.text);
        let px = |pt: f32| f64::from((pt * scale).round());
        let room = Own { before: own.map_or(0.0, |o| px(o.space_before)), after: own.map_or(0.0, |o| px(o.space_after)), page_break: own.is_some_and(|o| o.page_break_before) };
        (room, style, style.line_px() * scale)
    }

    /// Notes that a paragraph `height` px tall now ends from `top`, and gives where the next one
    /// starts below the room under it.
    pub(super) fn end(&mut self, top: f64, height: f32, after: f64) -> f64 {
        self.ended = top + f64::from(height);
        self.ended + after
    }

    /// The field's height once its last paragraph is placed: whole pages and their gaps, or `y`.
    pub(super) fn field_end(&self, y: f64) -> f64 {
        self.pages.map_or(y, |p| p.bottom_of(p.index((self.ended - 1.0).max(0.0))))
    }
}

/// Where a paragraph's layout starts, and the lines of it that start a page: each line's
/// index and the device px its top moves down by.
struct Spot {
    top: f64,
    gaps: Vec<(u32, f32)>,
}

/// What a paged field's paragraph keeps beyond a plain one's: when it crosses a page's end,
/// the lines that start a page and the px each moved down by, its layout without them (which
/// the next layout places again) and its height without what the gaps added.
#[derive(Clone)]
pub(super) struct Paged {
    pub(super) gaps: Box<[(u32, f32)]>,
    pub(super) natural: Option<Rc<Layout<Ink>>>,
    pub(super) natural_height: f32,
}

impl Paged {
    /// A paragraph that crosses no page's end, `height` px tall.
    pub(super) fn plain(height: f32) -> Paged {
        Paged { gaps: Box::default(), natural: None, natural_height: height }
    }
}

/// A paragraph placed: where it starts, its height with the gaps, its layout and what it keeps.
pub(super) struct Placed {
    pub(super) top: f64,
    pub(super) height: f32,
    pub(super) layout: Rc<Layout<Ink>>,
    pub(super) paged: Paged,
}

impl TextSystem {
    /// Places a paragraph `height` px tall, laid out as `layout`, after what ends at `y`. On its
    /// page: the lines past the page's end move to the next, in a layout broken with that room
    /// above them (the last layout's, when `kept` moved the same lines and `layout` is not new).
    #[allow(clippy::too_many_arguments)]
    pub(super) fn place_on_page(&mut self, pager: &Pager<'_>, y: f64, first: bool, own: &Own, layout: Rc<Layout<Ink>>, height: f32, kept: Option<&Paragraph>, fresh: bool, blocks: &[Block], (indent, size): (f32, f32), scale: f32, max_width: Option<f32>, align: Alignment) -> Placed {
        let spot = place(pager, y, first, own, height, || lines_of(&layout, height));
        let natural_height = height;
        let height = height + spot.gaps.iter().map(|g| g.1).sum::<f32>();
        let (layout, natural) = if spot.gaps.is_empty() {
            (layout, None)
        } else if let Some(o) = kept.filter(|o| !fresh && *o.paged.gaps == *spot.gaps) {
            (o.layout.clone(), Some(layout))
        } else {
            let mut l = layout.clone();
            line_break::rebreak_gapped(Rc::make_mut(&mut l), blocks, indent, size, scale, max_width, align, &spot.gaps);
            self.layout_ids.build(&l);
            (l, Some(layout))
        };
        Placed { top: spot.top, height, layout, paged: Paged { gaps: spot.gaps.into(), natural, natural_height } }
    }
}

/// Places a paragraph `height` px tall after what ends at `y`, the room below the last one
/// included. `forced` starts it on a new page. `lines` gives its lines' tops and bottoms in its
/// layout, and is asked only when it doesn't fit what is left of its page. The room above is
/// dropped at the top of a page the paragraph flowed onto, and kept under a forced break.
fn place(pager: &Pager<'_>, y: f64, first: bool, own: &Own, height: f32, lines: impl FnOnce() -> Vec<(f32, f32)>) -> Spot {
    let Some(p) = pager.pages else { return Spot { top: y + own.before, gaps: Vec::new() } };
    let mut y = y;
    // What came before ended on a page, and its room below ran past the page's end, or on into
    // the gap: both rooms are dropped and this paragraph starts the next page.
    let last = p.index((pager.ended - 1.0).max(0.0));
    if y >= p.bottom_of(last) { y = p.top_of(last + 1); }
    let forced = own.page_break && !first;
    let mut page = p.index(y);
    if forced && y > p.top_of(page) { page += 1; y = p.top_of(page); }
    let flowed = y == p.top_of(page) && !forced && !first;
    // The paragraph is drawn at whole pixels (`top.round()`), so it is fitted there too.
    let mut top = if flowed { y } else { (y + own.before).round() };
    let mut bottom = p.bottom_of(page);
    if top + f64::from(height) <= bottom {
        return Spot { top, gaps: Vec::new() };
    }
    let lines = lines();
    let mut gaps = Vec::new();
    let mut shift = 0.0f64;
    for (i, &(min, max)) in lines.iter().enumerate() {
        let (line_top, line_bottom) = (top + shift + f64::from(min), top + shift + f64::from(max));
        // A line alone on its page stays there, however tall.
        if line_bottom <= bottom || line_top <= p.top_of(page) { continue; }
        page += 1;
        let next = p.top_of(page);
        if i == 0 {
            top = (next - f64::from(min)).round();
        } else {
            let gap = next - line_top;
            gaps.push((i as u32, gap as f32));
            shift += gap;
        }
        bottom = p.bottom_of(page);
    }
    Spot { top, gaps }
}

/// A layout's lines' tops and bottoms, one line `height` tall for a layout with none.
fn lines_of(layout: &Layout<Ink>, height: f32) -> Vec<(f32, f32)> {
    let lines: Vec<(f32, f32)> = layout.lines().map(|l| (l.metrics().block_min_coord, l.metrics().block_max_coord)).collect();
    if lines.is_empty() { vec![(0.0, height)] } else { lines }
}

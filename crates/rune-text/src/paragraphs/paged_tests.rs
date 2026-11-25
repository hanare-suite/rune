//! Ways placing a paragraph on pages can go wrong when a line is taller than a page:
//!
//! 1. The line after it gets a negative gap, and is drawn over it.
//! 2. A line that fits a page straddles a page's end.
//! 3. The paragraph starts above where the text before it ended.

use super::*;

/// Pages `height` tall with `gap` between them, the last paragraph having ended at `ended`.
fn pages(height: f64, gap: f64, keep: bool, ended: f64) -> Pager<'static> {
    let flow = PageFlow::new(height as f32, gap as f32);
    Pager { pages: Some(Pages { flow, scale: 1.0, stride: height + gap, keep }), styles: &[], ended }
}

/// No room around a paragraph, and no page break before it.
fn bare() -> Own {
    Own { before: 0.0, after: 0.0, page_break: false }
}

/// Each line's top and bottom after `placed` moved them.
fn drawn(placed: &Spot, lines: &[(f32, f32)]) -> Vec<(f64, f64)> {
    let mut shift = 0.0;
    lines.iter().enumerate().map(|(i, l)| {
        shift += placed.gaps.iter().filter(|g| g.0 as usize == i).map(|g| f64::from(g.1)).sum::<f64>();
        (placed.top + shift + f64::from(l.0), placed.top + shift + f64::from(l.1))
    }).collect()
}

#[test]
fn a_line_taller_than_the_page_sits_alone_and_the_next_starts_a_page_below_it() {
    for keep in [true, false] {
        let lines = vec![(0.0, 30.0), (30.0, 60.0)];
        let placed = place(&pages(10.0, 3.0, keep, 0.0), 0.0, true, &bare(), 60.0, || lines.clone());
        assert_eq!((placed.top, placed.gaps.as_slice()), (0.0, &[(1, 9.0)][..]), "keep {keep}: the second line starts the page at 39");

        let lines = vec![(0.0, 30.0), (30.0, 60.0), (60.0, 90.0)];
        let placed = place(&pages(10.0, 3.0, keep, 0.0), 0.0, true, &bare(), 90.0, || lines.clone());
        assert_eq!(placed.gaps, [(1, 9.0), (2, 9.0)], "keep {keep}");
    }
}

#[test]
fn a_tall_line_among_short_ones_moves_only_what_follows_it() {
    // Pages of 40 with 10 between: a 20 px line, a 100 px line, a 20 px line.
    let lines = vec![(0.0, 20.0), (20.0, 120.0), (120.0, 140.0)];
    let placed = place(&pages(40.0, 10.0, false, 0.0), 0.0, true, &bare(), 140.0, || lines.clone());
    // The tall line starts page 2 at 50 and runs to 150, the top of page 4, where the last starts.
    assert_eq!(drawn(&placed, &lines), [(0.0, 20.0), (50.0, 150.0), (150.0, 170.0)]);
}

#[test]
fn no_gap_is_negative_and_a_line_that_fits_a_page_stays_on_one() {
    let mut seed = 0x2545_f491_4f6c_dd1du64;
    let mut below = |n: u64| { seed ^= seed << 13; seed ^= seed >> 7; seed ^= seed << 17; seed % n };
    for round in 0..2000 {
        let (height, gap, keep) = (10.0 + below(60) as f64, below(20) as f64, below(2) == 0);
        let stride = height + gap;
        let mut lines = Vec::new();
        let mut at = 0.0f32;
        for _ in 0..1 + below(8) {
            let h = if below(4) == 0 { 5.0 + below(150) as f32 } else { 5.0 + below(20) as f32 };
            lines.push((at, at + h));
            at += h;
        }
        let y = below(4) as f64 * stride + below(height as u64) as f64;
        let placed = place(&pages(height, gap, keep, y), y, false, &bare(), at, || lines.clone());
        let why = format!("round {round}: page {height}, gap {gap}, keep {keep}, y {y}, lines {lines:?}, gaps {:?}, top {}", placed.gaps, placed.top);
        assert!(placed.gaps.iter().all(|g| g.1 >= 0.0), "{why}");
        assert!(placed.top >= y, "{why}");
        for (top, bottom) in drawn(&placed, &lines) {
            let page = (top / stride).floor() * stride;
            assert!(bottom - top > height || bottom <= page + height, "a line that fits crosses its page's end: {top}..{bottom}. {why}");
        }
    }
}

#[test]
fn a_paragraph_that_starts_its_page_with_room_above_it_stays_on_it() {
    let before = |page_break| Own { before: 20.0, after: 0.0, page_break };
    for keep in [true, false] {
        // A 90 px atom after a page break, behind room above it: it opens page 2 and does not
        // fit what is left of it, but moving on would leave that page blank.
        let atom = vec![(0.0, 90.0)];
        let placed = place(&pages(100.0, 10.0, keep, 95.0), 95.0, false, &before(true), 90.0, || atom.clone());
        assert_eq!((placed.top, placed.gaps.as_slice()), (130.0, &[][..]), "keep {keep}: forced break");

        // The field's first paragraph, with room above it, taller than its page.
        let placed = place(&pages(100.0, 10.0, keep, 0.0), 0.0, true, &before(false), 90.0, || atom.clone());
        assert_eq!((placed.top, placed.gaps.as_slice()), (20.0, &[][..]), "keep {keep}: first paragraph");

        // Two lines, the second past the page's end: the paragraph is not moved off its page as
        // a whole, only the line that doesn't fit.
        let lines = vec![(0.0, 60.0), (60.0, 120.0)];
        let placed = place(&pages(100.0, 10.0, keep, 0.0), 0.0, true, &before(false), 120.0, || lines.clone());
        assert_eq!((placed.top, placed.gaps.as_slice()), (20.0, &[(1, 30.0)][..]), "keep {keep}: first paragraph, two lines");
    }
}

#[test]
fn a_plain_tall_first_paragraph_stays_on_the_first_page() {
    let atom = vec![(0.0, 150.0)];
    let placed = place(&pages(100.0, 10.0, true, 0.0), 0.0, true, &bare(), 150.0, || atom.clone());
    assert_eq!((placed.top, placed.gaps.as_slice()), (0.0, &[][..]));
}

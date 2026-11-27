use super::*;
use parley::layout::Affinity;
use rune_core::color::Color;

fn input<'a>(content: &'a str, blocks: &'a [Block], style: TextStyle, dir: TextDir) -> TextInput<'a> {
    TextInput {
        flow: None,
        content,
        spans: &[],
        blocks,
        pills: &[],
        span_boxes: &[],
        inline_boxes: &[],
        text_indent: 0.0,
        style,
        shape_key: None,
        wrap_anywhere: false,
        pretty: false,
        redacted: false,
        by_line: false,
        by_paragraph: true,
        dir,
    }
}

/// `inp` as a document field's: paragraph attributes, one for each paragraph, and the key a
/// field takes from them (text laid out under the same key is the same layout).
fn document<'a>(mut inp: TextInput<'a>, attrs: &'a [ParagraphAttrs], key: u64) -> TextInput<'a> {
    inp.flow = Some(Flowed { styles: &[], page: None, attrs, key, styles_key: 1 });
    inp
}

#[test]
fn an_empty_line_stays_at_the_left_in_a_field_without_paragraph_attributes() {
    let mut ts = TextSystem::default();
    let style = TextStyle::new(14.0, 400.0);
    let (scale, max_w, caret_w) = (2.0, 400.0, 3.0);
    let text = "one\n\ntwo";
    for align in [Alignment::Center, Alignment::End, Alignment::Right] {
        let laid = ts.layout_text(&input(text, &[], style, TextDir::Fixed(Flow::Ltr)), true, scale, Some(max_w), align, None);
        assert_eq!(laid.view().caret(4, Affinity::Downstream, caret_w).x0 as f32, 0.0, "{align:?}: a plain field's empty line starts at its left edge");
    }

    // A document field places it by the paragraph's own alignment.
    for (key, (align, want)) in [(ParagraphAlign::Center, max_w / 2.0), (ParagraphAlign::Right, max_w - caret_w), (ParagraphAlign::Left, 0.0)].into_iter().enumerate() {
        let attrs = [ParagraphAttrs::default(), ParagraphAttrs::default().align(align), ParagraphAttrs::default()];
        let laid = ts.layout_text(&document(input(text, &[], style, TextDir::Fixed(Flow::Ltr)), &attrs, key as u64 + 10), true, scale, Some(max_w), Alignment::Start, None);
        assert_eq!(laid.view().caret(4, Affinity::Downstream, caret_w).x0 as f32, want, "{align:?}");
    }
}

#[test]
fn caret_on_empty_indented_block_matches_filled_block_start() {
    let mut ts = TextSystem::default();
    let style = TextStyle::new(14.0, 400.0);
    let scale = 2.0;
    let max_w = 400.0;
    let caret_w = 1.5 * scale;

    for blk in [
        Block::Paragraph,
        Block::Bullet(0),
        Block::Bullet(2),
        Block::Numbered(0),
        Block::Numbered(1),
        Block::Quote,
        Block::Code,
    ] {
        let blocks = [blk, blk];
        let empty = ts.layout_text(&input("one
", &blocks, style, TextDir::Fixed(Flow::Ltr)), true, scale, Some(max_w), Alignment::Start, None);
        let filled = ts.layout_text(&input("one
x", &blocks, style, TextDir::Fixed(Flow::Ltr)), true, scale, Some(max_w), Alignment::Start, None);
        let empty_x = empty.view().caret(4, Affinity::Downstream, caret_w).x0 as f32;
        let filled_x = filled.view().caret(4, Affinity::Downstream, caret_w).x0 as f32;
        let indent = (blk.indent_pt(style.size) * scale).round();
        assert_eq!(empty_x, filled_x, "LTR {blk:?}: empty caret x should match filled start");
        assert_eq!(empty_x, indent, "LTR {blk:?}: empty caret x should equal block indent");
        if blk == Block::Paragraph {
            assert_eq!(empty_x, 0.0);
        } else {
            assert!(empty_x > 0.0);
        }

        // RTL: right-edge caret respects code block right padding.
        let rtl_empty = ts.layout_text(&input("א
", &blocks, style, TextDir::Fixed(Flow::Rtl)), true, scale, Some(max_w), Alignment::Start, None);
        let at = "א
".len();
        let rtl_x = rtl_empty.view().caret(at, Affinity::Downstream, caret_w).x0 as f32;
        let right_pad = if blk == Block::Code { indent } else { 0.0 };
        assert_eq!(rtl_x, (max_w - right_pad - caret_w).max(indent), "RTL {blk:?}");
    }

    // A centred paragraph of a document field is centred when it reads right to left too.
    let attrs = [ParagraphAttrs::default(), ParagraphAttrs::default().align(ParagraphAlign::Center)];
    let centred = ts.layout_text(&document(input("א\n", &[], style, TextDir::Fixed(Flow::Rtl)), &attrs, 20), true, scale, Some(max_w), Alignment::Start, None);
    assert_eq!(centred.view().caret("א\n".len(), Affinity::Downstream, caret_w).x0 as f32, max_w / 2.0, "RTL centred");

    // Each paragraph uses its own block indent rather than the previous paragraph's.
    let to_list = ts.layout_text(&input("one
", &[Block::Paragraph, Block::Numbered(0)], style, TextDir::Fixed(Flow::Ltr)), true, scale, Some(max_w), Alignment::Start, None);
    let to_para = ts.layout_text(&input("one
", &[Block::Numbered(0), Block::Paragraph], style, TextDir::Fixed(Flow::Ltr)), true, scale, Some(max_w), Alignment::Start, None);
    assert_eq!(to_list.view().caret(4, Affinity::Downstream, caret_w).x0 as f32, (Block::Numbered(0).indent_pt(style.size) * scale).round());
    assert_eq!(to_para.view().caret(4, Affinity::Downstream, caret_w).x0 as f32, 0.0);
}

#[test]
fn empty_indented_block_move_line_selection_and_highlight_use_indent() {
    let mut ts = TextSystem::default();
    let style = TextStyle::new(14.0, 400.0);
    let scale = 2.0;
    let max_w = 400.0;

    // Moving up from an empty nested list item keeps its indented x as the column goal.
    let nested = Block::Bullet(2);
    let indent = (nested.indent_pt(style.size) * scale).round();
    let text = "hello world across the top line
";
    let laid = ts.layout_text(&input(text, &[Block::Paragraph, nested], style, TextDir::Fixed(Flow::Ltr)), true, scale, Some(max_w), Alignment::Start, None);
    let (up_idx, goal_x) = laid.view().move_line(text.len(), false, None).expect("move up");
    assert_eq!(goal_x, indent);
    assert!(up_idx > 0, "moving up from indented empty item should land inside the paragraph above, not at byte 0");

    // Selecting across a blank list item starts its newline box after the indent.
    let num = Block::Numbered(0);
    let num_indent = (num.indent_pt(style.size) * scale).round();
    let multi = "a

b";
    let list_laid = ts.layout_text(&input(multi, &[num, num, num], style, TextDir::Fixed(Flow::Ltr)), true, scale, Some(max_w), Alignment::Start, None);
    let boxes = list_laid.view().range_boxes(0, multi.len(), false);
    assert_eq!(boxes.len(), 3, "one box per paragraph");
    assert_eq!(boxes[1].0.x0 as f32, num_indent, "empty list item selection starts after indent");

    // Highlight decorations across a blank list item also start after the indent.
    let mut decos = Vec::new();
    let hl = TextHighlight {
        start: 0,
        end: multi.len() as u32,
        bg: Color::WHITE,
        fg: None,
        ring: None,
        pad: 0.0,
        radius: 0.0,
        vpad: 0.0,
        key: None,
    };
    ts.highlight_decorations_flow(list_laid.view(), &[hl], multi.len(), scale, [0.0, 0.0], f32::NEG_INFINITY..f32::INFINITY, &mut decos);
    assert_eq!(decos.len(), 3, "one highlight box per paragraph");
    assert_eq!(decos[1].rect[0], num_indent, "empty list item highlight starts after indent");
}

const INTER: &[u8] = include_bytes!("../../../rune/assets/Inter-Regular.ttf");
const PLEX: &[u8] = include_bytes!("../../../rune/assets/IBMPlexMono-Regular.ttf");

/// A self-contained `TextSystem` with bundled sans and fixed-pitch monospace faces, so
/// `by_line` deferral tests do not depend on host system monospace fonts.
fn mono_text_system() -> TextSystem {
    TextSystem::new_local(&[INTER, PLEX], ("Inter", "IBM Plex Mono"))
}

fn rich_input<'a>(
    content: &'a str,
    spans: &'a [Span],
    style: TextStyle,
    by_line: bool,
    dir: TextDir,
) -> TextInput<'a> {
    TextInput {
        flow: None,
        content,
        spans,
        blocks: &[],
        pills: &[],
        span_boxes: &[],
        inline_boxes: &[],
        text_indent: 0.0,
        style,
        shape_key: None,
        wrap_anywhere: false,
        pretty: false,
        redacted: false,
        by_line,
        by_paragraph: true,
        dir,
    }
}

fn as_paragraphs(tl: &TextLayout) -> &Rc<ParagraphLayout> {
    match tl {
        TextLayout::Paragraphs(p) => p,
        TextLayout::Single(_) => panic!("expected ParagraphLayout"),
    }
}

fn assert_waiting_runs_span_free(pl: &ParagraphLayout, spans: &[Span]) {
    let layout_spans: Vec<&Span> = span::layout_spans(spans).collect();
    for p in &pl.paragraphs {
        if !p.shaped {
            assert!(
                !layout_spans.iter().any(|s| (s.start as usize) < p.end && p.start < s.end as usize && {
                    // Any overlap with a non-newline byte inside the waiting run is forbidden.
                    let lo = (s.start as usize).max(p.start);
                    let hi = (s.end as usize).min(p.end);
                    pl.lazy.as_ref().unwrap().content.as_bytes()[lo..hi].iter().any(|&b| b != b'\n' && b != b'\r')
                }),
                "waiting paragraph {}..{} overlaps a layout span",
                p.start,
                p.end,
            );
        }
    }
}

#[test]
fn by_line_defers_plain_ascii_paragraphs_when_head_has_underline() {
    let head = "A".repeat(1500);
    let tail: String = (0..1000).map(|i| format!("line {i}\n")).collect();
    let content = format!("{head}\n{tail}");
    let spans = [Span::new(0, head.len()).underline(Color::BLACK)];
    let style = TextStyle::new(13.0, 400.0).mono();
    let scale = 2.0;
    let max_w = Some(320.0 * scale);

    let mut plain_ts = mono_text_system();
    let plain_tl = plain_ts.layout_text(&rich_input(&content, &[], style, true, TextDir::Auto), false, scale, max_w, Alignment::Start, None);
    let plain_pl = as_paragraphs(&plain_tl);
    assert!(plain_pl.waiting(), "unspanned baseline should defer plain interior lines");
    let baseline_shaped = plain_ts.shaped;

    let mut spanned_ts = mono_text_system();
    let spanned_tl = spanned_ts.layout_text(&rich_input(&content, &spans, style, true, TextDir::Auto), false, scale, max_w, Alignment::Start, None);
    let spanned_pl = as_paragraphs(&spanned_tl);
    assert!(spanned_pl.waiting(), "decorated head line must not disable deferral for plain lines below it");
    assert_eq!(spanned_ts.shaped, baseline_shaped, "initial shaped count matches unspanned baseline");
    assert_waiting_runs_span_free(spanned_pl, &spans);
    assert!(spanned_pl.paragraphs[0].shaped, "decorated head paragraph shapes eagerly");
    assert_eq!(spanned_pl.height, plain_pl.height, "total height matches unspanned layout");

    // Filling every waiting paragraph moves no line heights and matches the baseline line geometry.
    let (filled, moved) = spanned_ts
        .fill_paragraphs(spanned_pl, std::time::Duration::MAX, (0.0, f32::INFINITY), f32::INFINITY)
        .expect("waiting paragraphs fill");
    assert!(!moved, "filling span-free waiting runs must not shift heights");
    assert!(!filled.waiting());
    assert_eq!(filled.height, plain_pl.height);
    assert_eq!(filled.paragraphs.len(), 1002);

    // Head underline decorations match laying out the underlined head paragraph alone.
    let mut head_decos = Vec::new();
    spanned_ts.decorations(&filled.paragraphs[0].layout, &spans, scale, [0.0, 0.0], &mut head_decos);
    let mut solo_ts = mono_text_system();
    let solo_layout = solo_ts.layout_rich(&head, &spans, style, scale, max_w);
    let mut solo_decos = Vec::new();
    solo_ts.decorations(&solo_layout, &spans, scale, [0.0, 0.0], &mut solo_decos);
    assert!(!head_decos.is_empty(), "underlined head emits underline decorations");
    assert_eq!(head_decos.len(), solo_decos.len());
    for (a, b) in head_decos.iter().zip(&solo_decos) {
        assert_eq!(a.rect, b.rect);
    }
}

#[test]
fn interior_disjoint_overlapping_multiline_and_input_spans_keep_waiting_runs_span_free() {
    let lines: Vec<String> = (0..400).map(|i| format!("line {i:03} plain ascii")).collect();
    let content = lines.join("\n");
    let mut starts = Vec::with_capacity(lines.len());
    let mut at = 0;
    for l in &lines {
        starts.push((at, at + l.len()));
        at += l.len() + 1;
    }
    let style = TextStyle::new(13.0, 400.0).mono();
    let scale = 2.0;
    let max_w = Some(480.0 * scale);

    // 1. Disjoint spans in the interior: underline on 120, tall 22pt bold on 200, multi-line
    //    span crossing 280..=282, empty span on 150, colour-only span on 160, newline-only
    //    span between 170 and 171.
    let disjoint = vec![
        Span::new(starts[120].0, starts[120].1).underline(Color::BLACK),
        Span::new(starts[150].0, starts[150].0).underline(Color::BLACK),
        Span::new(starts[160].0, starts[160].1).color(Color::WHITE),
        Span::new(starts[170].1, starts[170].1 + 1).underline(Color::BLACK),
        Span::new(starts[200].0, starts[200].1).bold().size(22.0),
        Span::new(starts[280].0 + 2, starts[282].1 - 2).italic(),
    ];
    let mut ts = mono_text_system();
    let tl = ts.layout_text(&rich_input(&content, &disjoint, style, true, TextDir::Auto), false, scale, max_w, Alignment::Start, None);
    let pl = as_paragraphs(&tl);
    assert!(pl.waiting(), "interior disjoint spans keep plain runs waiting");
    assert_waiting_runs_span_free(pl, &disjoint);
    for idx in [120usize, 200, 280, 281, 282] {
        let (s, e) = starts[idx];
        let p = pl.paragraphs.iter().find(|p| p.start == s && p.end == e).unwrap_or_else(|| panic!("line {idx} should be its own eager paragraph"));
        assert!(p.shaped, "spanned line {idx} must shape eagerly");
    }
    // Colour-only line 160 stays inside a waiting run rather than forcing eager shaping.
    let (s160, e160) = starts[160];
    assert!(pl.paragraphs.iter().any(|p| !p.shaped && p.start <= s160 && e160 <= p.end), "colour-only interior line remains deferred");
    let expected_height = pl.height;
    let (filled, moved) = ts
        .fill_paragraphs(pl, std::time::Duration::MAX, (0.0, f32::INFINITY), f32::INFINITY)
        .expect("fill");
    assert!(!moved, "filling around a 22pt spanned line must not shift heights");
    assert_eq!(filled.height, expected_height);
    assert!(filled.paragraphs[200].height > filled.paragraphs[199].height, "22pt line keeps its taller height");

    // 2. Overlapping and unsorted layout spans (layout_disjoint == false).
    let overlapping = vec![
        Span::new(starts[250].0, starts[250].1).bold(),
        Span::new(starts[140].0, starts[140].1).underline(Color::BLACK),
        Span::new(starts[140].0 + 2, starts[140].1 - 2).italic(),
    ];
    let mut ts_ov = mono_text_system();
    let tl_ov = ts_ov.layout_text(&rich_input(&content, &overlapping, style, true, TextDir::Auto), false, scale, max_w, Alignment::Start, None);
    let pl_ov = as_paragraphs(&tl_ov);
    assert!(pl_ov.waiting(), "unsorted/overlapping layout spans still defer plain runs");
    assert_waiting_runs_span_free(pl_ov, &overlapping);
    for idx in [140usize, 250] {
        let (s, e) = starts[idx];
        let p = pl_ov.paragraphs.iter().find(|p| p.start == s && p.end == e).expect("eager paragraph");
        assert!(p.shaped);
    }
    let (_, moved_ov) = ts_ov
        .fill_paragraphs(pl_ov, std::time::Duration::MAX, (0.0, f32::INFINITY), f32::INFINITY)
        .expect("fill overlapping");
    assert!(!moved_ov);

    // 3. Multi-line input (by_line: false) with a layout span also defers plain ASCII runs.
    let mut ts_in = mono_text_system();
    let tl_in = ts_in.layout_text(&rich_input(&content, &disjoint, style, false, TextDir::Plaintext(Some(Flow::Ltr))), true, scale, max_w, Alignment::Start, None);
    let pl_in = as_paragraphs(&tl_in);
    assert!(pl_in.waiting(), "by_paragraph input with layout spans still defers plain runs");
    assert_waiting_runs_span_free(pl_in, &disjoint);
    let (_, moved_in) = ts_in
        .fill_paragraphs(pl_in, std::time::Duration::MAX, (0.0, f32::INFINITY), f32::INFINITY)
        .expect("fill input");
    assert!(!moved_in);

    // 4. Rebreaking at a narrower width keeps span-free invariant and exact height.
    let narrower = Some(200.0 * scale);
    let rebroken_tl = ts.layout_text(&rich_input(&content, &disjoint, style, true, TextDir::Auto), false, scale, narrower, Alignment::Start, Some(&tl));
    let rebroken_pl = as_paragraphs(&rebroken_tl);
    assert!(rebroken_pl.waiting());
    assert_waiting_runs_span_free(rebroken_pl, &disjoint);
    let (_, moved_rebreak) = ts
        .fill_paragraphs(rebroken_pl, std::time::Duration::MAX, (0.0, f32::INFINITY), f32::INFINITY)
        .expect("fill rebroken");
    assert!(!moved_rebreak);
}

#[test]
fn rtl_blocks_and_pills_preserve_existing_deferral_limits() {
    let content: String = (0..200).map(|i| format!("line {i:03} plain\n")).collect();
    let spans = [Span::new(0, 8).underline(Color::BLACK)];
    let style = TextStyle::new(13.0, 400.0).mono();
    let scale = 2.0;
    let max_w = Some(400.0 * scale);

    // Fixed RTL by_line stays eager (not deferred).
    let mut ts = mono_text_system();
    let rtl = ts.layout_text(&rich_input(&content, &spans, style, true, TextDir::Fixed(Flow::Rtl)), false, scale, max_w, Alignment::Start, None);
    assert!(!as_paragraphs(&rtl).waiting(), "RTL by_line must not defer");

    // Non-Paragraph blocks disable deferral.
    let blocks = [Block::Quote];
    let mut with_block = rich_input(&content, &spans, style, true, TextDir::Auto);
    with_block.blocks = &blocks;
    let blk_tl = ts.layout_text(&with_block, false, scale, max_w, Alignment::Start, None);
    assert!(!as_paragraphs(&blk_tl).waiting(), "non-Paragraph blocks must not defer");

    // Pills disable deferral.
    let pills = [PillChip { start: 0, end: 4, icon: None, image: None, bg: None, fg: None, ring: None, radius: None, bare: None, stand_in: false }];
    let mut with_pill = rich_input(&content, &spans, style, true, TextDir::Auto);
    with_pill.pills = &pills;
    let pill_tl = ts.layout_text(&with_pill, false, scale, max_w, Alignment::Start, None);
    assert!(!as_paragraphs(&pill_tl).waiting(), "pills must not defer");

    // An RTL line inside an LTR document shapes eagerly while surrounding plain lines defer.
    let mut mixed_lines: Vec<String> = (0..200).map(|i| format!("line {i:03} plain")).collect();
    mixed_lines[100] = "שלום עולם".to_string();
    let mixed = mixed_lines.join("\n");
    let mixed_tl = ts.layout_text(&rich_input(&mixed, &spans, style, true, TextDir::Plaintext(Some(Flow::Ltr))), false, scale, max_w, Alignment::Start, None);
    let mixed_pl = as_paragraphs(&mixed_tl);
    assert!(mixed_pl.waiting(), "plain ASCII lines around an RTL line still defer");
    assert_waiting_runs_span_free(mixed_pl, &spans);
    let (_, moved) = ts
        .fill_paragraphs(mixed_pl, std::time::Duration::MAX, (0.0, f32::INFINITY), f32::INFINITY)
        .expect("fill mixed");
    assert!(!moved);

    // When the mono slot resolves to a proportional face (e.g. a host with no monospace font
    // installed and none bundled), `by_line` intentionally disables deferral and shapes eagerly.
    let mut prop_ts = TextSystem::new_local(&[INTER], ("Inter", "Inter"));
    let prop_tl = prop_ts.layout_text(&rich_input(&content, &[], style, true, TextDir::Auto), false, scale, max_w, Alignment::Start, None);
    assert!(!as_paragraphs(&prop_tl).waiting(), "proportional fallback in mono slot must not defer by_line");
}

#[test]
fn layout_text_and_fill_paragraphs_ignore_prev_from_older_shaped_epoch() {
    let _guard = crate::with_families("Inter", "IBM Plex Mono");
    let style = TextStyle::new(14.0, 400.0);
    let scale = 2.0;
    let max_w = Some(600.0 * scale);
    let sample = "iiiiiiiiii 0123456789\niiiiiiiiii 0123456789";

    let mut ts = TextSystem::new_local(&[INTER, PLEX], ("Inter", "IBM Plex Mono"));
    let old_tl = ts.layout_text(&rich_input(sample, &[], style, true, TextDir::Auto), false, scale, max_w, Alignment::Start, None);
    let old_w = old_tl.view().full_width();

    // Switching families drops shaped caches and advances shaped_epoch: passing the previous
    // ParagraphLayout must re-shape in the new font instead of reusing old-font paragraphs.
    assert!(ts.set_families("IBM Plex Mono", "Inter"));
    let switched_tl = ts.layout_text(&rich_input(sample, &[], style, true, TextDir::Auto), false, scale, max_w, Alignment::Start, Some(&old_tl));
    let mut fresh_ts = TextSystem::new_local(&[INTER, PLEX], ("IBM Plex Mono", "Inter"));
    let fresh_tl = fresh_ts.layout_text(&rich_input(sample, &[], style, true, TextDir::Auto), false, scale, max_w, Alignment::Start, None);
    assert!(
        (switched_tl.view().full_width() - fresh_tl.view().full_width()).abs() < 0.01,
        "switched layout must match fresh launch: {} vs {}",
        switched_tl.view().full_width(),
        fresh_tl.view().full_width(),
    );
    assert!(
        (switched_tl.view().full_width() - old_w).abs() > 10.0,
        "switched width must differ from old font: {old_w} -> {}",
        switched_tl.view().full_width(),
    );

    // A deferred ParagraphLayout from an older shaped_epoch must not be filled with new-font lines.
    let long: String = (0..200).map(|i| format!("line {i:03} plain\n")).collect();
    let mono_style = TextStyle::new(13.0, 400.0).mono();
    let mut lazy_ts = mono_text_system();
    let lazy_tl = lazy_ts.layout_text(&rich_input(&long, &[], mono_style, true, TextDir::Auto), false, scale, Some(400.0 * scale), Alignment::Start, None);
    let lazy_pl = as_paragraphs(&lazy_tl);
    assert!(lazy_pl.waiting());
    assert!(lazy_ts.set_families("IBM Plex Mono", "Inter"));
    assert!(
        lazy_ts.fill_paragraphs(lazy_pl, std::time::Duration::MAX, (0.0, f32::INFINITY), f32::INFINITY).is_none(),
        "fill_paragraphs must ignore a ParagraphLayout from an older shaped_epoch",
    );
}

#[test]
fn filled_paragraphs_keep_their_shaped_height_through_an_edit_elsewhere() {
    // At a width this narrow the estimate of a waiting line (one line) is far from what shapes.
    let content: String = (0..300).map(|i| format!("line {i:03} plain ascii text here\n")).collect();
    let style = TextStyle::new(13.0, 400.0).mono();
    let (scale, max_w) = (2.0, Some(6.0));
    let mut ts = mono_text_system();
    let tl = ts.layout_text(&rich_input(&content, &[], style, true, TextDir::Auto), false, scale, max_w, Alignment::Start, None);
    let estimated = as_paragraphs(&tl).height;
    let (filled, moved) = ts.fill_paragraphs(as_paragraphs(&tl), std::time::Duration::MAX, (0.0, f32::INFINITY), f32::INFINITY).expect("waiting paragraphs fill");
    assert!(moved && filled.height > 2.0 * estimated, "the estimate is far from the shaped height");

    // The last line changes, and nothing else does: the other lines keep their shaped heights.
    let edited = content.replacen("line 299", "line 29x", 1);
    let prev = TextLayout::Paragraphs(filled.clone());
    let after = ts.layout_text(&rich_input(&edited, &[], style, true, TextDir::Auto), false, scale, max_w, Alignment::Start, Some(&prev));
    assert_eq!(as_paragraphs(&after).height, filled.height, "an edit elsewhere leaves every other line's height as shaped");
}

#[test]
fn room_below_a_paragraph_that_runs_past_its_pages_end_starts_the_next_paragraph_at_the_next_pages_top() {
    // Pages 100 px tall with no gap between them; each paragraph is one 10 px line with 3 px
    // above and 25 px below, so the third one's room below carries y from 89 to 114.
    let style = TextStyle::new(8.0, 400.0).lh(10.0).mono();
    let styles = [ParagraphStyle::new(style).space(3.0, 25.0)];
    let mut spec = rich_input("a\nb\nc\nd", &[], style, false, TextDir::Auto);
    spec.flow = Some(Flowed { styles: &styles, page: Some(PageFlow::new(100.0, 0.0)), attrs: &[], key: 1, styles_key: 1 });
    let mut ts = mono_text_system();
    let tl = ts.layout_text(&spec, true, 1.0, Some(300.0), Alignment::Start, None);
    let p = &as_paragraphs(&tl).paragraphs;
    assert_eq!(p.len(), 4);
    assert!(p[2].y + p[2].height < 100.0 && p[3].y >= 100.0, "the third ends on page 1: {} + {}", p[2].y, p[2].height);
    assert_eq!(p[3].y, 100.0, "the fourth starts at page 2's top, both rooms dropped");
}

/// Every caret of `laid`, and its height.
fn carets(laid: &TextLayout, content: &str) -> Vec<(usize, [f64; 3])> {
    let view = laid.view();
    content.char_indices().map(|(i, _)| i).chain([content.len()])
        .map(|i| { let c = view.caret(i, Affinity::Downstream, 2.0); (i, [c.x0, c.y0, c.y1]) }).collect()
}

/// An app that keeps its span's numbers through an edit that shifts the text: laid out again
/// from the last layout, the text must come out as a fresh layout of it does.
#[test]
fn a_span_that_keeps_its_numbers_through_a_shifting_edit_lays_out_as_a_fresh_text_does() {
    let style = TextStyle::new(14.0, 400.0);
    let before = "alpha and so gamma alpha alpha beta longerword x beta x\nlongerword gamma and beta δέλτα beta longerword beta\nlongerword so δέλτα x beta δέλτα longerword\nx δέλτα δέλτα longerword x beta beta x x alpha\nlongerword so δέλτα gamma longerword so δέλτα and x δέλτα\n";
    let after = before.replacen("longerword gamma", "lorword gamma", 1);
    let spans = [Span::new(109, 113).size(22.0)];
    let blocks: Vec<Block> = (0..6).map(|k| [Block::Quote, Block::Paragraph, Block::Bullet(0), Block::Paragraph][k % 4]).collect();
    let attrs = vec![ParagraphAttrs::default(); 6];
    for attributed in [false, true] {
        let mut ts = TextSystem::default();
        let lay = |ts: &mut TextSystem, content: &str, key: u64, prev: Option<&TextLayout>| {
            let mut inp = TextInput { blocks: &blocks, ..rich_input(content, &spans, style, false, TextDir::Fixed(Flow::Ltr)) };
            if attributed { inp = document(inp, &attrs, key); }
            ts.layout_text(&inp, true, 1.5, Some(300.0), Alignment::Start, prev)
        };
        let first = lay(&mut ts, before, 1, None);
        let next = lay(&mut ts, &after, 2, Some(&first));
        let fresh = lay(&mut TextSystem::default(), &after, 2, None);
        let (a, b) = (carets(&next, &after), carets(&fresh, &after));
        let at = a.iter().zip(&b).position(|(x, y)| x != y);
        assert!(at.is_none(), "attributed {attributed}: the caret at byte {:?} is {:?} laid out again and {:?} fresh", at.map(|i| a[i].0), at.map(|i| a[i].1), at.map(|i| b[i].1));
    }
}

#[test]
fn an_empty_rtl_paragraph_of_a_document_field_stands_where_its_typed_text_does() {
    let style = TextStyle::new(14.0, 400.0);
    let (scale, max_w, caret_w) = (2.0, 400.0, 3.0);
    let mut key = 100;
    let mut laid_out = |ts: &mut TextSystem, text: &str, field: Alignment, para: ParagraphAlign| {
        key += 1;
        let attrs = [ParagraphAttrs::default(), ParagraphAttrs::default().align(para)];
        ts.layout_text(&document(input(text, &[], style, TextDir::Fixed(Flow::Rtl)), &attrs, key), true, scale, Some(max_w), field, None)
    };
    // Typed text goes to the right edge, or to the left one: the caret is where it will start
    // (its right side) or where it ends (its left).
    for (field, para, right) in [
        (Alignment::Start, ParagraphAlign::Left, true),
        (Alignment::End, ParagraphAlign::Left, false),
        (Alignment::Left, ParagraphAlign::Left, false),
        (Alignment::Start, ParagraphAlign::Right, true),
        (Alignment::Start, ParagraphAlign::Justified, true),
    ] {
        let mut ts = TextSystem::default();
        let empty = laid_out(&mut ts, "א\n", field, para).view().caret(3, Affinity::Downstream, caret_w).x0;
        let typed = laid_out(&mut ts, "א\nא", field, para);
        let typed_at = typed.view().caret(if right { 3 } else { 5 }, Affinity::Downstream, caret_w).x0;
        assert!((empty - typed_at).abs() <= f64::from(caret_w), "{field:?} {para:?}: empty caret at {empty}, typed text at {typed_at}");
    }
}

#[test]
fn an_empty_rtl_code_block_with_an_indent_of_its_own_keeps_the_right_padding_line_breaking_uses() {
    let style = TextStyle::new(14.0, 400.0);
    let (scale, max_w, caret_w) = (2.0, 400.0, 3.0);
    let attrs = [ParagraphAttrs::default(), ParagraphAttrs::default().indent(20.0, 0.0)];
    let blocks = [Block::Code, Block::Code];
    let mut ts = TextSystem::default();
    let laid = ts.layout_text(&document(input("א\n", &blocks, style, TextDir::Fixed(Flow::Rtl)), &attrs, 200), true, scale, Some(max_w), Alignment::Start, None);
    let pad = (Block::Code.indent_pt(style.size) * scale).round() + (20.0 * scale).round();
    assert_eq!(laid.view().caret(3, Affinity::Downstream, caret_w).x0 as f32, max_w - pad - caret_w);
}

#[test]
fn a_selected_line_break_of_an_empty_paragraph_stays_inside_the_wrap_width() {
    let style = TextStyle::new(14.0, 400.0);
    let (scale, max_w) = (2.0, 400.0);
    let mut key = 300;
    for (flow, text) in [(Flow::Ltr, "a\n\nb"), (Flow::Rtl, "א\n\nב")] {
        for (field, para) in [
            (Alignment::Start, ParagraphAlign::Left),
            (Alignment::End, ParagraphAlign::Left),
            (Alignment::Left, ParagraphAlign::Left),
            (Alignment::Start, ParagraphAlign::Right),
            (Alignment::Start, ParagraphAlign::Center),
        ] {
            key += 1;
            let attrs = [ParagraphAttrs::default(), ParagraphAttrs::default().align(para), ParagraphAttrs::default()];
            let mut ts = TextSystem::default();
            let laid = ts.layout_text(&document(input(text, &[], style, TextDir::Fixed(flow)), &attrs, key), true, scale, Some(max_w), field, None);
            // The boxes of the empty paragraph's line: the break after the text before it may
            // stand past the wrap width, as the text it follows does.
            let empty = laid.view().caret(text.find('\n').unwrap() + 1, Affinity::Downstream, 3.0).y0;
            for (bb, _) in laid.view().range_boxes(0, text.len(), false).into_iter().filter(|(bb, _)| (bb.y0 - empty).abs() < 1.0) {
                let (left, right) = (bb.x0, bb.x1);
                assert!(left >= 0.0 && right <= f64::from(max_w), "{flow:?} {field:?} {para:?}: a box spans {left}..{right}, outside 0..{max_w}");
            }
        }
    }
}

/// An app that hands a span over at the numbers its text moved to: laid out again from the last
/// layout, the paragraph the span now starts in must come out as a fresh layout of it does.
#[test]
fn a_span_moved_with_the_tail_that_now_starts_before_the_edit_lays_out_as_a_fresh_text_does() {
    let style = TextStyle::new(14.0, 400.0);
    let (old, new) = ("aaaa\nbbbb\ncccc", "aaaa\nbbbb\ncc");
    let attrs = vec![ParagraphAttrs::default(); 3];
    let mut ts = TextSystem::default();
    let lay = |ts: &mut TextSystem, content: &str, spans: &[Span], prev: Option<&TextLayout>| {
        let inp = document(rich_input(content, spans, style, false, TextDir::Fixed(Flow::Ltr)), &attrs, 1);
        ts.layout_text(&inp, true, 2.0, Some(400.0), Alignment::Start, prev)
    };
    let first = lay(&mut ts, old, &[Span::new(5, 9).size(28.0)], None);
    let spans = [Span::new(3, 7).size(28.0)];
    let next = lay(&mut ts, new, &spans, Some(&first));
    let fresh = lay(&mut TextSystem::default(), new, &spans, None);
    assert_eq!(carets(&next, new), carets(&fresh, new));
    assert_eq!(next.view().height(), fresh.view().height());
}

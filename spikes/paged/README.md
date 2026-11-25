# Paged rich text in one text field

A throwaway spike. It answers one question: can a word processor's editor be Rune's `text_input` and `TextEdit`, extended with run styles, paragraph styles and page flow, or does it have to draw its own? The answer on this branch is that it can: `check.py` holds the pass marks and `perf.py` takes the numbers.

The app shows a generated German document of 25,335 words on 51 A4 pages in one field. The words come from a fixed list and a fixed seed, so every run lays out the same document.

## What the fork gained

- `El::page_flow(PageFlow::new(height, gap))` lays the field's lines into pages. A line that doesn't fit what is left of its page moves whole to the next one. The field is always whole pages tall, so `PageFlow::pages(height)` of its laid-out height is the page count, and the app draws the sheets behind it.
- `Block::Styled(n)` gives a paragraph the field's `n`th `ParagraphStyle` (`El::paragraph_styles`): a text style, the room above and below, and optionally a page break before it.
- `Mark::Size` styles a run, as upstream's `Mark::Color` does its colour.

A paragraph that crosses a page's end keeps one layout. Its lines after the end are broken again with the gap above them (`crates/rune-text/src/paragraphs/paged.rs`, `line_break::rebreak_gapped`), so the caret, presses, selection boxes, glyphs, caret follow and the input method's cursor rectangle need no page mapping of their own.

## Run it

```sh
cargo build --profile perf -p paged-spike --bin paged
cargo build --profile perf -p paged-spike --features profile --bin paged-perf
target/perf/paged                 # a window
python3 spikes/paged/check.py     # the interaction checks, headless
python3 spikes/paged/perf.py      # the numbers
```

`check.py` drives the app with one script per check (`scripts/`), reads the element tree and a screenshot, and writes what it found to `out/<check>.json` and `out/<check>*.png`, which git ignores:

| Check | What it proves |
|---|---|
| `open` | 51 pages and 25,335 words, one sheet drawn per page from the field's height |
| `arrows` | Down from page 1's last line lands on page 2's first; Up goes back |
| `tap` | a press on page 2 puts the caret under the pointer |
| `drag` | a drag from page 1 into page 2 selects on both, with no selection drawn in the gap |
| `typing` | typing at a paragraph's end on page 1's last line moves its last line to page 2; Backspace brings it back |
| `forced-break` | a paragraph whose style breaks the page starts at a page's top, with room left on the page before |
| `ime-headless` | a composition on page 2 moves the input method's cursor rectangle with the caret |

`perf.py` runs `paged-perf` (keystroke, Return, scroll, open and undo memory on a headless runtime) and times a fresh process to its first pixels, into `out/perf.txt`.

## Not proven here

The input method check in a real macOS window, recorded on video, was not run: the machine's display could not be captured. `ime-headless` checks the rectangle Rune hands the platform, not where the system draws its candidate window.

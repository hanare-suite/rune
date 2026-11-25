# Paged rich text in one text field

The fork's own check that Rune's `text_input` and `TextEdit` carry a word processor's editor: run marks, paragraph attributes, pages with their rules, locked blocks, and an undo history the app owns. It began as a spike that asked whether the field could do it at all. It can, and this example now checks every part of it headless. `check.py` holds the pass marks and `perf.py` takes the numbers.

The app shows a generated German document of 25,335 words on 52 A4 pages in one field. The words come from a fixed list and a fixed seed, so every run lays out the same document. Smaller documents (`src/docs.rs`, picked with `PAGED_DOC`) hold what single checks look at. For those the app's line of facts lists every paragraph's id, block and atom, and says whether the document it keeps from the reported changes still matches the field (`sync`).

## What the field does here

- `El::page_flow(PageFlow::new(height, gap))` lays the field's lines into pages. A line that doesn't fit what is left of its page moves whole to the next one. A paragraph keeps its first two lines together and its last two. `Cx::pages("editor")` is the page count in the commit that changed it, and the app draws the sheets behind the field from it.
- Each paragraph has `ParagraphAttrs`: a style index into `El::paragraph_styles`, an alignment, a left and a first-line indent, a page break before it, an id, and for a locked block its height. Return copies them to the new paragraph, which takes the next id.
- `Mark::Size`, `Color`, `Family`, `Highlight` and `Baseline` style a run.
- The field's own history is off (`TextEdit::set_history(false)`). Every edit reports the bytes it changed (`El::on_input_edits`), and the document follows the field through those reports: each event is one step of the document's history, which shares the paragraphs an edit did not touch. Cmd+Z reaches the app's key handler.

A paragraph that crosses a page's end keeps one layout. Its lines after the end are broken again with the gap above them (`crates/rune-text/src/paragraphs/paged.rs`, `line_break::rebreak_paragraph`), so the caret, presses, selection boxes, glyphs, caret follow and the input method's cursor rectangle need no page mapping of their own.

## Run it

```sh
cargo build --profile perf -p paged-spike --bin paged
cargo build --profile perf -p paged-spike --features profile --bin paged-perf
target/perf/paged                 # a window
python3 spikes/paged/check.py     # the interaction checks, headless
python3 spikes/paged/perf.py      # the numbers
```

In the window the function keys stand in for a ribbon: F1 to F4 align the paragraph (centre, right, justified, left), F5 sets the selection in IBM Plex Mono, F6 highlights it, F7 and F8 make it a superscript or a subscript, F9 indents the paragraph, F10 toggles its page break, F11 its bullet, and F12 inserts a locked block.

`check.py` drives the app with one script per check (`scripts/`), reads the element tree and a screenshot, and writes what it found to `out/<check>.json` and `out/<check>*.png`, which git ignores:

| Check | What it proves |
|---|---|
| `open` | 52 pages and 25,335 words, one sheet drawn per page |
| `arrows` | Down from page 1's last line lands on page 2's first; Up goes back |
| `tap` | a press on page 2 puts the caret under the pointer |
| `drag` | a drag from page 1 into page 2 selects on both, with no selection drawn in the gap |
| `typing` | with widow control off, typing at a paragraph's end on page 1's last line moves its last line to page 2; Backspace brings it back |
| `forced-break` | a paragraph with a page break before it starts at a page's top, with room left on the page before |
| `fractional-scale` | at scale 1.25 a chapter heading on page 45 still sits at its sheet's top margin, within half a device pixel |
| `ime-headless` | a composition on page 2 moves the input method's cursor rectangle with the caret |
| `marks-family` | a run in a registered family is shaped in it (monospaced: four M as wide as four i); an unknown name keeps the field's family |
| `marks-highlight` | a highlight starts and ends with its word, fills its line's height, and has square corners; two sizes on one line share one band |
| `marks-baseline` | sub- and superscripts are 58 % as wide and sit a third of the em below and above the baseline |
| `attrs-align` | left, centred, right and justified paragraphs put their lines' ends where they belong |
| `attrs-indent` | a left and a first-line indent, a hanging indent, a styled bullet and an indented one |
| `attrs-split-merge` | Return copies style, alignment and indents to a new paragraph with the next id and without the page break; Backspace merges back to the first paragraph's attributes and id |
| `page-rules` | a paragraph's first line and its last line never stand alone on a page; the room above a paragraph is dropped at a page's top it flowed onto and kept after a forced break |
| `atoms` | the caret crosses a locked block in one step, Shift+Right selects it whole, text typed beside it gets its own paragraph, Backspace removes it as one edit and one history step, and the accessibility tree names it as a group |
| `paste-over-atom` | a rich paste over the selected last block lands where the caret did after the block went, bold on its first letter |
| `paste-at-atom` | a rich paste at a locked block's end and at its start puts bold and bullets on the pasted lines, and the block keeps its line |
| `atom-ids` | text typed over an empty paragraph before a locked block, or over a block up to the next one, leaves every paragraph its own id |
| `atom-keys` | Backspace after a locked block and Delete before one select it, and a second press deletes it as one edit and one history step; an empty paragraph beside a block goes at the first press |
| `history` | the field records nothing; typing, a highlight, Cmd+B and an alignment each report their bytes and make one step of the app's history; Cmd+Z and Shift+Cmd+Z go through the app |
| `reuse` | a keystroke keys one paragraph at 51 pages and at 200, and centring one paragraph keys only it |
| `pages-same-frame` | the frame that lays out a new page already shows its count and its sheet |

`perf.py` runs `paged-perf` (keystroke, Return, scroll, open and undo memory on a headless runtime) at 51 pages against the limits F0 set, again at 200 pages with no limit, and times a fresh process to its first pixels, into `out/perf.txt`.

## Not proven here

The input method check in a real macOS window, recorded on video, was not run: the machine's display could not be captured. `ime-headless` checks the rectangle Rune hands the platform, not where the system draws its candidate window.

//! The F0 numbers for the paged field, headless: no window, the same runtime and renderer.
//!
//! cargo build --profile perf -p paged-spike --features profile --bin paged-perf
//! target/perf/paged-perf [--samples 240] [--words 25000] [--page 25] [--gpu] [--only open,type,enter,scroll,undo,pages]
//!
//! - `open`: from the document model to the first frame laid out and painted, each time on a
//!   new runtime with a new text system (nothing shaped yet). The first run also loads fonts.
//! - `type`: one typed character in the middle of a body paragraph on page 25, and the frame it
//!   commits, with the document's history step. `enter` and `backspace` are Return there and
//!   the Backspace that takes it back: every paragraph below moves, so every page below
//!   breaks again.
//! - `scroll`: each frame of a wheel scroll from the top to the bottom in 120 point steps.
//! - `undo`: heap and footprint before and after 200 typed characters, each its own step in
//!   the document's history, then 200 undos back to the text it started from.
//!
//! - `pages`: Return typed until the document gains a page, and what the view showed in the
//!   one frame that followed: the page count it read and the sheets it drew.
//!
//! Prints `METRIC case=.. metric=.. p50=.. p95=.. max=.. n=..`. `wall_ms` is the input and its
//! frame on the wall clock, `cpu_ms` the same in thread CPU; the phases overlap (commit holds
//! view, layout and apply, layout holds text). `--gpu` adds `encode_ms`: the renderer's
//! prepare, draw and submit on a headless GPU. Presentation is not in any number.
use std::time::Instant;

use paged_spike::{State, WINDOW, body, generate, view};
use rune::render::{Gpu, OFFSCREEN_FORMAT, Offscreen, Renderer};
use rune::text::TextSystem;
use rune::{Env, Ime, Key, KeyEvent, Mods, Runtime};

#[global_allocator]
static ALLOC: procwatch::alloc::Counting = procwatch::alloc::Counting::system();

const STRIDE: f32 = 858.0;

struct Draw { gpu: Gpu, renderer: Renderer, out: Offscreen }

impl Draw {
    fn draw(&mut self, rt: &mut Runtime<State>) -> f64 {
        let t = Instant::now();
        self.renderer.upload_images(&self.gpu.device, &self.gpu.queue, &mut rt.images);
        self.renderer.prepare(&self.gpu.device, &self.gpu.queue, &mut rt.text.atlas, &rt.instances, (self.out.width, self.out.height));
        let mut encoder = self.gpu.device.create_command_encoder(&Default::default());
        self.renderer.draw(&mut encoder, &self.out.view, rt.clear_color());
        self.renderer.submit(&self.gpu.queue, encoder);
        let ms = t.elapsed().as_secs_f64() * 1000.0;
        self.gpu.device.poll(wgpu::PollType::wait_indefinitely()).ok();
        ms
    }
}

const METRICS: &[&str] = &["wall_ms", "cpu_ms", "view_ms", "commit_ms", "layout_ms", "text_ms", "paint_ms", "shaped", "keyed", "glyphs", "encode_ms"];

/// Times `input` and the one frame that follows it.
fn timed(rt: &mut Runtime<State>, d: &mut Option<Draw>, input: impl FnOnce(&mut Runtime<State>)) -> Vec<f64> {
    rune::take_frame_phases();
    let keyed = rt.text.keyed;
    let (t, cpu0) = (Instant::now(), rune::thread_cpu_ms());
    input(rt);
    rt.frame(1.0 / 120.0);
    let (wall, cpu) = (t.elapsed().as_secs_f64() * 1000.0, rune::thread_cpu_ms() - cpu0);
    let p = rune::take_frame_phases();
    let encode = d.as_mut().map_or(0.0, |d| d.draw(rt));
    let q = rune::PHASES;
    vec![wall, cpu, p[q], p[q + 1], p[q + 2], p[q + 3], p[q + 4], rt.stats.shaped as f64, (rt.text.keyed - keyed) as f64, rt.stats.glyphs as f64, encode]
}

fn settle(rt: &mut Runtime<State>, d: &mut Option<Draw>) {
    let mut n = 0;
    while n < 240 && rt.frame(1.0 / 120.0) { n += 1; }
    if let Some(d) = d { d.draw(rt); }
}

fn press(rt: &mut Runtime<State>, key: Key, cmd: bool) {
    rt.key(KeyEvent { key, mods: Mods { cmd, ..Mods::default() } });
}

fn find(rt: &Runtime<State>, key: &str) -> rune::runtime::ElementInfo {
    rt.elements().into_iter().find(|e| e.key.as_deref() == Some(key)).unwrap()
}

/// The page the caret is on, from 1.
fn caret_page(rt: &Runtime<State>) -> usize {
    let e = find(rt, "editor");
    e.caret.map_or(0, |c| ((c[1] - e.rect[1]) / STRIDE) as usize + 1)
}

fn pages(rt: &Runtime<State>) -> usize {
    paged_spike::flow().pages(find(rt, "editor").rect[3])
}

/// A runtime showing the document with the caret at `caret`, its first frame drawn.
fn runtime(words: usize, caret: usize, fonts: TextSystem) -> Runtime<State> {
    let mut state = State::new(generate(words));
    state.edit.focused = true;
    state.edit.set_caret(caret.min(state.edit.text.len()));
    let mut rt = Runtime::with_text(state, view, WINDOW, Env::new(false, 2.0), fonts);
    rt.on_key = Some(paged_spike::app_key);
    rt.frame(0.0);
    rt
}

fn warm_fonts() -> TextSystem {
    let mut fonts = TextSystem::default();
    fonts.layout_rich("Warm fonts", &[], body(), 2.0, None);
    for _ in 0..3 { fonts.end_commit(); }
    fonts
}

/// The page the caret is on once it stands at byte `at`.
fn page_at(rt: &mut Runtime<State>, d: &mut Option<Draw>, at: usize) -> usize {
    rt.update(|s, _| s.edit.set_caret(at));
    // A caret the keyboard moved is followed into view, which is where its page is read.
    press(rt, Key::Right, false);
    press(rt, Key::Left, false);
    settle(rt, d);
    caret_page(rt)
}

/// A byte in the middle of a long body paragraph, on `page`.
fn byte_on_page(rt: &mut Runtime<State>, d: &mut Option<Draw>, page: usize) -> usize {
    let text = rt.state.edit.text.clone();
    let mut start = 0;
    let middles: Vec<usize> = text.split('\n').filter_map(|line| {
        let at = start + line.char_indices().filter(|&(i, c)| c == ' ' && i <= line.len() / 2).last().map_or(0, |(i, _)| i);
        start += line.len() + 1;
        (line.len() > 600).then_some(at)
    }).collect();
    let k = middles.partition_point(|&at| page_at(rt, d, at) < page);
    assert!(k < middles.len() && page_at(rt, d, middles[k]) == page, "no long body paragraph's middle is on page {page}");
    middles[k]
}

fn report(case: &str, rows: &[Vec<f64>]) {
    for (m, name) in METRICS.iter().enumerate() {
        let mut v: Vec<f64> = rows.iter().map(|r| r[m]).collect();
        v.sort_by(f64::total_cmp);
        let at = |q: f64| v[((v.len() - 1) as f64 * q).round() as usize];
        println!("METRIC case={case} metric={name} p50={:.4} p95={:.4} max={:.4} n={}", at(0.5), at(0.95), v[v.len() - 1], v.len());
    }
}

fn heap_mb() -> f64 { procwatch::alloc::heap().map_or(0.0, |h| h.live as f64 / 1_048_576.0) }
fn footprint_mb() -> f64 { procwatch::snapshot().map_or(0.0, |s| s.footprint as f64 / 1_048_576.0) }

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let value = |name: &str| args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned();
    let samples: usize = value("--samples").map_or(240, |s| s.parse().expect("--samples N"));
    let words: usize = value("--words").map_or(25_000, |s| s.parse().expect("--words N"));
    let page: usize = value("--page").map_or(25, |s| s.parse().expect("--page N"));
    let only = value("--only");
    let run = |case: &str| only.as_deref().is_none_or(|o| o.split(',').any(|o| o == case));
    assert!(!cfg!(debug_assertions), "paged-perf: time a --profile perf build");
    let mut d = args.iter().any(|a| a == "--gpu").then(|| {
        let gpu = Gpu::headless();
        let renderer = Renderer::new(&gpu.device, OFFSCREEN_FORMAT);
        let out = Offscreen::new(&gpu.device, (WINDOW.0 * 2.0) as u32, (WINDOW.1 * 2.0) as u32);
        Draw { gpu, renderer, out }
    });

    if run("open") {
        let mut rows = Vec::new();
        for i in 0..9 {
            let doc = generate(words);
            let t = Instant::now();
            let mut state = State::new(doc);
            state.edit.focused = true;
            let model_ms = t.elapsed().as_secs_f64() * 1000.0;
            let mut rt = Runtime::with_text(state, view, WINDOW, Env::new(false, 2.0), TextSystem::default());
            rt.frame(0.0);
            // The sheets come with the second frame: the view reads the field's height.
            rt.frame(1.0 / 120.0);
            let encode = d.as_mut().map_or(0.0, |d| d.draw(&mut rt));
            let ms = t.elapsed().as_secs_f64() * 1000.0;
            println!("OPEN run={i} wall_ms={ms:.2} model_ms={model_ms:.2} encode_ms={encode:.2} pages={} words={} paragraphs={} bytes={}",
                pages(&rt), rt.state.words, rt.state.doc.paras.len(), rt.state.edit.text.len());
            rows.push(ms);
        }
        let first = rows[0];
        rows.sort_by(f64::total_cmp);
        println!("METRIC case=open metric=wall_ms first={first:.4} p50={:.4} max={:.4} n={}", rows[rows.len() / 2], rows[rows.len() - 1], rows.len());
    }

    if run("type") || run("enter") {
        let mut rt = runtime(words, 0, warm_fonts());
        settle(&mut rt, &mut d);
        let at = byte_on_page(&mut rt, &mut d, page);
        rt.update(|s, _| s.edit.set_caret(at));
        for _ in 0..3 { rt.ime(Ime::Commit("w".into())); rt.frame(1.0 / 120.0); }
        settle(&mut rt, &mut d);
        println!("CARET byte={at} page={} of={}", caret_page(&rt), pages(&rt));
        if run("type") {
            let steps = rt.state.history.len();
            let rows: Vec<_> = (0..samples).map(|i| {
                let ch = ["a", "b", "c", " "][i % 4];
                timed(&mut rt, &mut d, |rt| rt.ime(Ime::Commit(ch.into())))
            }).collect();
            assert_eq!(rt.state.history.len() - steps, samples, "type: one history step per character");
            assert_eq!(caret_page(&rt), page, "type: the caret stayed on its page");
            report("type", &rows);
        }
        if run("enter") {
            let (mut enters, mut backs) = (Vec::new(), Vec::new());
            let len = rt.state.edit.text.len();
            for _ in 0..samples / 4 {
                enters.push(timed(&mut rt, &mut d, |rt| press(rt, Key::Enter, false)));
                backs.push(timed(&mut rt, &mut d, |rt| press(rt, Key::Backspace, false)));
            }
            assert_eq!(rt.state.edit.text.len(), len, "enter: each Return taken back");
            report("enter", &enters);
            report("backspace", &backs);
        }
    }

    if run("scroll") {
        let mut rt = runtime(words, 0, warm_fonts());
        settle(&mut rt, &mut d);
        let (top, mut rows) = (find(&rt, "editor").rect[1], Vec::new());
        loop {
            let before = find(&rt, "editor").rect[1];
            rows.push(timed(&mut rt, &mut d, |rt| rt.wheel(WINDOW.0 / 2.0, WINDOW.1 / 2.0, -120.0)));
            if (find(&rt, "editor").rect[1] - before).abs() < 0.01 { break; }
        }
        println!("SCROLL frames={} travel_pt={:.0}", rows.len(), top - find(&rt, "editor").rect[1]);
        report("scroll", &rows);
    }

    if run("pages") {
        let mut rt = runtime(words, 0, warm_fonts());
        settle(&mut rt, &mut d);
        let end = rt.state.edit.text.len();
        rt.update(|s, _| s.edit.set_caret(end));
        settle(&mut rt, &mut d);
        let shown = |rt: &Runtime<State>| {
            let status = find(rt, "status").text.unwrap_or_default();
            let sheets = rt.elements().iter().filter(|e| e.key.as_deref().is_some_and(|k| k.starts_with("page") && k != "paper")).count();
            (status.split(' ').next().and_then(|n| n.parse::<usize>().ok()).unwrap_or(0), sheets)
        };
        let before = (pages(&rt), shown(&rt));
        // Return at the document's end until its last line moves onto a new page: the frame
        // that lays the new page out must already show it.
        let (mut presses, mut after) = (0, before);
        while after.0 == before.0 && presses < 200 {
            press(&mut rt, Key::Enter, false);
            rt.frame(1.0 / 120.0);
            presses += 1;
            after = (pages(&rt), shown(&rt));
        }
        println!("PAGES before_field={} before_status={} before_sheets={} returns={presses} after_field={} same_frame_status={} same_frame_sheets={}",
            before.0, before.1.0, before.1.1, after.0, after.1.0, after.1.1);
        assert_eq!((after.0, after.1.0, after.1.1), (before.0 + 1, before.0 + 1, before.0 + 1), "pages: the count and the sheets follow in the same frame");
    }

    if run("undo") {
        let mut rt = runtime(words, 0, warm_fonts());
        settle(&mut rt, &mut d);
        let at = byte_on_page(&mut rt, &mut d, page);
        rt.update(|s, _| s.edit.set_caret(at));
        settle(&mut rt, &mut d);
        let original = rt.state.edit.text.clone();
        let steps = rt.state.history.len();
        let (heap0, foot0) = (heap_mb(), footprint_mb());
        for i in 0..200 {
            rt.ime(Ime::Commit(["a", "b", "c", " "][i % 4].into()));
            rt.frame(1.0 / 120.0);
        }
        settle(&mut rt, &mut d);
        let (heap1, foot1) = (heap_mb(), footprint_mb());
        let typed = rt.state.history.len() - steps;
        let shared = rt.state.doc.paras.len();
        let mut undone = 0;
        while rt.state.edit.text != original && undone < 400 {
            press(&mut rt, Key::Command('z'), true);
            rt.frame(1.0 / 120.0);
            undone += 1;
        }
        println!("UNDO steps={typed} paragraphs={shared} heap_before_mb={heap0:.2} heap_after_mb={heap1:.2} heap_extra_mb={:.2} \
            footprint_before_mb={foot0:.2} footprint_after_mb={foot1:.2} footprint_extra_mb={:.2} undos_to_original={undone} restored={}",
            heap1 - heap0, foot1 - foot0, rt.state.edit.text == original);
        assert_eq!((typed, undone), (200, 200), "undo: 200 steps typed, 200 undos back");
    }
}

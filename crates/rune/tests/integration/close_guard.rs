//! `App::on_close_guard` in a headless `--script` run: the close button (`{"close": true}`), a
//! user close request (`{"close_request": true}`) and `Tx::close_window` all ask the guard
//! while it says true, in the order close request, guard, `on_close`; `Tx::quit` is not held.
use rune::*;

#[derive(Default)]
struct State { guarded: bool, defers: bool, reclose: bool, log: Vec<&'static str> }

fn view(s: &State, _: &mut Cx<State>) -> El {
    text(s.log.join(",")).key("log").into()
}

fn on_key(s: &mut State, k: &KeyEvent, tx: &mut Tx) -> bool {
    match &k.key {
        Key::Text(t) if t == "c" => tx.close_window(),
        Key::Text(t) if t == "q" => tx.quit(),
        _ => return false,
    }
    s.log.push("key");
    true
}

/// Runs `script` against an app whose guard says `guarded`, whose `on_close_request` defers when
/// `defers`, and returns the tree's `closed` flag and the log of what ran.
fn run(script: &str, guarded: bool, defers: bool) -> (bool, String) { run_app(script, guarded, defers, false) }

/// As `run`, with an `ask` that asks to close the window again.
fn run_reclose(script: &str, desktop: bool) -> (bool, String) { run_with(script, Opts { reclose: true, desktop, ..Opts::default() }) }

/// As `run`, for an app that `keep_running` or not.
fn run_app(script: &str, guarded: bool, defers: bool, keep_running: bool) -> (bool, String) {
    run_with(script, Opts { guarded, defers, keep_running, ..Opts::default() })
}

#[derive(Default)]
struct Opts { guarded: bool, defers: bool, keep_running: bool, reclose: bool, desktop: bool }

fn run_with(script: &str, Opts { guarded, defers, keep_running, reclose, desktop }: Opts) -> (bool, String) {
    if !rune::render::Gpu::test_adapter_available() { return (false, String::new()); }
    static RUNS: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let n = RUNS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("rune-close-guard-{}-{n}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let (script_path, tree_path, png) = (dir.join("script.json"), dir.join("tree.json"), dir.join("out.png"));
    std::fs::write(&script_path, script).unwrap();
    // The backdrop a `--desktop` run draws its windows over: one pixel.
    std::fs::write(&png, [0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44, 0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1f, 0x15, 0xc4, 0x89, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x44, 0x41, 0x54, 0x78, 0xda, 0x63, 0x64, 0x60, 0xf8, 0x5f, 0x0f, 0x00, 0x02, 0x87, 0x01, 0x80, 0xeb, 0x47, 0xba, 0x92, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4e, 0x44, 0xae, 0x42, 0x60, 0x82]).unwrap();
    let spec = AppSpec {
        on_key: Some(on_key),
        ..AppSpec::new("close-guard", (200.0, 100.0), State::default, view)
    };
    let app = App::new(spec);
    let app = if keep_running { app.keep_running() } else { app };
    app
        .init(move || State { guarded, defers, reclose, log: Vec::new() })
        .on_close_request(|s, _| {
            s.log.push("request");
            if s.defers { CloseResponse::Defer } else { CloseResponse::Close }
        })
        .on_close_guard(|s| s.guarded || s.reclose, |s, tx| {
            s.log.push("ask");
            if s.reclose { tx.close_window(); }
        })
        .on_close(|s, _| s.log.push("close"))
        .args(["--script", script_path.to_str().unwrap(), "--tree", tree_path.to_str().unwrap()].into_iter().chain(desktop.then(|| ["--desktop", png.to_str().unwrap()]).into_iter().flatten()).map(String::from))
        .try_run()
        .unwrap();
    let tree: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&tree_path).unwrap()).unwrap();
    std::fs::remove_dir_all(&dir).unwrap();
    let log = tree["elements"].as_array().unwrap().iter().find(|e| e["key"] == "log").unwrap();
    (tree["closed"] == true, log["text"].as_str().unwrap().to_owned())
}

#[test]
fn the_close_button_asks_the_guard_after_on_close_request() {
    if !rune::render::Gpu::test_adapter_available() { return; }
    let script = r#"[{"t": 0.1, "close": true}]"#;
    assert_eq!(run(script, true, false), (false, "request,ask".into()));
    assert_eq!(run(script, false, false), (true, "request,close".into()));
}

#[test]
fn a_user_close_request_asks_the_guard_after_on_close_request() {
    if !rune::render::Gpu::test_adapter_available() { return; }
    let script = r#"[{"t": 0.1, "close_request": true}]"#;
    assert_eq!(run(script, true, false), (false, "request,ask".into()));
    assert_eq!(run(script, false, false), (true, "request,close".into()));
}

#[test]
fn tx_close_window_asks_the_guard_and_skips_on_close_request() {
    if !rune::render::Gpu::test_adapter_available() { return; }
    let script = r#"[{"t": 0.1, "type": "type", "text": "c"}]"#;
    assert_eq!(run(script, true, false), (false, "key,ask".into()));
    assert_eq!(run(script, false, false), (true, "key,close".into()));
}

#[test]
fn a_deferred_close_request_stands_with_the_guard_off() {
    if !rune::render::Gpu::test_adapter_available() { return; }
    for script in [r#"[{"t": 0.1, "close": true}]"#, r#"[{"t": 0.1, "close_request": true}]"#] {
        assert_eq!(run(script, false, true), (false, "request".into()), "{script}");
        assert_eq!(run(script, true, true), (false, "request".into()), "{script}");
    }
}

#[test]
fn tx_quit_is_not_held_by_the_guard() {
    if !rune::render::Gpu::test_adapter_available() { return; }
    // The guard is installed and holds: the close at the end asks. The Tx::quit before it did not.
    let script = r#"[{"t": 0.1, "type": "type", "text": "q"}, {"t": 0.3, "close": true}]"#;
    assert_eq!(run(script, true, false), (false, "key,request,ask".into()));
}

#[test]
fn a_window_that_ends_the_app_closes_once() {
    if !rune::render::Gpu::test_adapter_available() { return; }
    let script = r#"[{"t": 0.1, "close": true}, {"t": 0.3, "close": true}]"#;
    assert_eq!(run(script, false, false), (true, "request,close".into()));
}

#[test]
fn a_keep_running_window_closes_again_after_a_reopen() {
    if !rune::render::Gpu::test_adapter_available() { return; }
    let script = r#"[{"t": 0.1, "close": true}, {"t": 0.2, "reopen": true}, {"t": 0.3, "close": true}]"#;
    assert_eq!(run_app(script, false, false, true), (true, "request,close,request,close".into()));
    // Between the two the window was open again.
    let script = r#"[{"t": 0.1, "close": true}, {"t": 0.2, "reopen": true}]"#;
    assert_eq!(run_app(script, false, false, true), (false, "request,close".into()));
}

#[test]
fn an_ask_that_asks_to_close_again_asks_once_per_close() {
    if !rune::render::Gpu::test_adapter_available() { return; }
    for desktop in [false, true] {
        // A `--desktop` run has no close button to press (`{"close": true}` dies there by design).
        let scripts = [r#"[{"t": 0.1, "type": "type", "text": "c"}]"#, r#"[{"t": 0.1, "close": true}]"#];
        for script in &scripts[..if desktop { 1 } else { 2 }] {
            let (closed, log) = run_reclose(script, desktop);
            assert!(!closed, "desktop {desktop} {script}");
            let asks = log.matches("ask").count();
            assert_eq!(asks, 1, "desktop {desktop} {script}: {log}");
        }
    }
}

#[test]
fn a_keep_running_window_stays_closed_until_it_reopens() {
    if !rune::render::Gpu::test_adapter_available() { return; }
    let script = r#"[{"t": 0.1, "close": true}, {"t": 0.3, "close": true}]"#;
    assert_eq!(run_app(script, false, false, true), (true, "request,close".into()));
}

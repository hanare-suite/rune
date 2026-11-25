//! `App::on_close_guard` in a headless `--script` run: the close button (`{"close": true}`), a
//! user close request (`{"close_request": true}`) and `Tx::close_window` all ask the guard
//! while it says true, in the order close request, guard, `on_close`; `Tx::quit` is not held.
use rune::*;

#[derive(Default)]
struct State { guarded: bool, defers: bool, log: Vec<&'static str> }

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

/// As `run`, for an app that `keep_running` or not.
fn run_app(script: &str, guarded: bool, defers: bool, keep_running: bool) -> (bool, String) {
    if !rune::render::Gpu::test_adapter_available() { return (false, String::new()); }
    static RUNS: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let n = RUNS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("rune-close-guard-{}-{n}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let (script_path, tree_path) = (dir.join("script.json"), dir.join("tree.json"));
    std::fs::write(&script_path, script).unwrap();
    let spec = AppSpec {
        on_key: Some(on_key),
        ..AppSpec::new("close-guard", (200.0, 100.0), State::default, view)
    };
    let app = App::new(spec);
    let app = if keep_running { app.keep_running() } else { app };
    app
        .init(move || State { guarded, defers, log: Vec::new() })
        .on_close_request(|s, _| {
            s.log.push("request");
            if s.defers { CloseResponse::Defer } else { CloseResponse::Close }
        })
        .on_close_guard(|s| s.guarded, |s, _| s.log.push("ask"))
        .on_close(|s, _| s.log.push("close"))
        .args(["--script", script_path.to_str().unwrap(), "--tree", tree_path.to_str().unwrap()].map(String::from))
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

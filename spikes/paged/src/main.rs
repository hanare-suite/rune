//! `cargo run -p paged-spike --bin paged`, or headless with `--script`, `--tree`, `--snap`.
//! `PAGED_DOC` picks the document (`docs::from_env`), `PAGED_WORDS` the generated one's length
//! in words (25,000 by default). `PAGED_CARET` opens it focused, with the caret before the
//! first place its text occurs. `PAGED_KEEP=0` turns widow and orphan control off, and
//! `PAGED_FACTS` shows the line of facts the check scripts read.

use paged_spike::{State, WINDOW, docs, view};
use rune::AppSpec;

fn open() -> State {
    if std::env::var("PAGED_KEEP").is_ok_and(|v| v == "0") {
        paged_spike::KEEP_LINES.store(false, std::sync::atomic::Ordering::Relaxed);
    }
    let mut state = State::new(docs::from_env());
    state.facts = std::env::var("PAGED_FACTS").is_ok();
    if let Some(at) = std::env::var("PAGED_CARET").ok().and_then(|find| state.edit.text.find(&find)) {
        state.edit.set_caret(at);
        state.edit.focused = true;
    }
    state
}

fn main() {
    rune::run(AppSpec { on_key: Some(paged_spike::app_key), fonts: paged_spike::FONTS, ..AppSpec::new("Paged", WINDOW, open, view) });
}

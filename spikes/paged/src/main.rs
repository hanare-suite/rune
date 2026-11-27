//! `cargo run -p paged-spike --bin paged`, or headless with `--script`, `--tree`, `--snap`.
//! `PAGED_WORDS` sets the document's length in words (25,000 by default). `PAGED_CARET` opens
//! the document focused, with the caret before the first place its text occurs.

use paged_spike::{State, WINDOW, generate, view};
use rune::AppSpec;

fn open() -> State {
    let words = std::env::var("PAGED_WORDS").ok().and_then(|w| w.parse().ok()).unwrap_or(25_000);
    let mut state = State::new(generate(words));
    if let Some(at) = std::env::var("PAGED_CARET").ok().and_then(|find| state.edit.text.find(&find)) {
        state.edit.set_caret(at);
        state.edit.focused = true;
    }
    state
}

fn main() {
    rune::run(AppSpec::new("Paged", WINDOW, open, view));
}

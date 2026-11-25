//! `App::on_close_guard` in a native window: a guarded close stays open and asks.
use winit::event_loop::ActiveEventLoop;

use super::{Host, WindowApp};

/// What a quit asked of a guarded window.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::window) enum Hold {
    /// The window lets the quit go on.
    No,
    /// It holds, and its question is already up.
    Held,
    /// It holds, and has just asked.
    Asked,
}

/// What a window that stays open still has to do once its hooks ran.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::window) enum Settle {
    Stop,
    Paint,
    Covered,
    Wakes,
}

pub(in crate::window) fn settle_plan(ended: bool, window: Option<bool>) -> Settle {
    match (ended, window) {
        (true, _) => Settle::Stop,
        (false, Some(true)) => Settle::Paint,
        (false, Some(false)) => Settle::Covered,
        (false, None) => Settle::Wakes,
    }
}

/// The host-free core of `Host::holds_close`.
pub(in crate::window) fn hold_close_core(
    app: &mut dyn WindowApp,
    clipboard: &mut crate::clipboard::Client,
    closes: bool,
    exiting: bool,
    ends_app: &mut bool,
) -> bool {
    if exiting || !closes || !app.hold_close() { return false; }
    *ends_app = false;
    clipboard.flush_core(app.rt_mut());
    true
}

/// The host-free core of a quit asking a guarded window.
pub(in crate::window) fn hold_quit_core(
    _app: &mut dyn WindowApp,
    _clipboard: &mut crate::clipboard::Client,
    _closes: bool,
    _ends_app: &mut bool,
    _asked: &mut bool,
    _once: bool,
) -> Hold {
    Hold::No
}

impl Host {
    /// A guarded window stays open and asks (`App::on_close_guard`), unless the loop is exiting.
    /// True when it held the close.
    pub(in crate::window) fn holds_close(&mut self, event_loop: &ActiveEventLoop) -> bool {
        if event_loop.exiting() || !self.closes() || !self.app.hold_close() { return false; }
        self.ends_app = false;
        self.clipboard.flush_core(self.app.rt_mut());
        self.settle_open(event_loop);
        true
    }

    /// What a window that stays open after a close was asked of it still has to do: carry out the
    /// requests its hook made (`Host::shell`), then paint them, or in a covered or minimized
    /// window give them a covered frame, or with no window shown schedule their wakes.
    pub(in crate::window) fn settle_open(&mut self, event_loop: &ActiveEventLoop) {
        self.shell(event_loop);
        if self.life.finished() || event_loop.exiting() {
            return;
        }
        if let Some(w) = self.shown_win() {
            if !self.life.covered() && !w.minimized() {
                w.request_redraw();
            } else {
                self.covered_frame();
                self.shell(event_loop);
            }
        } else {
            self.app.rt_mut().schedule_pending_wakes();
        }
    }
}

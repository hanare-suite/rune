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

/// Whether a quit asks this window: any window not gone, a hidden `keep_running` one too.
pub(in crate::window) fn quit_asks(life: crate::window::Life) -> bool {
    !life.finished()
}

/// Whether the close of this window is held for other windows' guards: it ends the app (`ends`:
/// it keeps the loop and is not `keep_running`) with no other main window left, and another open
/// window holds back its close.
#[allow(dead_code)]
pub(in crate::window) fn holds_app_end(_closes: bool, _exiting: bool, _ends: bool, _other_mains: usize, _others_guarded: usize) -> bool {
    false
}

/// The host-free core of `Host::holds_close`: asks the guard of a window that closes, unless the
/// loop is exiting. True when it holds (`CloseGuard::holds` drops a close the ask leaves behind).
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

/// The host-free core of a quit asking a guarded window (`open`: `quit_asks`, `shown`: the window
/// shows). With `once` (the app's end held for a window) the window asks once, and the end stays
/// dropped, however the answer goes, for as long as the window shows or its guard holds; a window
/// that closed (a `keep_running` one hides) with its guard down holds nothing. Without `once`
/// every quit asks (a macOS Quit).
pub(in crate::window) fn hold_quit_core(
    app: &mut dyn WindowApp,
    clipboard: &mut crate::clipboard::Client,
    open: bool,
    shown: bool,
    ends_app: &mut bool,
    asked: &mut bool,
    once: bool,
) -> Hold {
    if !open {
        *asked = false;
        return Hold::No;
    }
    let guarded = app.close_guarded();
    if !shown && !guarded {
        *asked = false;
        return Hold::No;
    }
    if once && *asked { return Hold::Held; }
    if !guarded { return Hold::No; }
    *asked |= once;
    if hold_close_core(app, clipboard, open, false, ends_app) { Hold::Asked } else { Hold::No }
}

impl Host {
    /// A guarded window stays open and asks (`App::on_close_guard`), unless the loop is exiting.
    /// True when it held the close.
    pub(in crate::window) fn holds_close(&mut self, event_loop: &ActiveEventLoop) -> bool {
        let closes = self.closes();
        if !hold_close_core(&mut *self.app, &mut self.clipboard, closes, event_loop.exiting(), &mut self.ends_app) {
            return false;
        }
        self.settle_open(event_loop);
        true
    }

    /// A quit asks the window (`App::on_close_guard`); its follow-up comes before the turn ends (`about_to_wait`).
    pub(in crate::window) fn holds_quit(&mut self, once: bool) -> bool {
        let open = quit_asks(self.life);
        let shown = self.life.shown();
        match hold_quit_core(&mut *self.app, &mut self.clipboard, open, shown, &mut self.ends_app, &mut self.quit_asked, once) {
            Hold::No => false,
            Hold::Held => true,
            Hold::Asked => { self.settle_due = true; true }
        }
    }

    /// What a window that stays open after a close was asked of it still has to do: carry out the
    /// requests its hook made (`Host::shell`), then paint them, or in a covered or minimized
    /// window give them a covered frame, or with no window shown schedule their wakes.
    pub(in crate::window) fn settle_open(&mut self, event_loop: &ActiveEventLoop) {
        self.settle_due = false;
        self.shell(event_loop);
        let window = self.shown_win().map(|w| !self.life.covered() && !w.minimized());
        match settle_plan(self.life.finished() || event_loop.exiting(), window) {
            Settle::Stop => {}
            Settle::Paint => if let Some(w) = self.shown_win() { w.request_redraw(); },
            Settle::Covered => {
                self.covered_frame();
                self.shell(event_loop);
            }
            Settle::Wakes => self.app.rt_mut().schedule_pending_wakes(),
        }
    }

    /// Tells `crate::quit` whether this window holds back a quit, after every handler, so a state
    /// change and a Quit in one pass meet the same answer.
    pub(in crate::window) fn note_guard(&mut self) {
        let now = quit_asks(self.life) && self.app.close_guarded();
        if now != self.guarded {
            self.guarded = now;
            crate::quit::guarded_by(now);
        }
    }
}

impl Drop for Host {
    fn drop(&mut self) {
        if self.guarded { crate::quit::guarded_by(false); }
    }
}

//! `App::on_close_guard` in a native window: a guarded close stays open and asks.
use winit::event_loop::ActiveEventLoop;

use super::{Host, WindowApp};

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
pub(in crate::window) fn holds_app_end(closes: bool, exiting: bool, ends: bool, other_mains: usize, others_guarded: usize) -> bool {
    closes && !exiting && ends && other_mains == 0 && others_guarded > 0
}

/// The host-free core of `Host::holds_close`: asks the guard of a window that closes, unless the
/// loop is exiting. True when it holds (`CloseGuard::holds` drops a close the ask leaves behind).
/// A quit asks an open window the same way (`closes`: `quit_asks`).
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

impl Host {
    /// A guarded window stays open and asks (`App::on_close_guard`), unless the loop is exiting.
    /// So does the last main window that would end the app while another window holds back its
    /// close: it stays open, and the windows that hold ask instead, as for a macOS Quit
    /// (`holds_app_end`). True when it held the close.
    pub(in crate::window) fn holds_close(&mut self, event_loop: &ActiveEventLoop) -> bool {
        let closes = self.closes();
        let exiting = event_loop.exiting();
        if !hold_close_core(&mut *self.app, &mut self.clipboard, closes, exiting, &mut self.ends_app) && !self.holds_for_others(closes, exiting) {
            return false;
        }
        self.settle_open(event_loop);
        true
    }

    /// The close of the last main window, held for the windows that hold back theirs: they ask at
    /// the loop's next turn, as for a macOS Quit (`crate::quit::take_held`).
    fn holds_for_others(&mut self, closes: bool, exiting: bool) -> bool {
        let ends = self.keeps_loop && !self.keep_running;
        let (mains, others) = (crate::quit::other_mains(self.main), crate::quit::other_guarded(self.guarded));
        if !holds_app_end(closes, exiting, ends, mains, others) { return false; }
        self.ends_app = false;
        crate::quit::hold();
        true
    }

    /// A quit asks the window (`App::on_close_guard`); its follow-up comes before the turn ends (`turn_ends`).
    pub(in crate::window) fn holds_quit(&mut self) -> bool {
        let open = quit_asks(self.life);
        let held = hold_close_core(&mut *self.app, &mut self.clipboard, open, false, &mut self.ends_app);
        self.settle_due |= held;
        held
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

    /// Tells `crate::quit` whether this window holds back a quit and whether it is a main window
    /// still open, after every handler, so a state change and a Quit in one pass meet the same answer.
    pub(in crate::window) fn note_guard(&mut self) {
        let guarded = quit_asks(self.life) && self.app.close_guarded();
        if guarded != self.guarded {
            self.guarded = guarded;
            crate::quit::guarded_by(guarded);
        }
        let main = self.keeps_loop && !self.life.finished();
        if main != self.main {
            self.main = main;
            crate::quit::main_open(main);
        }
    }
}

impl Drop for Host {
    fn drop(&mut self) {
        if self.guarded { crate::quit::guarded_by(false); }
        if self.main { crate::quit::main_open(false); }
    }
}

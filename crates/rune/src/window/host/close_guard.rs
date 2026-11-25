//! `App::on_close_guard` in a native window: a guarded close stays open and asks.
use winit::event_loop::ActiveEventLoop;

use super::Host;

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

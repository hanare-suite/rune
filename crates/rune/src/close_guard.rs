//! `App::on_close_guard`: a window that holds back its closes (and a macOS Quit) while it has
//! unsaved changes. The guard lives in the app's `Shared`, so every host reaches it the same way.
use std::sync::Mutex;

use crate::app::App;
use crate::el::Tx;

/// `App::on_close_guard`.
pub(crate) struct CloseGuard<S> {
    pub(crate) guarded: fn(&S) -> bool,
    pub(crate) ask: fn(&mut S, &mut Tx),
}

impl<S> Clone for CloseGuard<S> {
    fn clone(&self) -> Self { *self }
}

impl<S> Copy for CloseGuard<S> {}

impl<S: 'static> CloseGuard<S> {
    /// Whether the window holds its close back now.
    pub(crate) fn is_guarded(self, state: &S) -> bool {
        #[cfg(feature = "hot")]
        return crate::hot::patched_or_original!("close_guard::guarded", self.guarded, state);
        #[cfg(not(feature = "hot"))]
        (self.guarded)(state)
    }

    /// Runs `ask` when the window is guarded. True when it held the close.
    pub(crate) fn holds(self, rt: &mut crate::Runtime<S>) -> bool {
        let held = self.is_guarded(&rt.state);
        if held {
            #[cfg(feature = "hot")]
            rt.update(|s, tx| crate::hot::patched_or_original!("close_guard::ask", self.ask, s, tx));
            #[cfg(not(feature = "hot"))]
            rt.update(self.ask);
        }
        // An `ask` that leaves the guard up and asks to close the window all the same would only
        // ask again: that close is dropped, in every host.
        if held && self.is_guarded(&rt.state) && rt.core().shell_request == Some(crate::el::Shell::Close) {
            rt.core_mut().shell_request = None;
        }
        held
    }
}

/// Where `Shared` keeps the guard: set once by the builder, read by whichever host closes.
pub(crate) struct Slot<S>(Mutex<Option<CloseGuard<S>>>);

impl<S> Default for Slot<S> {
    fn default() -> Self { Slot(Mutex::new(None)) }
}

impl<S> Slot<S> {
    pub(crate) fn set(&self, guard: CloseGuard<S>) { *self.0.lock().unwrap() = Some(guard); }

    pub(crate) fn get(&self) -> Option<CloseGuard<S>> { *self.0.lock().unwrap() }
}

impl<S: 'static> App<S> {
    /// Lets the window hold back every close and a macOS Quit, for unsaved changes. Before a
    /// close (the close button, ⌘W, `Tx::close_window`, `AppHandle::close`) and before a macOS
    /// Quit (⌘Q, the menu, the Dock, logout) Rune asks `guarded`. While it says true the window
    /// stays and `ask` runs instead, where the app puts its question up. To close after the
    /// answer, make `guarded` say false and call `Tx::close_window`. A quit that was held back is
    /// not taken up again: closing the window ends an app that does not keep running.
    /// `Tx::quit`, `AppHandle::quit` and the end of a Windows session are not held back.
    /// `guarded` runs often (after every turn of the event loop on macOS), so it only reads the
    /// state.
    ///
    /// On a user's close request the app's [`App::on_close_request`] runs first and a
    /// [`CloseResponse::Defer`](crate::CloseResponse::Defer) stands on its own; a `Close` goes on
    /// to ask `guarded`, and then to `on_close`. A close the app asks for skips
    /// `on_close_request` and meets the guard all the same, as does a macOS Quit. An `ask` that
    /// asks to close the window while `guarded` still says true is asked once: that close is
    /// dropped.
    ///
    /// When the close of the last main window would end the app and another window, open or a
    /// hidden `App::keep_running` one, is guarded, that close is held too: the main window stays,
    /// the guarded windows ask once, as for a held macOS Quit, and the question is not taken up
    /// again. After the answer, closing the main window again ends the app, or asks again if a
    /// guard still holds. A hidden `keep_running` window counts as guarded while `guarded` says
    /// true, for that close and for a macOS Quit: its `ask` runs, and showing the window is the
    /// app's job.
    pub fn on_close_guard(self, guarded: fn(&S) -> bool, ask: fn(&mut S, &mut Tx)) -> Self {
        self.shared.close_guard.set(CloseGuard { guarded, ask });
        self
    }
}

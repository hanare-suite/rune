//! `App::on_close_guard` in the native window's decisions: the host-free cores
//! (`hold_close_core`, `hold_quit_core`, `settle_plan`) and what the app does with them
//! (`hold_quit`, `cleanup_hosts`) for several windows.
use std::cell::RefCell;
use std::rc::Rc;

use winit::event::WindowEvent;
use winit::event_loop::ActiveEventLoop;
use winit::window::WindowId;

use super::{cleanup_hosts, hold_quit};
use crate::close_guard::CloseGuard;
use crate::quit::{Effect, Event, State as QuitState, step as quit_step};
use crate::runtime::{Env, Runtime};
use crate::window::host::app::{WindowApp, test_app_with_close_guard};
use crate::window::host::close_guard::{Hold, Settle, hold_close_core, hold_quit_core, settle_plan};
use crate::window::{AnyHost, Life, WakeEvent};
use crate::{Cx, El, Tx, el};

/// What `ask` does with the guard.
#[derive(Clone, Copy)]
enum Answer {
    /// The answer comes: the guard goes down.
    Lower,
    /// The question stays up: the guard holds.
    Keep,
    /// The guard holds and `ask` asks to close the window all the same.
    KeepAndClose,
    /// The guard goes down and `ask` closes the window, as an answer of "close anyway" does.
    LowerAndClose,
}

struct Doc {
    events: Rc<RefCell<Vec<String>>>,
    name: &'static str,
    guard_on: bool,
    answer: Answer,
}

fn view(_: &Doc, _: &mut Cx<Doc>) -> El { el() }
fn guard_is_on(s: &Doc) -> bool { s.guard_on }

fn ask(s: &mut Doc, tx: &mut Tx) {
    s.events.borrow_mut().push(format!("{}:ask", s.name));
    match s.answer {
        Answer::Lower => s.guard_on = false,
        Answer::Keep => {}
        Answer::KeepAndClose => tx.close_window(),
        Answer::LowerAndClose => { s.guard_on = false; tx.close_window(); }
    }
}

fn on_close(s: &mut Doc, _: &mut Tx) { s.events.borrow_mut().push(format!("{}:on_close", s.name)); }

fn on_close_request(s: &mut Doc, _: &mut Tx) -> crate::CloseResponse {
    s.events.borrow_mut().push(format!("{}:on_close_request", s.name));
    crate::CloseResponse::Close
}

/// A window with the real `TypedWindowApp`, `Life` and `clipboard::Client`, deciding as `Host` does.
struct Win {
    app: Box<dyn WindowApp>,
    clipboard: crate::clipboard::Client,
    life: Life,
    keeps_loop: bool,
    ends_app: bool,
    asked: bool,
}

impl Win {
    fn new(name: &'static str, keeps_loop: bool, guarded: Option<Answer>, events: &Rc<RefCell<Vec<String>>>, mem: &crate::clipboard::MemoryClipboard) -> Self {
        let state = Doc { events: events.clone(), name, guard_on: guarded.is_some(), answer: guarded.unwrap_or(Answer::Lower) };
        let mut rt = Runtime::new(state, view, (100.0, 100.0), Env::default());
        rt.frame(0.0);
        let guard = guarded.map(|_| CloseGuard { guarded: guard_is_on as fn(&Doc) -> bool, ask });
        let app = test_app_with_close_guard(rt, Some(on_close_request), guard, Some(on_close), None);
        let mut clipboard = crate::clipboard::Client::default();
        clipboard.use_memory(mem);
        let mut life = Life::Unopened;
        life.show();
        Self { app, clipboard, life, keeps_loop, ends_app: false, asked: false }
    }

    fn closes(&self) -> bool { self.life.closes(false, true) }

    fn hold_close(&mut self, exiting: bool) -> bool {
        let closes = self.closes();
        hold_close_core(&mut *self.app, &mut self.clipboard, closes, exiting, &mut self.ends_app)
    }
}

impl AnyHost for Win {
    fn resumed(&mut self, _: &ActiveEventLoop) {}
    fn user_event(&mut self, _: &ActiveEventLoop, _: WakeEvent) {}
    fn owns(&self, _: WindowId) -> bool { false }
    fn window_event(&mut self, _: &ActiveEventLoop, _: WindowId, _: WindowEvent) {}
    fn finished(&self) -> bool { self.life.finished() }
    fn keeps_loop(&self) -> bool { self.keeps_loop }
    fn hold_quit(&mut self, once: bool) -> bool {
        let closes = self.closes();
        hold_quit_core(&mut *self.app, &mut self.clipboard, closes, &mut self.ends_app, &mut self.asked, once) != Hold::No
    }
    fn terminate(&mut self) {
        if !self.closes() { return; }
        self.life.finish();
        self.app.run_on_close();
    }
}

fn events() -> (Rc<RefCell<Vec<String>>>, crate::clipboard::MemoryClipboard) {
    (Rc::new(RefCell::new(Vec::new())), crate::clipboard::memory())
}

fn step_quit(state: &mut QuitState) -> impl FnMut(Event) -> Vec<Effect> + '_ {
    move |event| { let (next, effects) = quit_step(*state, event); *state = next; effects }
}

#[test]
fn a_close_asks_the_guard_after_on_close_request_and_only_while_it_holds() {
    let (log, mem) = events();
    let mut win = Win::new("w", true, Some(Answer::Lower), &log, &mem);
    win.ends_app = true;
    // As `Host::close_requested` goes: the hook first, then the close meets the guard.
    let closes = win.closes();
    let mut ends_app = win.ends_app;
    let decision = super::super::host::lifecycle::close_request_core(&mut *win.app, &mut win.clipboard, closes, false, &mut ends_app);
    assert_eq!(decision, super::super::host::lifecycle::CloseDecision::Close);
    win.ends_app = ends_app;
    assert!(win.hold_close(false), "the guard holds the close");
    assert!(!win.ends_app, "a held close leaves no fast exit behind");
    assert_eq!(*log.borrow(), vec!["w:on_close_request", "w:ask"]);
    assert!(!win.hold_close(false), "the answer lowered the guard: the close goes on");
    assert_eq!(*log.borrow(), vec!["w:on_close_request", "w:ask"], "and nothing asks twice");
}

#[test]
fn a_close_is_not_held_by_a_closed_window_or_an_exiting_loop() {
    let (log, mem) = events();
    let mut win = Win::new("w", true, Some(Answer::Keep), &log, &mem);
    assert!(!win.hold_close(true), "the loop is exiting");
    win.life.finish();
    assert!(!win.hold_close(false), "the window is gone");
    assert!(log.borrow().is_empty());
}

#[test]
fn an_ask_that_closes_the_window_while_the_guard_holds_is_asked_once_and_the_close_dropped() {
    let (log, mem) = events();
    let mut win = Win::new("w", true, Some(Answer::KeepAndClose), &log, &mem);
    assert!(win.hold_close(false));
    assert_eq!(*log.borrow(), vec!["w:ask"]);
    assert_eq!(win.app.rt().shell_request, None, "a close that would only ask again is dropped");
}

#[test]
fn an_ask_that_lowers_the_guard_and_closes_the_window_keeps_its_close() {
    let (log, mem) = events();
    let mut win = Win::new("w", true, Some(Answer::LowerAndClose), &log, &mem);
    assert!(win.hold_close(false));
    assert_eq!(win.app.rt().shell_request, Some(crate::el::Shell::Close), "the answer's own close goes on to close");
    assert!(!win.hold_close(false), "and meets no guard");
}

#[test]
fn a_held_close_settles_by_what_the_window_can_paint() {
    assert_eq!(settle_plan(true, Some(true)), Settle::Stop);
    assert_eq!(settle_plan(false, Some(true)), Settle::Paint);
    assert_eq!(settle_plan(false, Some(false)), Settle::Covered, "a covered or minimized window gets a covered frame");
    assert_eq!(settle_plan(false, None), Settle::Wakes, "no window shown schedules the wakes");
}

#[test]
fn a_macos_quit_asks_every_guarded_window_each_time() {
    let (log, mem) = events();
    let mut hosts: Vec<Option<Box<dyn AnyHost>>> = vec![
        Some(Box::new(Win::new("a", true, Some(Answer::Keep), &log, &mem))),
        Some(Box::new(Win::new("plain", false, None, &log, &mem))),
        Some(Box::new(Win::new("b", false, Some(Answer::Keep), &log, &mem))),
    ];
    assert!(hold_quit(&mut hosts, false));
    assert_eq!(*log.borrow(), vec!["a:ask", "b:ask"]);
    assert!(hold_quit(&mut hosts, false), "a second Quit asks again");
    assert_eq!(*log.borrow(), vec!["a:ask", "b:ask", "a:ask", "b:ask"]);
    let (log, mem) = events();
    let mut hosts: Vec<Option<Box<dyn AnyHost>>> = vec![Some(Box::new(Win::new("plain", true, None, &log, &mem)))];
    assert!(!hold_quit(&mut hosts, false), "no guarded window lets the Quit go on");
}

#[test]
fn the_last_main_window_closing_asks_a_guarded_secondary_before_the_app_ends() {
    let (log, mem) = events();
    let mut main = Win::new("main", true, None, &log, &mem);
    main.life.finish();
    let mut hosts: Vec<Option<Box<dyn AnyHost>>> = vec![
        Some(Box::new(main)),
        Some(Box::new(Win::new("second", false, Some(Answer::Keep), &log, &mem))),
    ];
    let mut quit = QuitState::Running;
    assert!(!cleanup_hosts(&mut hosts, &mut Vec::new(), true, false, false, step_quit(&mut quit)), "the app stays");
    assert_eq!(*log.borrow(), vec!["second:ask"]);
    assert!(!cleanup_hosts(&mut hosts, &mut Vec::new(), true, false, false, step_quit(&mut quit)));
    assert_eq!(*log.borrow(), vec!["second:ask"], "the question is up: it is not asked again");
    assert!(hosts[1].as_ref().is_some_and(|h| !h.finished()), "and the window is not closed under it");
}

#[test]
fn the_app_ends_once_the_guarded_secondary_is_answered() {
    let (log, mem) = events();
    let mut main = Win::new("main", true, None, &log, &mem);
    main.life.finish();
    let mut hosts: Vec<Option<Box<dyn AnyHost>>> = vec![
        Some(Box::new(main)),
        Some(Box::new(Win::new("second", false, Some(Answer::Lower), &log, &mem))),
    ];
    let mut quit = QuitState::Running;
    assert!(!cleanup_hosts(&mut hosts, &mut Vec::new(), true, false, false, step_quit(&mut quit)));
    assert!(cleanup_hosts(&mut hosts, &mut Vec::new(), true, false, false, step_quit(&mut quit)), "unguarded now: the app ends");
    assert_eq!(*log.borrow(), vec!["second:ask", "second:on_close"]);
}

#[test]
fn an_unguarded_secondary_closes_with_the_last_main_window_as_before() {
    let (log, mem) = events();
    let mut main = Win::new("main", true, None, &log, &mem);
    main.life.finish();
    let mut hosts: Vec<Option<Box<dyn AnyHost>>> = vec![
        Some(Box::new(main)),
        Some(Box::new(Win::new("second", false, None, &log, &mem))),
    ];
    let mut quit = QuitState::Running;
    assert!(cleanup_hosts(&mut hosts, &mut Vec::new(), true, false, false, step_quit(&mut quit)));
    assert_eq!(*log.borrow(), vec!["second:on_close"]);
}

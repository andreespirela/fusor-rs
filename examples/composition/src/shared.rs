use fusor::{ContextKey, Signal};
use std::{cell::Cell, rc::Rc};

pub struct Theme;
impl ContextKey for Theme {
    type Value = Signal<String>;
}

/// Counters make instance and async lifetimes observable in the browser tests.
#[derive(Default)]
pub struct Metrics {
    pub created: Cell<u32>,
    pub dropped: Cell<u32>,
    pub started: Cell<u32>,
    pub cancelled: Cell<u32>,
    pub futures_dropped: Cell<u32>,
    pub computations: Cell<u32>,
}
impl ContextKey for Metrics {
    type Value = Self;
}

pub fn increment(counter: &Cell<u32>) {
    counter.set(counter.get() + 1);
}

pub struct PendingRead(pub Rc<Metrics>);
impl Drop for PendingRead {
    fn drop(&mut self) {
        increment(&self.0.futures_dropped);
    }
}

use crate::counter::Counter;
use fusor::prelude::*;

struct App {
    count: Signal<i32>,
}

impl App {
    fn new() -> Self {
        Self { count: signal(0) }
    }
}

fusor::template!("web/index.html");

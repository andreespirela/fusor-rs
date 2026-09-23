use fusor::{Signal, signal};

struct App {
    name: Signal<String>,
    count: Signal<i32>,
}

impl App {
    fn new() -> Self {
        Self {
            name: signal("Ada".into()),
            count: signal(0),
        }
    }
}

fusor::template!("web/index.html");

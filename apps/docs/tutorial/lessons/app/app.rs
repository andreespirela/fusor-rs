use fusor::{Signal, signal};

struct Dashboard {
    count: Signal<u32>,
}

fn create_dashboard() -> Dashboard {
    Dashboard { count: signal(0) }
}

fusor::template!("web/index.html");

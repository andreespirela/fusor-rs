use fusor::{Signal, signal};

#[derive(Clone, PartialEq)]
struct Item {
    id: u32,
    title: String,
}

struct App {
    items: Signal<Vec<Item>>,
}

impl App {
    fn new() -> Self {
        Self {
            items: signal(vec![
                Item { id: 1, title: "Read the guide".into() },
                Item { id: 2, title: "Build a page".into() },
            ]),
        }
    }

    fn remove(&self, id: u32) {
        self.items.update(|items| items.retain(|item| item.id != id));
    }
}

fusor::template!("web/index.html");

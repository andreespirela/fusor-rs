use crate::{
    details::Details,
    pricing::{Price, Stock},
    reader::Reader,
    rows::{Item, Row},
    watch::Watch,
};
use fusor::{Signal, signal};
use fusor_async::AsyncBoundary;

struct App {
    selected_id: Signal<u32>,
    expanded: Signal<bool>,
    items: Signal<Vec<Item>>,
    watching: Signal<bool>,
    lifecycle: Signal<String>,
    show_reader: Signal<bool>,
    product: Signal<String>,
    view: AsyncBoundary,
}

impl App {
    fn new() -> Self {
        Self {
            selected_id: signal(1),
            expanded: signal(true),
            items: signal(vec![
                Item {
                    id: 1,
                    title: "First issue".into(),
                },
                Item {
                    id: 2,
                    title: "Second issue".into(),
                },
            ]),
            watching: signal(true),
            lifecycle: signal("Not mounted".into()),
            show_reader: signal(true),
            product: signal("A".into()),
            view: AsyncBoundary::coherent(),
        }
    }

    fn rename_first(&self) {
        self.items.update(|items| {
            if let Some(item) = items.iter_mut().find(|item| item.id == 1) {
                item.title = "Renamed first issue".into();
            }
        });
    }
}

fusor::bindings!(app);

use crate::panels::{CoherentPanel, Price, Row, RowData, Stock};
use fusor::{Signal, signal};
use fusor_async::AsyncBoundary;

pub struct App {
    selected: Signal<String>,
    show_stock: Signal<bool>,
    stock_version: Signal<u32>,
    locale: Signal<String>,
    view: AsyncBoundary,
    clicks: Signal<usize>,
    inert: Signal<bool>,
    rows: Signal<Vec<RowData>>,
    mounts: Signal<usize>,
}
impl App {
    fn new() -> Self {
        Self {
            selected: signal("A".into()),
            show_stock: signal(true),
            stock_version: signal(0),
            locale: signal("en".into()),
            view: AsyncBoundary::coherent(),
            clicks: signal(0),
            inert: signal(false),
            rows: signal(vec![
                RowData {
                    id: 1,
                    label: "one".into(),
                },
                RowData {
                    id: 2,
                    label: "two".into(),
                },
            ]),
            mounts: signal(0),
        }
    }

    fn change_rows(&self) {
        self.rows.set(vec![
            RowData {
                id: 2,
                label: "second".into(),
            },
            RowData {
                id: 3,
                label: "third".into(),
            },
        ]);
    }
}
fusor::bindings!(app);

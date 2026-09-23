use fusor::{Signal, signal};

#[derive(Clone, PartialEq)]
struct Book {
    id: u32,
    title: String,
}

pub struct Keyed {
    books: Signal<Vec<Book>>,
    next_id: Signal<u32>,
}

impl Keyed {
    pub fn new() -> Self {
        Self {
            books: signal(vec![
                Book {
                    id: 1,
                    title: "The Rust Programming Language".into(),
                },
                Book {
                    id: 2,
                    title: "Programming WebAssembly".into(),
                },
                Book {
                    id: 3,
                    title: "Designing Data-Intensive Applications".into(),
                },
            ]),
            next_id: signal(4),
        }
    }

    fn add(&self) {
        let id = self.next_id.get();
        self.books.update(|books| {
            books.push(Book {
                id,
                title: format!("Reading pick {id}"),
            })
        });
        self.next_id.set(id + 1);
    }
}

impl Keyed {
    fn remove(&self, id: u32) {
        self.books
            .update(|books| books.retain(|book| book.id != id));
    }
}

fusor::bindings!(keyed);

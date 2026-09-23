use fusor::prelude::*;

#[derive(FromInputs)]
pub struct KeyedList {
    #[local(init = signal(vec![1_u32, 2, 3]))]
    rows: Signal<Vec<u32>>,
    #[local(init = signal(4_u32))]
    next_id: Signal<u32>,
}

impl KeyedList {
    fn add(&self) {
        let id = self.next_id.get();
        self.rows.update(|rows| rows.push(id));
        self.next_id.set(id + 1);
    }

    fn remove(&self, id: u32) {
        self.rows.update(|rows| rows.retain(|row| *row != id));
    }
}

fusor::template!("web/components/keyed_list.html");

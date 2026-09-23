use fusor::prelude::*;

#[derive(FromInputs)]
pub struct Counter {
    #[local(init = signal(0))]
    count: Signal<i32>,
}

impl Counter {
    fn increment(&self) {
        self.count.update(|n| *n += 1);
    }

    fn reset(&self) {
        self.count.set(0);
    }
}

fusor::template!("web/components/counter.html");

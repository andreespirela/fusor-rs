use fusor::prelude::*;

#[derive(FromInputs)]
pub struct Counter {
    #[local(init = signal(0))]
    count: Signal<i32>,
}

fusor::template!("web/components/counter.html");

use fusor::prelude::*;

#[derive(FromInputs)]
pub struct Counter {
    #[input]
    count: Signal<i32>,

    #[local(init = signal(0))]
    clicks: Signal<i32>,
}

fusor::template!("web/components/counter.html");

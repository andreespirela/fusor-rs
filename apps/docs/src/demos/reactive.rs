use fusor::{Memo, Signal, memo, signal};

pub struct Reactive {
    name: Signal<String>,
    seats: Signal<u32>,
    annual: Signal<bool>,
    total: Memo<u32>,
}

impl Reactive {
    pub fn new() -> Self {
        let seats = signal(2);
        let annual = signal(false);
        let total = {
            let seats = seats.clone();
            let annual = annual.clone();
            memo(move || seats.get() * if annual.get() { 20 } else { 24 })
        };
        Self {
            name: signal("Acme Studio".into()),
            seats,
            annual,
            total,
        }
    }
}

fusor::bindings!(reactive);

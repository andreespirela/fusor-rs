//! Reproducible native workloads, not a browser or cross-framework benchmark.
//! cargo run -p fusor --example memo_workload --release --locked
use fusor::{derived, effect, memo, signal};
use std::{cell::Cell, hint::black_box, rc::Rc, time::Instant};

fn run(name: &str, cached: bool, consumers: usize, updates: u64, work: u64, group: u64) {
    let input = signal(0_u64);
    let computations = Rc::new(Cell::new(0_u64));
    let renders = Rc::new(Cell::new(0_u64));
    let compute = {
        let (input, computations) = (input.clone(), computations.clone());
        move || {
            computations.set(computations.get() + 1);
            let mut n = input.get() / group;
            for _ in 0..work {
                n = black_box(n.wrapping_mul(6364136223846793005).wrapping_add(1));
            }
            n
        }
    };
    let read: Rc<dyn Fn() -> u64> = if cached {
        let value = memo(compute);
        Rc::new(move || value.get())
    } else {
        let value = derived(compute);
        Rc::new(move || value.get())
    };
    let subscriptions: Vec<_> = (0..consumers)
        .map(|_| {
            let (read, renders) = (read.clone(), renders.clone());
            effect(move || {
                black_box(read());
                renders.set(renders.get() + 1);
            })
        })
        .collect();
    computations.set(0);
    renders.set(0);
    let start = Instant::now();
    for value in 1..=updates {
        input.set(black_box(value));
    }
    let elapsed = start.elapsed().as_micros();
    let mode = if cached { "Memo" } else { "Derived" };
    println!(
        "{name},{mode},{consumers},{updates},{},{},{elapsed}",
        computations.get(),
        renders.get()
    );
    assert_eq!(
        computations.get(),
        updates * if cached { 1 } else { consumers as u64 }
    );
    assert_eq!(
        renders.get(),
        if cached { updates / group } else { updates } * consumers as u64
    );
    drop(subscriptions);
}

fn main() {
    println!("workload,mode,consumers,updates,computations,effects,microseconds");
    for cached in [false, true] {
        run("cheap scalar", cached, 1, 100_000, 0, 1);
        run("shared costly projection", cached, 8, 2_000, 10_000, 1);
        run("mostly equal projection", cached, 8, 20_000, 0, 100);
    }
}

use std::time::Instant;
fn main() {
    let args: Vec<_> = std::env::args().collect();
    let count = args.get(1).and_then(|n| n.parse().ok()).unwrap_or(1000);
    if args.get(2).is_some_and(|arg| arg == "html") {
        print!("{}", leptos_bench_workload::server_render(count));
        return;
    }
    for _ in 0..3 {
        std::hint::black_box(leptos_bench_workload::server_render(count));
    }
    let mut times = vec![];
    let mut bytes = 0;
    for _ in 0..15 {
        let start = Instant::now();
        let html = leptos_bench_workload::server_render(count);
        times.push(start.elapsed().as_secs_f64() * 1000.0);
        bytes = html.len();
        std::hint::black_box(html);
    }
    println!("{{\"framework\":\"leptos\",\"n\":{count},\"bytes\":{bytes},\"samples\":{times:?}}}");
}

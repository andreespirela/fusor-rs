#[cfg(not(target_arch = "wasm32"))]
fn main() {
    use fusor_server::{Context, Render};
    let html = fusor_control_flow::Shared::new()
        .render(&mut Context::new())
        .unwrap();
    println!("{html}");
}
#[cfg(target_arch = "wasm32")]
fn main() {}

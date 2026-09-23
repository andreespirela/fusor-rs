fn main() {
    println!(
        "cargo:rustc-env=FUSOR_HOST={}",
        std::env::var("TARGET").expect("Cargo target")
    );
}

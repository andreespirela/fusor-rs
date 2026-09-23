#[path = "build/highlight.rs"]
mod highlight;

use std::{env, fmt::Write, fs, path::Path};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("cargo:rerun-if-changed=build/highlight.rs");
    let highlighter = highlight::Highlighter::new();
    let mut generated = String::new();
    for example in ["counter", "search", "keyed_list", "async_data"] {
        for (language, source) in [
            ("rs", format!("src/{example}.rs")),
            ("html", format!("web/components/{example}.html")),
        ] {
            println!("cargo:rerun-if-changed={source}");
            let code = fs::read_to_string(&source)?;
            let name = format!("{example}.{language}");
            let href = format!("./source/{source}.txt");
            let tokens = highlighter.tokens(&code, language)?;
            let constant = format!("{}_{}", example.to_uppercase(), language.to_uppercase());
            writeln!(
                generated,
                "pub static {constant}: CodeFile = CodeFile {{ name: {name:?}, href: {href:?}, tokens: {tokens} }};"
            )?;

            let destination = Path::new("public/source").join(format!("{source}.txt"));
            // Watched too: a fresh checkout with a cached target/ lacks the copy.
            println!("cargo:rerun-if-changed={}", destination.display());
            fs::create_dir_all(destination.parent().unwrap())?;
            if fs::read_to_string(&destination).ok().as_deref() != Some(&code) {
                fs::write(destination, &code)?;
            }
        }
    }
    fs::write(
        Path::new(&env::var("OUT_DIR")?).join("highlighted.rs"),
        generated,
    )?;
    fusor_build::compile_app()
}

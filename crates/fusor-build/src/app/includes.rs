//! A source that includes a file cannot be reused across a development refresh,
//! except for the includes this compiler arranges itself: `fusor::template!`,
//! `fusor::bindings!` and the generated module. Those shapes are this crate's
//! contract; changing a constant below means changing the matching macro in
//! `crates/fusor-core/src/authoring.rs`.
use proc_macro2::{TokenStream, TokenTree};

pub(crate) const TEMPLATE_DIRECTORY: &str = "fusor_templates";

pub(crate) const BINDINGS_PREFIX: &str = "fusor_";

pub(crate) const MODULE_VARIABLE: &str = "FUSOR_MODULE";

/// `None` when the source does not tokenize; callers should assume it does.
pub fn includes_foreign_file(rust: &str) -> Option<bool> {
    rust.parse().ok().map(|stream| walk(&stream))
}

/// Matched as token streams, so formatting does not matter. The metavariables
/// appear because a path dependency on `fusor` puts the macro definitions
/// themselves in the scanned sources.
fn generated_contracts() -> [TokenStream; 3] {
    [
        format!(r#"concat!(env!("OUT_DIR"), "/{TEMPLATE_DIRECTORY}/", $path, ".rs")"#),
        format!(r#"concat!(env!("OUT_DIR"), "/{BINDINGS_PREFIX}", stringify!($name), ".rs")"#),
        format!(r#"env!("{MODULE_VARIABLE}")"#),
    ]
    .map(|contract| contract.parse().expect("generated include contract"))
}

fn walk(stream: &TokenStream) -> bool {
    let tokens: Vec<_> = stream.clone().into_iter().collect();
    for sequence in tokens.windows(3) {
        let [
            TokenTree::Ident(name),
            TokenTree::Punct(bang),
            TokenTree::Group(arguments),
        ] = sequence
        else {
            continue;
        };
        let is_include = bang.as_char() == '!'
            && matches!(
                name.to_string().as_str(),
                "include" | "include_str" | "include_bytes"
            );
        if is_include && !is_generated(name.to_string().as_str(), &arguments.stream()) {
            return true;
        }
    }
    tokens.iter().any(|token| match token {
        TokenTree::Group(group) => walk(&group.stream()),
        _ => false,
    })
}

fn is_generated(macro_name: &str, arguments: &TokenStream) -> bool {
    // This compiler emits Rust to include, never data to embed.
    if macro_name != "include" {
        return false;
    }
    let arguments = arguments.to_string();
    generated_contracts()
        .iter()
        .any(|contract| contract.to_string() == arguments)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_file_include_outside_the_generated_contract_prevents_reuse() {
        for source in [
            r#"const PAGE: &str = include_str!("index.html");"#,
            r#"fn f() { let _ = std::include_bytes!("public/data.bin"); }"#,
            r#"include!(concat!(env!("OUT_DIR"), "/custom.rs"));"#,
            r#"macro_rules! content { () => { include_str!("file") } }"#,
            r#"include!(concat!(env!("OUT_DIR"), "/custom_", stringify!($name), ".rs"));"#,
        ] {
            assert_eq!(includes_foreign_file(source), Some(true), "{source}");
        }
    }

    #[test]
    fn this_compilers_own_includes_do_not_prevent_reuse() {
        for source in [
            r#"include!(env!("FUSOR_MODULE"));"#,
            r#"macro_rules! bindings { ($name:ident) => { include!(concat!(env!("OUT_DIR"), "/fusor_", stringify!($name), ".rs")); }; }"#,
            r#"macro_rules! template { ($path:literal) => { include!(concat!(env!("OUT_DIR"), "/fusor_templates/", $path, ".rs")); }; }"#,
            "// include_str!(\"file\")\nfn f() {}",
            r#"const EXAMPLE: &str = "include_str!(file)";"#,
        ] {
            assert_eq!(includes_foreign_file(source), Some(false), "{source}");
        }
    }

    #[test]
    fn source_that_does_not_tokenize_has_no_answer() {
        assert_eq!(includes_foreign_file("fn f() { '"), None);
    }
}

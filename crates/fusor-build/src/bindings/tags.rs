//! Component invocations use Rust paths and ordinary named-field input structs.
use super::{
    interpolation::{exact_expression, interpolations},
    ir::*,
    tokens::Rust,
};
use crate::{ExtractError, error};
use fusor::template::MountId;
use html5gum::StartTag;
use quote::quote;

// html5gum intentionally folds HTML tag names. Read only the original name
// from the token's exact source range; Rust paths still go through syn/rustc.
pub(super) fn name(source: &str, offset: usize) -> &str {
    source[offset..]
        .trim_start_matches('<')
        .trim_start_matches('/')
        .split(|ch: char| ch.is_ascii_whitespace() || matches!(ch, '>' | '/'))
        .next()
        .unwrap_or("")
}

pub(super) fn is_component(name: &str) -> bool {
    if name.contains("::") {
        return true;
    }
    if name.contains('-') || !name.chars().next().is_some_and(char::is_uppercase) {
        return false;
    }
    // Preserve conventional ALL-UPPERCASE native HTML. PascalCase, including
    // Button and Input, is always a Rust component; lowercase tags remain HTML.
    if name.chars().any(char::is_lowercase) {
        return true;
    }
    !matches!(
        name.to_ascii_lowercase().as_str(),
        "a" | "abbr"
            | "acronym"
            | "address"
            | "applet"
            | "area"
            | "article"
            | "aside"
            | "audio"
            | "b"
            | "base"
            | "basefont"
            | "bdi"
            | "bdo"
            | "bgsound"
            | "big"
            | "blockquote"
            | "body"
            | "br"
            | "button"
            | "canvas"
            | "caption"
            | "center"
            | "cite"
            | "code"
            | "col"
            | "colgroup"
            | "data"
            | "datalist"
            | "dd"
            | "del"
            | "details"
            | "dfn"
            | "dialog"
            | "dir"
            | "div"
            | "dl"
            | "dt"
            | "em"
            | "embed"
            | "fieldset"
            | "figcaption"
            | "figure"
            | "font"
            | "footer"
            | "form"
            | "frame"
            | "frameset"
            | "h1"
            | "h2"
            | "h3"
            | "h4"
            | "h5"
            | "h6"
            | "head"
            | "header"
            | "hgroup"
            | "hr"
            | "html"
            | "i"
            | "iframe"
            | "img"
            | "input"
            | "ins"
            | "kbd"
            | "label"
            | "legend"
            | "li"
            | "link"
            | "listing"
            | "main"
            | "map"
            | "mark"
            | "marquee"
            | "math"
            | "menu"
            | "meta"
            | "meter"
            | "nav"
            | "nobr"
            | "noembed"
            | "noframes"
            | "noscript"
            | "object"
            | "ol"
            | "optgroup"
            | "option"
            | "output"
            | "p"
            | "param"
            | "picture"
            | "plaintext"
            | "pre"
            | "progress"
            | "q"
            | "rb"
            | "rp"
            | "rt"
            | "rtc"
            | "ruby"
            | "s"
            | "samp"
            | "script"
            | "search"
            | "section"
            | "select"
            | "slot"
            | "small"
            | "source"
            | "span"
            | "strike"
            | "strong"
            | "style"
            | "sub"
            | "summary"
            | "sup"
            | "svg"
            | "table"
            | "tbody"
            | "td"
            | "template"
            | "textarea"
            | "tfoot"
            | "th"
            | "thead"
            | "time"
            | "title"
            | "tr"
            | "track"
            | "tt"
            | "u"
            | "ul"
            | "var"
            | "video"
            | "wbr"
            | "xmp"
    )
}

pub(super) fn field(source: &str, name: &str, offset: usize) -> Result<Rust, ExtractError> {
    if !name
        .bytes()
        .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
        || syn::parse_str::<syn::Ident>(name).is_err()
    {
        return Err(error(
            source,
            offset,
            "component input names must be snake_case Rust field identifiers",
        ));
    }
    Rust::parse(source, name, offset)
}

pub(super) fn invocation(
    source: &str,
    tag: &StartTag<usize>,
    point: MountId,
) -> Result<Binding, ExtractError> {
    let name = name(source, tag.span.start);
    let path = syn::parse_str::<syn::Path>(name).map_err(|_| {
        error(
            source,
            tag.span.start,
            "component tags require a Rust type path or imported alias",
        )
    })?;
    if path
        .segments
        .iter()
        .any(|part| !matches!(part.arguments, syn::PathArguments::None))
    {
        return Err(error(
            source,
            tag.span.start,
            "use a Rust type alias for a generic component",
        ));
    }
    let ty = Rust::parse(source, name, tag.span.start)?;
    let mut inputs = Vec::new();
    let mut condition = None;
    let mut key = None;
    for (name, value) in &tag.attributes {
        let name = String::from_utf8_lossy(name);
        let value_text = String::from_utf8_lossy(value);
        let offset = value.span.start;
        let authored = source[offset..]
            .split(|ch: char| ch.is_ascii_whitespace() || matches!(ch, '=' | '/' | '>'))
            .next()
            .unwrap_or("");
        if authored != name.as_ref() {
            return Err(error(
                source,
                offset,
                "component input attributes must use their exact snake_case Rust spelling",
            ));
        }
        match name.as_ref() {
            "rust:if" => condition = Some(Rust::parse(source, &value_text, offset)?),
            "rust:key" => key = Some(Rust::parse(source, &value_text, offset)?),
            _ => {
                let name = field(source, &name, offset)?;
                let parts = interpolations(source, &value_text, offset, false)?;
                let value = if parts.is_empty() {
                    let text = value_text.as_ref();
                    Rust::parse(source, &quote! { #text }.to_string(), offset)?
                } else {
                    exact_expression(
                        source,
                        &value_text,
                        parts,
                        offset,
                        "component inputs require a literal string or exactly one {{ Rust value }}; use format! explicitly for formatted strings",
                    )?
                };
                inputs.push(Input {
                    name,
                    value: InputValue::Expression(value),
                });
            }
        }
    }
    Ok(Binding::Invocation {
        children: None,
        point,
        ty,
        inputs,
        condition,
        key,
    })
}

pub(super) fn void_element(name: &str) -> bool {
    matches!(
        name,
        "area"
            | "base"
            | "br"
            | "col"
            | "embed"
            | "hr"
            | "img"
            | "input"
            | "link"
            | "meta"
            | "param"
            | "source"
            | "track"
            | "wbr"
    )
}

pub(super) fn reserved_scope_name(name: &str) -> bool {
    matches!(name, "state" | "owner" | "event" | "ready") || name.starts_with("__rf")
}

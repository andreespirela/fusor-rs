//! The wrapper-free application boundary. Rust infers the constructor's result.
use super::{interpolation::interpolations, tokens::Rust};
use crate::{ExtractError, error};
use html5gum::StartTag;

pub(super) fn state(source: &str, tag: &StartTag<usize>) -> Result<Rust, ExtractError> {
    if tag.self_closing {
        return Err(error(
            source,
            tag.span.start,
            "App requires one native HTML root and an explicit closing tag",
        ));
    }
    if tag.attributes.len() != 1 || !tag.attributes.contains_key(b"state".as_slice()) {
        return Err(error(
            source,
            tag.span.start,
            "App requires only state=\"{{ Rust expression }}\"; put HTML attributes on its native root",
        ));
    }
    let value = &tag.attributes[b"state".as_slice()];
    let text = String::from_utf8_lossy(value);
    let parts = interpolations(source, &text, value.span.start, false)?;
    super::interpolation::exact_expression(
        source,
        &text,
        parts,
        value.span.start,
        "App state requires exactly one {{ Rust expression }}",
    )
}

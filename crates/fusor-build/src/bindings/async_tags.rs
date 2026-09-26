//! Structural async inputs; embedded expressions remain native Rust tokens.
use super::{ir::RegionKind, tag_input::TagInput, tokens::Rust};
use crate::ExtractError;

/// An `Async` tag's boundary, or an `Await` tag's read and the name of its value.
pub(super) fn inputs(
    input: &TagInput,
    await_value: bool,
) -> Result<(Rust, RegionKind), ExtractError> {
    input.closed()?;
    if await_value {
        input.accepts(
            &["value", "let"],
            "only value=\"{{ read }}\" and let=\"name\"; put HTML attributes on its native root",
        )?;
        let alias = input
            .binding("let")?
            .ok_or_else(|| input.error("Await requires let=\"name\" to name its resolved value"))?;
        let kind = RegionKind::Await { alias: Some(alias) };
        Ok((input.expression("value")?, kind))
    } else {
        input.accepts(
            &["boundary"],
            "only boundary=\"{{ boundary }}\"; put HTML attributes on its native root",
        )?;
        let value = match input.optional_expression("boundary")? {
            Some(value) => value,
            None => Rust::synthetic(
                quote::quote! { ::fusor::coherence::AsyncBoundary::coherent() },
                input.tag.span.start,
            ),
        };
        Ok((value, RegionKind::Boundary))
    }
}

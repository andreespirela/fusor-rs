//! Structural async inputs; embedded expressions remain native Rust tokens.
use super::{tag_input::TagInput, tokens::Rust};
use crate::ExtractError;

pub(super) enum Declaration {
    Async { value: Rust },
    Await { value: Rust, alias: Rust },
}

impl Declaration {
    pub fn alias(&self) -> Option<&Rust> {
        match self {
            Self::Async { .. } => None,
            Self::Await { alias, .. } => Some(alias),
        }
    }
}

pub(super) fn inputs(input: &TagInput, await_value: bool) -> Result<Declaration, ExtractError> {
    input.closed()?;
    if await_value {
        input.accepts(
            &["value", "let"],
            "only value=\"{{ read }}\" and let=\"name\"; put HTML attributes on its native root",
        )?;
        let alias = input
            .binding("let")?
            .ok_or_else(|| input.error("Await requires let=\"name\" to name its resolved value"))?;
        Ok(Declaration::Await {
            value: input.expression("value")?,
            alias,
        })
    } else {
        input.accepts(
            &["boundary"],
            "only boundary=\"{{ boundary }}\"; put HTML attributes on its native root",
        )?;
        let value = match input.optional_expression("boundary")? {
            Some(value) => value,
            None => Rust::synthetic(
                quote::quote! { ::fusor::coherence::AsyncBoundary::coherent() },
                input.offset(),
            ),
        };
        Ok(Declaration::Async { value })
    }
}

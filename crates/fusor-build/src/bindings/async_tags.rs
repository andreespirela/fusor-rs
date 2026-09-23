//! Structural async inputs; embedded expressions remain native Rust tokens.
use super::{interpolation::interpolations, tokens::Rust};
use crate::{ExtractError, error};
use html5gum::StartTag;

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

pub(super) fn inputs(
    source: &str,
    tag: &StartTag<usize>,
    await_value: bool,
) -> Result<Declaration, ExtractError> {
    if tag.self_closing
        || tag.attributes.keys().any(|key| {
            if await_value {
                !matches!(key.as_ref(), b"value" | b"let")
            } else {
                key.as_ref() != b"boundary"
            }
        })
    {
        return Err(error(
            source,
            tag.span.start,
            "Async accepts optional boundary; Await requires value and let. Use explicit closing tags and put DOM attributes on the native root",
        ));
    }
    let expression = |name: &[u8]| -> Result<Rust, ExtractError> {
        let value = tag.attributes.get(name).ok_or_else(|| {
            error(
                source,
                tag.span.start,
                "Await requires value=\"{{ read }}\" and let=\"name\"",
            )
        })?;
        let text = String::from_utf8_lossy(value);
        let parts = interpolations(source, &text, value.span.start, false)?;
        super::interpolation::exact_expression(
            source,
            &text,
            parts,
            value.span.start,
            "async inputs require exactly one {{ Rust expression }}",
        )
    };
    if await_value {
        let name = tag.attributes.get(b"let".as_slice()).ok_or_else(|| {
            error(
                source,
                tag.span.start,
                "Await requires let=\"name\" to name its resolved value",
            )
        })?;
        let text = String::from_utf8_lossy(name);
        if super::tags::reserved_scope_name(&text) {
            return Err(error(
                source,
                name.span.start,
                "Await names cannot shadow framework scope names",
            ));
        }
        Ok(Declaration::Await {
            value: expression(b"value")?,
            alias: super::tags::field(source, &text, name.span.start)?,
        })
    } else {
        Ok(Declaration::Async {
            value: if tag.attributes.is_empty() {
                Rust::parse(
                    source,
                    "::fusor::coherence::AsyncBoundary::coherent()",
                    tag.span.start,
                )?
            } else {
                expression(b"boundary")?
            },
        })
    }
}

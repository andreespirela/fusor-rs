//! Structural rules for application sources, checked before extraction.
use crate::{ExtractError, error, html};
use html5gum::Token;

/// A component file is a library of `<template rust:component>` declarations and
/// their Rust, never a document of its own.
pub(super) fn component_file(source: &str) -> Result<(), ExtractError> {
    let mut template_depth = 0;
    let mut in_script = false;
    for token in html::tokens(source) {
        match token {
            Token::StartTag(tag) => {
                if &*tag.name == b"template" {
                    if template_depth == 0
                        && !tag.attributes.contains_key(b"rust:component".as_slice())
                    {
                        return Err(error(
                            source,
                            tag.span.start,
                            "component files require named <template rust:component=\"Type\"> declarations",
                        ));
                    }
                    template_depth += 1;
                } else if &*tag.name == b"script" {
                    let module = template_depth > 0 && html::has_type(&tag, b"module");
                    if !html::is_rust_script(&tag) && !module {
                        return Err(error(
                            source,
                            tag.span.start,
                            "component files may contain Rust scripts and component-local module scripts inside templates",
                        ));
                    }
                    in_script = true;
                } else if template_depth == 0 {
                    return Err(error(
                        source,
                        tag.span.start,
                        "component files contain Rust scripts and component templates only; put page markup and styles in the entry page or assets",
                    ));
                }
            }
            Token::EndTag(tag) => {
                if &*tag.name == b"template" && template_depth > 0 {
                    template_depth -= 1;
                } else if &*tag.name == b"script" && in_script {
                    in_script = false;
                } else if template_depth == 0 {
                    return Err(error(
                        source,
                        tag.span.start,
                        "unexpected top-level closing tag in component file",
                    ));
                }
            }
            Token::String(text)
                if template_depth == 0
                    && !in_script
                    && !std::str::from_utf8(&text).is_ok_and(|text| text.trim().is_empty()) =>
            {
                return Err(error(
                    source,
                    text.span.start,
                    "put component text inside its template",
                ));
            }
            Token::Doctype(tag) => {
                return Err(error(
                    source,
                    tag.span.start,
                    "a component file is a template library, not a document",
                ));
            }
            _ => {}
        }
    }
    Ok(())
}

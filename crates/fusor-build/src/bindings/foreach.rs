//! Structural list syntax and item-only forwarding-row analysis.
use super::{ir::*, tokens::Rust};
use crate::{ExtractError, error};
use html5gum::{DefaultEmitter, StartTag, Token, Tokenizer};
use std::collections::BTreeSet;

/// A ForEach owns its native parent's children, without adding a wrapper node.
/// Validate this before lowering so static siblings cannot silently disappear.
pub(super) fn hosts(source: &str) -> Result<BTreeSet<usize>, ExtractError> {
    struct Frame {
        name: String,
        start: usize,
        children: usize,
        list: bool,
        text: bool,
    }
    let mut emitter = DefaultEmitter::<usize>::new_with_span();
    emitter.naively_switch_states(true);
    let mut stack: Vec<Frame> = Vec::new();
    let mut hosts = BTreeSet::new();
    for token in Tokenizer::new_with_emitter(source, emitter) {
        match token.expect("in-memory HTML") {
            Token::StartTag(tag) => {
                let name = String::from_utf8_lossy(&tag.name).into_owned();
                let list = super::tags::name(source, tag.span.start) == "ForEach";
                if name == "foreach" && !list {
                    return Err(error(
                        source,
                        tag.span.start,
                        "the built-in component is spelled ForEach",
                    ));
                }
                if list
                    && stack.iter().any(|frame| {
                        matches!(frame.name.as_str(), "svg" | "math" | "select" | "option")
                    })
                {
                    return Err(error(
                        source,
                        tag.span.start,
                        "ForEach requires ordinary HTML outside SVG, MathML and select controls",
                    ));
                }
                if let Some(parent) = stack.last_mut() {
                    parent.children += 1;
                    parent.list |= list;
                }
                if !tag.self_closing && !super::tags::void_element(&name) {
                    stack.push(Frame {
                        name,
                        start: tag.span.start,
                        children: 0,
                        list: false,
                        text: false,
                    });
                }
            }
            Token::EndTag(tag) => {
                let name = String::from_utf8_lossy(&tag.name);
                if let Some(index) = stack.iter().rposition(|frame| frame.name == name) {
                    let frame = &stack[index];
                    if frame.list {
                        if frame.children != 1
                            || frame.text
                            || super::tags::is_component(super::tags::name(source, frame.start))
                            || frame.name == "template"
                        {
                            return Err(error(
                                source,
                                frame.start,
                                "ForEach requires its own native HTML container; put other content outside that container",
                            ));
                        }
                        hosts.insert(frame.start);
                    }
                    stack.truncate(index);
                }
            }
            Token::String(text) => {
                if let Some(frame) = stack.last_mut() {
                    frame.text |= !String::from_utf8_lossy(&text).trim().is_empty();
                }
            }
            _ => {}
        }
    }
    Ok(hosts)
}

pub(super) fn inputs(
    source: &str,
    tag: &StartTag<usize>,
) -> Result<(Rust, Rust, Rust, Rust), ExtractError> {
    if tag.self_closing {
        return Err(error(
            source,
            tag.span.start,
            "ForEach requires inline HTML and an explicit closing tag",
        ));
    }
    for name in tag.attributes.keys() {
        if !matches!(name.as_ref(), b"items" | b"key" | b"item" | b"index") {
            return Err(error(
                source,
                tag.span.start,
                "ForEach accepts items, key, and optional item/index binding names",
            ));
        }
    }
    let expression = |name: &[u8]| {
        let value = tag.attributes.get(name).ok_or_else(|| {
            error(
                source,
                tag.span.start,
                "ForEach requires items and key expressions",
            )
        })?;
        let value_text = String::from_utf8_lossy(value);
        let parts =
            super::interpolation::interpolations(source, &value_text, value.span.start, false)?;
        super::interpolation::exact_expression(
            source,
            &value_text,
            parts,
            value.span.start,
            "ForEach inputs require exactly one {{ Rust expression }}",
        )
    };
    let name = |attr: &[u8], default: &str| {
        let text = tag
            .attributes
            .get(attr)
            .map(|v| String::from_utf8_lossy(v).into_owned())
            .unwrap_or_else(|| default.into());
        if super::tags::reserved_scope_name(&text) {
            return Err(error(
                source,
                tag.span.start,
                "ForEach bindings cannot shadow framework scope names",
            ));
        }
        super::tags::field(source, &text, tag.span.start)
    };
    let item = name(b"item", "item")?;
    let index = name(b"index", "index")?;
    if item.tokens.to_string() == index.tokens.to_string() {
        return Err(error(
            source,
            tag.span.start,
            "ForEach item and index names must differ",
        ));
    }
    Ok((expression(b"items")?, expression(b"key")?, item, index))
}

// This proof intentionally covers only the existing direct component-forwarding
// row lowering. Descendant content and nested lexical rows retain the full Row.
pub(super) fn mark_item_only_rows(components: &mut [Component]) {
    fn bodies(bindings: &[Binding], rows: &mut Vec<usize>) {
        for binding in bindings {
            match binding {
                Binding::ForEach { body, .. } => rows.push(*body),
                Binding::Region { bindings, .. } => bodies(bindings, rows),
                _ => {}
            }
        }
    }
    let mut rows = Vec::new();
    for component in components.iter() {
        bodies(&component.bindings, &mut rows);
    }
    for row in rows {
        let item_only = forwards_item_only(&components[row], components);
        components[row].item_only_row = item_only;
    }
}

fn forwards_item_only(component: &Component, components: &[Component]) -> bool {
    if !component.inline
        || component.capture.is_some()
        || component.locals.len() != 1
        || !component.async_locals.is_empty()
        || !component.route_locals.is_empty()
        || !component.elements.is_empty()
        || !component.texts.is_empty()
        || !component.text_elements.is_empty()
    {
        return false;
    }
    let [
        binding @ Binding::Invocation {
            inputs,
            children,
            condition: None,
            key: None,
            ..
        },
    ] = component.bindings.as_slice()
    else {
        return false;
    };
    // The parser retains an empty Children fragment for explicit closing tags.
    // children_factory discards it. No descendant code is evaluated or captured.
    if children.is_some_and(|index| {
        let child = &components[index];
        !child.empty || !child.bindings.is_empty()
    }) {
        return false;
    }
    if inputs
        .iter()
        .any(|input| !matches!(input.value, InputValue::Expression(_)))
    {
        return false;
    }
    let index = component.locals[0].1.tokens.to_string();
    let index = index.strip_prefix("r#").unwrap_or(&index);
    fn independent(tokens: proc_macro2::TokenStream, index: &str) -> bool {
        tokens.into_iter().all(|token| match token {
            proc_macro2::TokenTree::Group(group) => independent(group.stream(), index),
            proc_macro2::TokenTree::Ident(ident) => {
                let name = ident.to_string();
                let name = name.strip_prefix("r#").unwrap_or(&name);
                // Raw names compare like ordinary identifiers. Conservatively
                // avoid Unicode normalization and compiler-context escapes.
                name.is_ascii() && name != index && !name.starts_with("__fusor")
            }
            // Opaque macros/attributes may introduce a use absent from tokens.
            // Rejecting unary ! and != too is an intentional false positive.
            proc_macro2::TokenTree::Punct(punct) => !matches!(punct.as_char(), '!' | '#'),
            proc_macro2::TokenTree::Literal(_) => true,
        })
    }
    index.is_ascii()
        && binding
            .fragments()
            .iter()
            .all(|fragment| independent(fragment.tokens.clone(), index))
}

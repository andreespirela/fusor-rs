use crate::bindings::interpolation::{Interpolation, exact_expression, interpolations};
use crate::bindings::ir::{
    Binding, Component, Element, InputKind, InterpolatedString, RenderTarget, StringPart,
};
use crate::bindings::tags::void_element;
use crate::bindings::tokens::Rust;
use crate::{ExtractError, error};
use fusor::template::{self, ChildPolicy, ComponentId, ElementId};
use html5gum::StartTag;
use std::collections::BTreeMap;

fn escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('"', "&quot;")
        .replace('<', "&lt;")
}

fn boolean_attribute(name: &str) -> bool {
    matches!(
        name,
        "allowfullscreen"
            | "async"
            | "autofocus"
            | "autoplay"
            | "controls"
            | "default"
            | "defer"
            | "disabled"
            | "formnovalidate"
            | "hidden"
            | "inert"
            | "ismap"
            | "itemscope"
            | "loop"
            | "multiple"
            | "muted"
            | "nomodule"
            | "novalidate"
            | "open"
            | "playsinline"
            | "readonly"
            | "required"
            | "reversed"
            | "selected"
    )
}

pub(super) fn is_directive(name: &str) -> bool {
    name == "hydrate"
        || name.starts_with("hydrate:")
        || name.starts_with("rust:")
        || name.starts_with("on:")
        || name.starts_with("bind:")
        || name.starts_with("class:")
        || name.starts_with("prop:")
}

fn string(value: &str, parts: Vec<Interpolation>, offset: usize) -> InterpolatedString {
    let mut result = Vec::new();
    let mut cursor = 0;
    for part in parts {
        result.push(StringPart::Literal(
            value[cursor..part.range.start].to_owned(),
        ));
        result.push(StringPart::Expression(Rust {
            tokens: part.tokens,
            offset,
        }));
        cursor = part.range.end;
    }
    result.push(StringPart::Literal(value[cursor..].to_owned()));
    InterpolatedString(result)
}

// Appends at most one element descriptor. Traversal records the returned edit
// before selecting a TextHost, whose opening_edit indexes that edit.
pub(super) fn lower(
    source: &str,
    tag: &StartTag<usize>,
    component: &mut Component,
    node: ElementId,
    component_id: Option<ComponentId>,
    foreach_host: bool,
    async_root: bool,
) -> Result<Option<String>, ExtractError> {
    let name = String::from_utf8_lossy(&tag.name);
    let attrs: BTreeMap<String, (String, usize)> = tag
        .attributes
        .iter()
        .map(|(key, value)| {
            (
                if key.starts_with(b"prop:") || key.starts_with(b"on:") {
                    let authored = source[value.span.start..]
                        .split(|ch: char| ch.is_ascii_whitespace() || matches!(ch, '=' | '/' | '>'))
                        .next()
                        .unwrap_or("");
                    let prefix = if key.starts_with(b"prop:") {
                        "prop"
                    } else {
                        "on"
                    };
                    format!(
                        "{prefix}:{}",
                        authored.split_once(':').map_or("", |(_, suffix)| suffix)
                    )
                } else {
                    String::from_utf8_lossy(key).into_owned()
                },
                (
                    String::from_utf8_lossy(value).into_owned(),
                    value.span.start,
                ),
            )
        })
        .collect();
    let mut rendered = String::new();
    let mut changed = component_id.is_some() || foreach_host || async_root;
    let mut bound = foreach_host || async_root;
    for (attr, (value, offset)) in &attrs {
        let offset = *offset;
        if matches!(attr.as_str(), "rust:each" | "rust:row") {
            return Err(error(
                source,
                offset,
                "rust:each and rust:row were removed; use <ForEach items=\"{{ values }}\" key=\"{{ |item| item.id }}\"> with inline HTML",
            ));
        }
        if attr == "rust:attach" {
            return Err(error(
                source,
                offset,
                "rust:attach was removed; use a component JavaScript module with onMount and onCleanup (see /docs/npm)",
            ));
        }
        if attr == "rust:outlet" {
            return Err(error(
                source,
                offset,
                "rust:outlet was removed; use <Router> with <Route> children",
            ));
        }
        if attr == "rust:mount" {
            return Err(error(
                source,
                offset,
                "rust:mount was removed; use a component tag with named inputs, such as <Details id=\"{{ state.id.get() }}\"></Details>; implement FromInputs for custom construction",
            ));
        }
        let expression = || Rust::parse(source, value, offset);
        if attr == "rust:component" {
            continue;
        }
        if attr == "rust:render" {
            if component.app.is_some() {
                return Err(error(
                    source,
                    offset,
                    "App is a browser startup boundary; use rust:component templates for native or shared rendering",
                ));
            }
            if component_id.is_none() {
                return Err(error(
                    source,
                    offset,
                    "rust:render belongs on a component declaration",
                ));
            }
            component.render = match value.as_str() {
                "server" => RenderTarget::Server,
                "shared" => RenderTarget::Shared,
                _ => {
                    return Err(error(
                        source,
                        offset,
                        "rust:render must be server or shared",
                    ));
                }
            };
            changed = true;
            continue;
        }
        if attr == "hydrate:target" {
            if name != "button"
                || value.trim().is_empty()
                || value.contains("{{")
                || attrs
                    .get("type")
                    .is_none_or(|(value, _)| !value.eq_ignore_ascii_case("button"))
            {
                return Err(error(
                    source,
                    offset,
                    "hydrate:target requires an explicit instance ID on a native type=button button",
                ));
            }
            rendered.push_str(&format!(
                " data-fusor-activate-target=\"{}\"",
                escape(value)
            ));
            changed = true;
            continue;
        }
        if matches!(
            attr.as_str(),
            "rust:island"
                | "rust:props"
                | "rust:activate"
                | "rust:prefetch"
                | "rust:activate-target"
        ) {
            return Err(error(
                source,
                offset,
                "island directives were removed; use <Component hydrate=\"visible\"> with named inputs, and hydrate:target on an activation button",
            ));
        }
        if attr == "hydrate" || attr.starts_with("hydrate:") {
            return Err(error(
                source,
                offset,
                "hydrate belongs on a component tag; only hydrate:target belongs on a native button",
            ));
        }
        if attr == "rust:async" || attr == "rust:await" {
            if value.trim().is_empty() || void_element(&name) || name == "template" {
                return Err(error(
                    source,
                    offset,
                    "rust:async and rust:await require an expression and an ordinary HTML region",
                ));
            }
            changed = true;
            bound = true;
            continue;
        }
        if attr == "rust:app" {
            return Err(error(
                source,
                offset,
                "rust:app was removed; wrap the native root in <App state=\"{{ constructor }}\"> and remove its rust:component declaration",
            ));
        }
        if matches!(attr.as_str(), "rust:key" | "rust:slot" | "rust:if") {
            changed = true;
            continue;
        }
        let binding = if let Some(property) = attr.strip_prefix("prop:") {
            if !name.contains('-') || property.is_empty() || value.trim().is_empty() {
                return Err(error(
                    source,
                    offset,
                    "prop:name requires a custom HTML element and a Rust value",
                ));
            }
            if matches!(
                property,
                "innerHTML" | "outerHTML" | "textContent" | "innerText" | "outerText"
            ) || property.starts_with("on")
            {
                return Err(error(
                    source,
                    offset,
                    "prop:name cannot replace owned HTML or event handlers; use HTML children and on:event",
                ));
            }
            Some(Binding::Property {
                node,
                name: property.to_owned(),
                value: expression()?,
            })
        } else if let Some(event) = attr.strip_prefix("on:") {
            if event.is_empty() || value.trim().is_empty() {
                return Err(error(source, offset, "on:event requires a Rust handler"));
            }
            Some(Binding::Event {
                node,
                name: event.to_owned(),
                handler: expression()?,
            })
        } else if attr == "bind:field" {
            let kind = attrs
                .get("type")
                .map_or("text", |(kind, _)| kind)
                .to_ascii_lowercase();
            if name != "textarea"
                && (name != "input"
                    || !matches!(
                        kind.as_str(),
                        "text" | "search" | "email" | "url" | "tel" | "password"
                    ))
            {
                return Err(error(
                    source,
                    offset,
                    "bind:field requires a text input (text/search/email/url/tel/password) or textarea with a static type",
                ));
            }
            if ["value", "bind:value", "bind:checked", "rust:slot"]
                .iter()
                .any(|name| attrs.contains_key(*name))
            {
                return Err(error(
                    source,
                    offset,
                    "bind:field owns the control value; remove value attributes, other value bindings and child ownership directives",
                ));
            }
            Some(Binding::Field {
                node,
                value: expression()?,
            })
        } else if let Some(property) = attr.strip_prefix("bind:") {
            if name != "input" || !matches!(property, "value" | "checked") {
                return Err(error(
                    source,
                    offset,
                    "supported two-way bindings are bind:value/bind:checked on <input>, and bind:field on text inputs/textarea",
                ));
            }
            if attrs
                .get(property)
                .is_some_and(|(value, _)| value.contains("{{"))
            {
                return Err(error(
                    source,
                    offset,
                    "a property cannot have both an interpolation and a two-way binding",
                ));
            }
            let kind = if property == "value" {
                InputKind::Value
            } else {
                InputKind::Checked
            };
            Some(Binding::Input {
                node,
                kind,
                value: expression()?,
            })
        } else if let Some(class) = attr.strip_prefix("class:") {
            if class.is_empty() || class.chars().any(char::is_whitespace) {
                return Err(error(
                    source,
                    offset,
                    "class:name requires a single CSS class name",
                ));
            }
            if attrs
                .get("class")
                .is_some_and(|(value, _)| value.contains("{{"))
            {
                return Err(error(
                    source,
                    offset,
                    "use a static class attribute with class:name bindings",
                ));
            }
            Some(Binding::Class {
                node,
                name: class.to_owned(),
                value: expression()?,
            })
        } else if attr.starts_with("rust:") {
            return Err(error(
                source,
                offset,
                format!("unknown Rust directive {attr:?}"),
            ));
        } else {
            let parts = interpolations(source, value, offset, false)?;
            if parts.is_empty() {
                rendered.push_str(&format!(" {attr}=\"{}\"", escape(value)));
                None
            } else {
                if attr.starts_with("on") || attr == "srcdoc" {
                    return Err(error(
                        source,
                        offset,
                        "interpolation cannot create executable HTML; use on:event with a Rust handler",
                    ));
                }
                if boolean_attribute(attr) || attr == "checked" {
                    let value = exact_expression(
                        source,
                        value,
                        parts,
                        offset,
                        "this property requires exactly one {{ Rust boolean expression }}",
                    )?;
                    if attr == "checked" {
                        if name != "input" {
                            return Err(error(source, offset, "checked requires an <input>"));
                        }
                        Some(Binding::Checked { node, value })
                    } else {
                        Some(Binding::Boolean {
                            node,
                            name: attr.clone(),
                            value,
                        })
                    }
                } else if attr == "value" && name == "input" {
                    Some(Binding::Value {
                        node,
                        value: string(value, parts, offset),
                    })
                } else {
                    Some(Binding::Attribute {
                        node,
                        name: attr.clone(),
                        value: string(value, parts, offset),
                    })
                }
            }
        };
        if let Some(binding) = binding {
            component.bindings.push(binding);
            changed = true;
            bound = true;
        }
    }
    if let Some(constructor) = attrs.get("rust:slot") {
        if void_element(&name)
            || matches!(
                name.as_ref(),
                "script"
                    | "style"
                    | "textarea"
                    | "title"
                    | "select"
                    | "option"
                    | "xmp"
                    | "iframe"
                    | "noembed"
                    | "noframes"
                    | "plaintext"
            )
        {
            return Err(error(
                source,
                tag.span.start,
                "rust:slot requires an ordinary HTML container",
            ));
        }
        let expression = |(code, offset): &(String, usize)| Rust::parse(source, code, *offset);
        let value = expression(constructor)?;
        let condition = attrs.get("rust:if").map(expression).transpose()?;
        let key = attrs.get("rust:key").map(expression).transpose()?;
        component.bindings.push(Binding::Slot {
            node,
            content: value,
            condition,
            key,
        });
        bound = true;
    } else {
        if attrs.contains_key("rust:if") {
            return Err(error(
                source,
                tag.span.start,
                "rust:if requires rust:slot on the same element",
            ));
        }
        if attrs.contains_key("rust:key") {
            return Err(error(
                source,
                tag.span.start,
                "rust:key requires a component mount or slot; use the key input on ForEach for lists",
            ));
        }
    }
    if !changed {
        return Ok(None);
    }
    if bound {
        rendered.push_str(&format!(" {}=\"{node}\"", template::ELEMENT_ATTRIBUTE));
        if foreach_host || attrs.contains_key("rust:slot") {
            rendered.push_str(&format!(" {}=\"\"", template::MANAGED_ATTRIBUTE));
        }
        component.elements.push(Element {
            id: node,
            tag: name.to_string(),
            children: if foreach_host || attrs.contains_key("rust:slot") {
                ChildPolicy::Managed
            } else {
                ChildPolicy::Static
            },
        });
    }
    if let Some(id) = component_id {
        rendered.push_str(&format!(
            " {}=\"{id}\" {}=\"{}\"",
            template::COMPONENT_ATTRIBUTE,
            template::VERSION_ATTRIBUTE,
            template::VERSION
        ));
    }
    Ok(Some(format!(
        "<{name}{rendered}{}>",
        if tag.self_closing { " /" } else { "" }
    )))
}

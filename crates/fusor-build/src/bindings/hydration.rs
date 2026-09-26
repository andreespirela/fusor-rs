//! Component-tag authoring for the existing typed delivery protocol.
use super::{ir::*, tags};
use crate::{ExtractError, error};
use fusor::template::{self, ChildPolicy, ElementId, MountId};
use html5gum::StartTag;

pub(super) fn lower(
    source: &str,
    tag: &StartTag<usize>,
    node: ElementId,
    component: &mut Component,
) -> Result<String, ExtractError> {
    let offset = tag.span.start;
    if !tags::is_component(tags::name(source, offset)) || tag.self_closing {
        return Err(error(
            source,
            offset,
            "hydrate requires a Rust component tag with an explicit closing tag",
        ));
    }
    let value = |name: &[u8]| {
        tag.attributes
            .get(name)
            .map(|v| String::from_utf8_lossy(v).into_owned())
    };
    let activation = value(b"hydrate").expect("hydrate present");
    let prefetch = value(b"hydrate:prefetch").unwrap_or_else(|| "none".into());
    let Some(activation) = Activation::parse(&activation) else {
        return Err(error(
            source,
            offset,
            "hydrate must be load, visible, idle, interaction, or manual",
        ));
    };
    let Some(prefetch) = Prefetch::parse(&prefetch) else {
        return Err(error(
            source,
            offset,
            "hydrate:prefetch must be none, load, visible, or idle",
        ));
    };
    let id = value(b"hydrate:id");
    if id
        .as_ref()
        .is_some_and(|id| id.trim().is_empty() || id.contains("{{"))
    {
        return Err(error(
            source,
            offset,
            "hydrate:id requires a nonempty static instance ID",
        ));
    }
    if activation == Activation::Interaction && id.is_none() {
        return Err(error(
            source,
            offset,
            "hydrate=\"interaction\" requires hydrate:id and a native button with the matching hydrate:target",
        ));
    }
    let mut inputs_tag = tag.clone();
    for name in [b"hydrate".as_slice(), b"hydrate:id", b"hydrate:prefetch"] {
        inputs_tag.attributes.remove(name);
    }
    let Binding::Invocation {
        ty,
        inputs,
        condition,
        key,
        ..
    } = tags::invocation(source, &inputs_tag, MountId::new(0))?
    else {
        unreachable!()
    };
    if condition.is_some() || key.is_some() {
        return Err(error(
            source,
            offset,
            "hydrated components cannot use rust:if or rust:key; their lifetime belongs to the server-rendered page",
        ));
    }
    component.bindings.push(Binding::Island {
        node,
        descriptor: ty,
        inputs,
        activation,
        prefetch,
    });
    component.elements.push(Element {
        id: node,
        tag: "div".into(),
        children: ChildPolicy::Managed,
    });
    let id = id
        .map(|id| format!(" id=\"{}\"", super::markup::escape_attribute(&id)))
        .unwrap_or_default();
    Ok(format!(
        "<div{id} {}=\"{node}\" {}=\"\">",
        template::ELEMENT_ATTRIBUTE,
        template::MANAGED_ATTRIBUTE
    ))
}

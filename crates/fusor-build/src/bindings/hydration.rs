//! Component-tag authoring for the existing typed delivery protocol.
use super::{ir::*, tags, tokens::Rust};
use crate::{ExtractError, error};
use fusor::template::{self, ChildPolicy, ElementId, MountId};
use html5gum::StartTag;
use quote::quote;

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
    if !matches!(
        activation.as_str(),
        "load" | "visible" | "idle" | "interaction" | "manual"
    ) {
        return Err(error(
            source,
            offset,
            "hydrate must be load, visible, idle, interaction, or manual",
        ));
    }
    if !matches!(prefetch.as_str(), "none" | "load" | "visible" | "idle") {
        return Err(error(
            source,
            offset,
            "hydrate:prefetch must be none, load, visible, or idle",
        ));
    }
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
    if activation == "interaction" && id.is_none() {
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
    let fields = inputs.iter().map(|input| {
        let name = &input.name;
        let InputValue::Expression(value) = &input.value else {
            unreachable!()
        };
        // String literal inputs own their value across the serialized boundary.
        if syn::parse2::<syn::LitStr>(quote! { #value }).is_ok() {
            quote! { #name: ::core::convert::Into::into(#value) }
        } else {
            quote! { #name: { #value } }
        }
    });
    let props = Rust::parse(
        source,
        &quote! {{
            type __FusorIslandProps = <#ty as ::fusor_islands::Island>::Props;
            __FusorIslandProps { #(#fields),* }
        }}
        .to_string(),
        offset,
    )?;
    component.bindings.push(Binding::Island {
        node,
        descriptor: ty,
        props,
        activation,
        prefetch,
    });
    component.elements.push(Element {
        id: node,
        tag: "div".into(),
        children: ChildPolicy::Managed,
    });
    let id = id
        .map(|id| {
            format!(
                " id=\"{}\"",
                id.replace('&', "&amp;")
                    .replace('"', "&quot;")
                    .replace('<', "&lt;")
            )
        })
        .unwrap_or_default();
    Ok(format!(
        "<div{id} {}=\"{node}\" {}=\"\">",
        template::ELEMENT_ATTRIBUTE,
        template::MANAGED_ATTRIBUTE
    ))
}

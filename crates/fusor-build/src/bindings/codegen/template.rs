use super::values::{clone_locals, element, point, string, text, typed_text_eligible};
use crate::bindings::{
    ir::{Binding, Component},
    tokens::Rust,
};
use fusor::template::{self, ChildPolicy, ElementId, MountId, RootKind, TextId};
use proc_macro2::TokenStream;
use quote::{quote, quote_spanned};

pub(super) struct TemplateCode {
    pub(super) declarations: TokenStream,
    pub(super) typed_handles: TokenStream,
    pub(super) bundle: Option<Vec<TokenStream>>,
}

pub(super) fn lower(component: &Component, has_ready: bool) -> TemplateCode {
    let id = component.id.index();
    let version = template::VERSION;
    let kind = match component.kind {
        RootKind::Existing => quote! { ::fusor::template::RootKind::Existing },
        RootKind::Template => quote! { ::fusor::template::RootKind::Template },
    };
    let elements = component.elements.iter().map(|node| {
        let id = node.id.index();
        let tag = &node.tag;
        let children = match node.children {
            ChildPolicy::Static => quote! { ::fusor::template::ChildPolicy::Static },
            ChildPolicy::Managed => quote! { ::fusor::template::ChildPolicy::Managed },
        };
        quote! { ::fusor::template::ElementDescriptor {
            id: ::fusor::template::ElementId::new(#id), tag: #tag, children: #children,
        }}
    });
    let texts = component.texts.iter().map(|id| {
        let id = id.index();
        quote! { ::fusor::template::TextId::new(#id) }
    });
    let text_elements = component.text_elements.iter().map(|element| {
        let id = element.id.index();
        let tag = &element.tag;
        let host = element.host.map_or_else(
            || quote! { ::std::option::Option::None },
            |id| {
                let id = id.index();
                quote! { ::std::option::Option::Some(::fusor::template::ElementId::new(#id)) }
            },
        );
        quote! { ::fusor::template::TextElementDescriptor {
            id: ::fusor::template::TextId::new(#id), host: #host, tag: #tag,
        } }
    });
    let handles = component.elements.iter().map(|node| {
        let name = element(node.id);
        let id = node.id.index();
        if node.tag == "input" {
            quote! { let #name = __fusor_nodes.take_input(::fusor::template::ElementId::new(#id))?; }
        } else {
            quote! { let #name = __fusor_nodes.take_element(::fusor::template::ElementId::new(#id))?; }
        }
    });
    let text_handles = component
        .texts
        .iter()
        .chain(component.text_elements.iter().map(|element| &element.id))
        .map(|id| {
            let name = text(*id);
            let id = id.index();
            quote! { let #name = __fusor_nodes.take_text(::fusor::template::TextId::new(#id))?; }
        });
    fn mount_points(bindings: &[Binding], mounts: &mut Vec<MountId>) {
        for binding in bindings {
            match binding {
                Binding::Invocation { point, .. }
                | Binding::Children { point, .. }
                | Binding::Router { point, .. }
                | Binding::Branch { point, .. } => mounts.push(*point),
                Binding::Region { bindings, .. } => mount_points(bindings, mounts),
                _ => {}
            }
        }
    }
    let mut mounts = Vec::new();
    mount_points(&component.bindings, &mut mounts);
    let mount_ids = mounts.iter().map(|id| {
        let id = id.index();
        quote! { ::fusor::template::MountId::new(#id) }
    });
    let mount_handles = mounts.iter().map(|id| {
        let name = point(*id);
        let id = id.index();
        quote! { let #name = __fusor_nodes.take_mount_point(::fusor::template::MountId::new(#id))?; }
    });
    TemplateCode {
        declarations: quote! {
            const __FUSOR_TEMPLATE: ::fusor::template::TemplateDescriptor = ::fusor::template::TemplateDescriptor {
                version: #version,
                component: ::fusor::template::ComponentId::new(#id),
                kind: #kind,
                elements: &[#(#elements),*],
                texts: &[#(#texts),*],
                text_elements: &[#(#text_elements),*],
            };
            const __FUSOR_MOUNTS: &[::fusor::template::MountId] = &[#(#mount_ids),*];
        },
        typed_handles: quote! {
            #(#handles)*
            #(#text_handles)*
            #(#mount_handles)*
        },
        bundle: binding_bundle(component, has_ready, &component.async_locals),
    }
}

// The bundle path preserves ordinary binding effects. Inputs, managed regions,
// fragments and all other binding kinds keep the existing typed interface.
fn binding_bundle(
    component: &Component,
    has_ready: bool,
    locals: &[Rust],
) -> Option<Vec<TokenStream>> {
    if component.fragment
        || component.bindings.is_empty()
        || component.elements.iter().any(|element| {
            element.children != ChildPolicy::Static
                || matches!(element.tag.as_str(), "input" | "textarea" | "select")
        })
    {
        return None;
    }
    // IDs are global and sparse; the bundle uses descriptor ordinals, not
    // raw IDs. Its Text section follows elements, then anchored/direct order.
    let elements = component
        .elements
        .iter()
        .enumerate()
        .map(|(index, element)| (element.id, u32::try_from(index).expect("bundle too large")))
        .collect();
    let texts = component
        .texts
        .iter()
        .chain(component.text_elements.iter().map(|element| &element.id))
        .enumerate()
        .map(|(index, id)| {
            (
                *id,
                u32::try_from(component.elements.len() + index).expect("bundle too large"),
            )
        })
        .collect();
    component
        .bindings
        .iter()
        .map(|binding| bundle_binding(binding, &elements, &texts, has_ready, locals))
        .collect()
}

fn bundle_binding(
    binding: &Binding,
    elements: &::std::collections::BTreeMap<ElementId, u32>,
    texts: &::std::collections::BTreeMap<TextId, u32>,
    has_ready: bool,
    locals: &[Rust],
) -> Option<TokenStream> {
    let span = binding.fragments()[0].span();
    let operation = match binding {
        Binding::Text { slot, value } => {
            let slot = texts[slot];
            if typed_text_eligible(value) {
                quote_spanned! {span=> __fusor_scope.bundle_text_value(&__fusor_bundle, #slot, move || {
                    use ::fusor::dom::text_value::Convert as _;
                    (&::fusor::dom::text_value::Value(&(#value))).__fusor_into_text()
                })?; }
            } else {
                quote_spanned! {span=> __fusor_scope.bundle_text_string(&__fusor_bundle, #slot, move || ::std::string::ToString::to_string(&(#value)))?; }
            }
        }
        Binding::Attribute { node, name, value } => {
            let slot = elements[node];
            let value = string(value);
            quote_spanned! {span=> __fusor_scope.bundle_attr(&__fusor_bundle, #slot, #name, move || ::std::option::Option::Some(#value))?; }
        }
        Binding::Event {
            node,
            name,
            handler,
        } => {
            let slot = elements[node];
            quote_spanned! {span=> __fusor_scope.bundle_on(&__fusor_bundle, #slot, #name, move |event| { #handler })?; }
        }
        // These operations require the ordinary typed handles or managed lifetimes.
        Binding::Branch { .. }
        | Binding::Router { .. }
        | Binding::ForEach { .. }
        | Binding::Children { .. }
        | Binding::Invocation { .. }
        | Binding::Island { .. }
        | Binding::Region { .. }
        | Binding::Property { .. }
        | Binding::Boolean { .. }
        | Binding::Value { .. }
        | Binding::Checked { .. }
        | Binding::Class { .. }
        | Binding::Input { .. }
        | Binding::Field { .. }
        | Binding::Slot { .. } => return None,
    };
    let ready = has_ready.then(|| quote! { let ready = ::std::rc::Rc::clone(&ready); });
    let locals = clone_locals(locals);
    Some(quote_spanned! {span=> {
        #locals
        #ready
        let state = ::std::rc::Rc::clone(&state);
        let __fusor_children = __fusor_children.clone();
        #operation
    }})
}

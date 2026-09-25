//! Lower the binding plan into native Rust token trees.

mod template;
mod values;

pub(super) use values::string;
use values::{clone_locals, element, point, text, typed_text_eligible};

use super::{
    ir::*,
    tokens::{self, Origins, Rust},
};
use crate::BindingLocation;
use fusor::template::RootKind;
use proc_macro2::TokenStream;
use quote::{format_ident, quote, quote_spanned};

fn children_factory(
    index: Option<usize>,
    components: &[Component],
    delivery: bool,
    ready: bool,
) -> TokenStream {
    let Some(index) = index.filter(|index| !components[*index].empty) else {
        return quote! { ::fusor::dom::Children::default() };
    };
    let prepare = component(&components[index], delivery, components, ready);
    let locals = clone_locals(&components[index].async_locals);
    let capture_ready = ready.then(|| quote! { let __fusor_ready = ::std::rc::Rc::clone(&ready); });
    let expose_ready = ready.then(|| quote! { let ready = ::std::rc::Rc::clone(&__fusor_ready); });
    quote! {{
        let __fusor_capture = ::std::rc::Rc::clone(&state);
        let __fusor_forward = __fusor_children.clone();
        #capture_ready
        #locals
        ::fusor::dom::Children::new(move |__fusor_parent| {
            let state = ::std::rc::Rc::clone(&__fusor_capture);
            let __fusor_children = __fusor_forward.clone();
            #expose_ready
            #locals
            #prepare
        })
    }}
}

// Recursive constructors must occur once in the emitted Rust, before runtime
// mode selection. Each mode keeps its own scheduling and ownership adapter.
struct BrowserBinding {
    shared: TokenStream,
    ordinary: TokenStream,
    coherent: TokenStream,
}

fn browser_binding(
    item: &Binding,
    shared: bool,
    components: &[Component],
    delivery_metadata: bool,
    has_ready: bool,
    locals: &[Rust],
) -> BrowserBinding {
    let span = item.fragments()[0].span();
    if let Binding::Branch {
        point: id,
        value,
        cases,
        snapshots,
    } = item
    {
        let slot = id.index();
        let point = point(*id);
        let read = format_ident!("__fusor_branch_read_{}", slot);
        let prepare = format_ident!("__fusor_branch_prepare_{}", slot);
        let selection = super::control::selection(value, cases, snapshots);
        let factories =
            branch_factories(cases, snapshots, components, delivery_metadata, has_ready);
        let clones = clone_locals(locals);
        let capture_ready = has_ready.then(|| quote! { let ready = ::std::rc::Rc::clone(&ready); });
        return BrowserBinding {
            shared: quote_spanned! {span=>
                let (#read, #prepare) = {
                    // Give rustc the selection payload before checking projections
                    // in the hoisted constructor; application types stay inferred.
                    fn __fusor_branch_parts<T, R, F>(read: R, prepare: F) -> (R, F)
                    where
                        T: ::std::clone::Clone + ::std::cmp::PartialEq + 'static,
                        R: ::std::ops::Fn() -> (usize, T),
                        F: ::std::ops::Fn(usize, ::fusor::Signal<T>, &::fusor::OwnerHandle)
                            -> ::std::result::Result<::fusor::dom::Scope, ::fusor::dom::JsValue>,
                    {
                        (read, prepare)
                    }
                    let __fusor_read_state = ::std::rc::Rc::clone(&state);
                    let __fusor_capture = ::std::rc::Rc::clone(&state);
                    let __fusor_children = __fusor_children.clone();
                    __fusor_branch_parts(
                        { #clones #capture_ready move || { let state = &__fusor_read_state; #selection } },
                        { #clones #capture_ready move |__fusor_case, __fusor_data, __fusor_parent| {
                            let state = ::std::rc::Rc::clone(&__fusor_capture);
                            match __fusor_case { #factories _ => unreachable!("generated branch index") }
                        } }
                    )
                };
            },
            ordinary: quote_spanned! {span=> __fusor_scope.branch_at(&#point, #read, #prepare)?; },
            coherent: quote_spanned! {span=> __fusor_frame.branch_at(#slot, &#point, &#read, &#prepare)?; },
        };
    }
    if let Binding::ForEach {
        node,
        items,
        key,
        body,
    } = item
    {
        let slot = node.index();
        let node = element(*node);
        let read = format_ident!("__fusor_list_read_{}", slot);
        let key_fn = format_ident!("__fusor_list_key_{}", slot);
        let prepare = format_ident!("__fusor_list_prepare_{}", slot);
        let row_constructor = if components[*body].item_only_row {
            quote! { ::fusor_components::ForEach::item_row }
        } else {
            quote! { ::fusor_components::ForEach::row }
        };
        let row = component(&components[*body], delivery_metadata, components, has_ready);
        let ready_capture = has_ready.then(|| {
            quote! {
                let __fusor_read_ready = ::std::rc::Rc::clone(&ready);
                let __fusor_key_ready = ::std::rc::Rc::clone(&ready);
                let __fusor_row_ready = ::std::rc::Rc::clone(&ready);
            }
        });
        let read_ready = has_ready.then(|| quote! { let ready = &__fusor_read_ready; });
        let key_ready = has_ready.then(|| quote! { let ready = &__fusor_key_ready; });
        let row_ready = has_ready.then(|| quote! { let ready = &__fusor_row_ready; });
        let clones = clone_locals(locals);
        let ordinary_row = quote! { move |entry| #prepare(entry, &__fusor_parent) };
        let mount = if shared {
            quote! { __fusor_scope.keyed_hydrated(&#node, #read, #key_fn, #ordinary_row, |key| ::fusor_islands::encode(key).map_err(|error| ::fusor::dom::JsValue::from_str(&error.to_string())))?; }
        } else {
            quote! { __fusor_scope.keyed(&#node, #read, #key_fn, #ordinary_row)?; }
        };
        return BrowserBinding {
            shared: quote_spanned! {span=>
                let (#read, #key_fn, #prepare) = {
                    fn __fusor_list_parts<T, K, R, KF, F>(read: R, key: KF, prepare: F) -> (R, KF, F)
                    where
                        T: ::std::clone::Clone + ::std::cmp::PartialEq + 'static,
                        K: ::std::cmp::Ord + ::std::clone::Clone + 'static,
                        R: ::std::ops::Fn() -> ::std::vec::Vec<T>,
                        KF: ::std::ops::Fn(&T) -> K,
                        F: ::std::ops::Fn(::fusor::Signal<T>, &::fusor::OwnerHandle)
                            -> ::std::result::Result<::fusor::dom::Scope, ::fusor::dom::JsValue>,
                    {
                        (read, key, prepare)
                    }
                    #ready_capture
                    let __fusor_read_state = ::std::rc::Rc::clone(&state);
                    let __fusor_key_state = ::std::rc::Rc::clone(&state);
                    let __fusor_row_state = ::std::rc::Rc::clone(&state);
                    let __fusor_children = __fusor_children.clone();
                    __fusor_list_parts(
                        { #clones move || { #read_ready let state = &__fusor_read_state; ::fusor_components::ForEach::entries({ #items }) } },
                        { #clones move |entry| { #key_ready let state = &__fusor_key_state; ::fusor_components::ForEach::key(entry, #key) } },
                        { #clones move |entry, __fusor_parent| {
                            #row_ready
                            let state = #row_constructor(::std::rc::Rc::clone(&__fusor_row_state), entry);
                            #row
                        } }
                    )
                };
            },
            ordinary: quote_spanned! {span=> {
                let __fusor_parent = __fusor_scope.owner();
                #mount
            } },
            coherent: quote_spanned! {span=> __fusor_frame.keyed(#slot, #node.as_ref(), &#read, &#key_fn, &#prepare)?; },
        };
    }
    if let Binding::Invocation {
        point: id,
        inputs,
        children: Some(child),
        ..
    } = item
    {
        // Named Content has only an ordinary path, and its creation supplies
        // retained slot identity. Only share factories used by both modes.
        if !components[*child].empty
            && !inputs
                .iter()
                .any(|input| matches!(input.value, InputValue::Content { .. }))
        {
            let make = format_ident!("__fusor_make_children_{}", id.index());
            let children = children_factory(Some(*child), components, delivery_metadata, has_ready);
            let clones = clone_locals(&components[*child].async_locals);
            let ready = has_ready.then(|| quote! { let ready = ::std::rc::Rc::clone(&ready); });
            return BrowserBinding {
                shared: quote_spanned! {span=>
                    let #make = {
                        let state = ::std::rc::Rc::clone(&state);
                        let __fusor_children = __fusor_children.clone();
                        #clones
                        #ready
                        move || #children
                    };
                },
                ordinary: invocation(
                    item,
                    quote! {{ let __fusor_make_children = #make; __fusor_make_children() }},
                    false,
                    components,
                    delivery_metadata,
                    has_ready,
                    locals,
                ),
                coherent: invocation(
                    item,
                    quote! { #make() },
                    true,
                    components,
                    delivery_metadata,
                    has_ready,
                    locals,
                ),
            };
        }
    }
    if let Binding::Region {
        node,
        await_value: true,
        alias: Some(_),
        ..
    } = item
    {
        let render = format_ident!("__fusor_await_render_{}", node.index());
        let node = element(*node);
        let body = coherent_binding(item, has_ready, components, delivery_metadata, locals);
        let clones = clone_locals(locals);
        let ready = has_ready.then(|| quote! { let ready = ::std::rc::Rc::clone(&ready); });
        return BrowserBinding {
            shared: quote_spanned! {span=>
                let #render = {
                    #clones
                    #ready
                    let state = ::std::rc::Rc::clone(&state);
                    let __fusor_children = __fusor_children.clone();
                    let #node = #node.clone();
                    move |__fusor_frame: &mut ::fusor::dom::coherent::Frame<'_>| {
                        let __fusor_attempt = __fusor_frame.attempt;
                        #body
                        ::std::result::Result::<(), ::std::string::String>::Ok(())
                    }
                };
            },
            ordinary: quote_spanned! {span=> {
                let __fusor_region_root = #node.clone();
                __fusor_scope.async_region(&__fusor_region_root, ::fusor::coherence::AsyncBoundary::coherent(), #render)?;
            } },
            coherent: quote_spanned! {span=> #render(__fusor_frame)?; },
        };
    }
    BrowserBinding {
        shared: TokenStream::new(),
        ordinary: binding(item, components, delivery_metadata, has_ready, locals),
        coherent: coherent_binding(item, has_ready, components, delivery_metadata, locals),
    }
}

fn invocation(
    binding: &Binding,
    children: TokenStream,
    coherent: bool,
    components: &[Component],
    delivery_metadata: bool,
    has_ready: bool,
    locals: &[Rust],
) -> TokenStream {
    let Binding::Invocation {
        point: id,
        ty,
        inputs,
        condition,
        key,
        ..
    } = binding
    else {
        unreachable!("component invocation")
    };
    let span = binding.fragments()[0].span();
    if coherent
        && inputs
            .iter()
            .any(|input| matches!(input.value, InputValue::Content { .. }))
    {
        return quote_spanned! {span=> __fusor_frame.reject("projected content cannot participate in coherent rendering")?; };
    }
    let point = point(*id);
    let condition = condition
        .as_ref()
        .map(|value| quote! { #value })
        .unwrap_or_else(|| quote! { true });
    let key = key
        .as_ref()
        .map(|value| quote! { #value })
        .unwrap_or_else(|| quote! { () });
    let fields = inputs.iter().map(|input| {
        let name = &input.name;
        let value = match &input.value {
            InputValue::Expression(value) => quote_spanned! {value.span()=> { #value } },
            InputValue::Content {
                component: index,
                origin,
            } => {
                let prepare = component(
                    &components[*index],
                    delivery_metadata,
                    components,
                    has_ready,
                );
                quote_spanned! {origin.span()=> {
                    let __fusor_capture = ::std::rc::Rc::clone(state);
                    let __fusor_forward = __fusor_children.clone();
                    ::fusor::dom::Content::from_prepared(move |__fusor_parent| {
                        let state = ::std::rc::Rc::clone(&__fusor_capture);
                        let __fusor_children = __fusor_forward.clone();
                        #prepare
                    })
                }}
            }
        };
        quote_spanned! {name.span()=> #name: #value }
    });
    if coherent {
        let slot = id.index();
        return quote_spanned! {span=> __fusor_frame.component_at(#slot, &#point, if #condition { Some({ #key }) } else { None }, |owner| {
            type __FusorInputs = <#ty as ::fusor::dom::FromInputs>::Inputs;
            <#ty as ::fusor::dom::FromInputs>::from_inputs(__FusorInputs { #(#fields),* }, owner)
        }, #children)?; };
    }
    let local_clones = clone_locals(locals);
    quote_spanned! {span=> {
        #local_clones
        let __fusor_identity_state = ::std::rc::Rc::clone(&state);
        let __fusor_child_state = ::std::rc::Rc::clone(&state);
        let __fusor_supplied = #children;
        let __fusor_children = __fusor_children.clone();
        __fusor_scope.component_at_with_children(&#point, { #local_clones move || {
            let state = &__fusor_identity_state;
            if #condition { ::std::option::Option::Some({ #key }) }
            else { ::std::option::Option::None }
        }}, { #local_clones move |owner| {
            let state = &__fusor_child_state;
            type __FusorInputs = <#ty as ::fusor::dom::FromInputs>::Inputs;
            <#ty as ::fusor::dom::FromInputs>::from_inputs(__FusorInputs { #(#fields),* }, owner)
        }}, __fusor_supplied)?;
    }}
}

fn binding(
    binding: &Binding,
    components: &[Component],
    delivery_metadata: bool,
    has_ready: bool,
    locals: &[Rust],
) -> TokenStream {
    let span = binding.fragments()[0].span();
    let operation = match binding {
        Binding::Branch { .. }
        | Binding::ForEach { .. }
        | Binding::Region {
            await_value: true,
            alias: Some(_),
            ..
        } => {
            unreachable!("recursive browser bindings share their constructors across modes")
        }
        Binding::Router {
            point: id, routes, ..
        } => {
            let point = point(*id);
            let factories = routes.iter().map(|route| {
                let prepare = component(&components[route.body], delivery_metadata, components, false);
                let clones = clone_locals(locals);
                let params = route.params.as_ref().map(|alias| {
                    let names = &route.names;
                    let keys = names.iter().map(|name| name.tokens.to_string());
                    quote! {
                        #[derive(Clone)]
                        struct __Params { #(pub #names: ::std::string::String),* }
                        let #alias = __Params {
                            #(#names: __fusor_match.params.get(#keys)
                                .expect("validated route capture").clone()),*
                        };
                    }
                });
                let factory = quote! {
                    move |__fusor_parent: &::fusor::OwnerHandle,
                          __fusor_match: &::fusor_router::pattern::Match| {
                        let state = ::std::rc::Rc::clone(&__fusor_capture);
                        let __fusor_children = __fusor_forward.clone();
                        #clones
                        #params
                        #prepare
                    }
                };
                let construct = if let Some(path) = &route.path {
                    quote! { ::fusor_router::browser::declarative::RouteView::new(#path, #factory)? }
                } else {
                    quote! { ::fusor_router::browser::declarative::RouteView::fallback(#factory) }
                };
                quote! {{
                    let __fusor_capture = ::std::rc::Rc::clone(&state);
                    let __fusor_forward = __fusor_children.clone();
                    #clones
                    #construct
                }}
            });
            return quote_spanned! {span=> {
                ::fusor_router::browser::declarative::mount_routes(
                    &mut __fusor_scope, &#point, ::std::env!("FUSOR_BASE_PATH"),
                    ::std::vec![#(#factories),*]
                )?;
            }};
        }

        Binding::Children { point: id, .. } => {
            let point = point(*id);
            quote! { __fusor_scope.children_at(&#point, &__fusor_children)?; }
        }
        Binding::Invocation { children, .. } => {
            let children = children_factory(*children, components, delivery_metadata, has_ready);
            return invocation(
                binding,
                children,
                false,
                components,
                delivery_metadata,
                has_ready,
                locals,
            );
        }
        Binding::Island { .. } => {
            quote_spanned! {span=> return ::std::result::Result::Err(::fusor::dom::JsValue::from_str("independent islands must be rendered by the server")); }
        }
        Binding::Region {
            await_value: true,
            alias: None,
            ..
        } => {
            quote_spanned! {span=> return ::std::result::Result::Err(::fusor::dom::JsValue::from_str("rust:await requires a coherent boundary ancestor; use Await for independent loading")); }
        }
        Binding::Region {
            node,
            value,
            await_value: false,
            bindings,
            ..
        } => {
            let node = element(*node);
            let bindings = bindings.iter().map(|binding| {
                coherent_binding(binding, has_ready, components, delivery_metadata, locals)
            });
            quote_spanned! {span=>
                let __fusor_region_root = #node.clone();
                __fusor_scope.async_region(&__fusor_region_root, (#value).clone(), move |__fusor_frame| {
                    let __fusor_attempt = __fusor_frame.attempt;
                    #(#bindings)*
                    ::std::result::Result::Ok(())
                })?;
            }
        }
        Binding::Text { slot, value } => {
            let node = text(*slot);
            if typed_text_eligible(value) {
                quote_spanned! {span=> __fusor_scope.text_node_value(&#node, move || {
                    use ::fusor::dom::text_value::Convert as _;
                    (&::fusor::dom::text_value::Value(&(#value))).__fusor_into_text()
                })?; }
            } else {
                quote_spanned! {span=> __fusor_scope.text_node_string(&#node, move || ::std::string::ToString::to_string(&(#value)))?; }
            }
        }
        Binding::Attribute { node, name, value } => {
            let node = element(*node);
            let value = string(value);
            quote_spanned! {span=> __fusor_scope.attr(&#node, #name, move || ::std::option::Option::Some(#value))?; }
        }
        Binding::Property { node, name, value } => {
            let node = element(*node);
            quote_spanned! {span=> __fusor_scope.property(&#node, #name, move || { #value })?; }
        }
        Binding::Boolean { node, name, value } => {
            let node = element(*node);
            quote_spanned! {span=>
                __fusor_scope.attr(&#node, #name, move || {
                    let value: bool = { #value };
                    value.then(::std::string::String::new)
                })?;
            }
        }
        Binding::Value { node, value } => {
            let node = element(*node);
            let value = string(value);
            quote_spanned! {span=> __fusor_scope.value(&#node, move || { #value })?; }
        }
        Binding::Checked { node, value } => {
            let node = element(*node);
            quote_spanned! {span=> __fusor_scope.checked(&#node, move || { #value })?; }
        }
        Binding::Class { node, name, value } => {
            let node = element(*node);
            quote_spanned! {span=> __fusor_scope.class(&#node, #name, move || { #value })?; }
        }
        Binding::Event {
            node,
            name,
            handler,
        } => {
            let node = element(*node);
            quote_spanned! {span=> __fusor_scope.on(&#node, #name, move |event| { #handler })?; }
        }
        Binding::Field { node, value } => {
            let node = element(*node);
            quote_spanned! {span=> ::fusor_std::forms::browser::bind(&mut __fusor_scope, &#node, (#value).clone())?; }
        }
        Binding::Input { node, kind, value } => {
            let node = element(*node);
            match kind {
                InputKind::Value => {
                    quote_spanned! {span=> __fusor_scope.input(&#node, (#value).clone())?; }
                }
                InputKind::Checked => {
                    quote_spanned! {span=> __fusor_scope.checkbox(&#node, (#value).clone())?; }
                }
            }
        }
        Binding::Slot {
            node,
            content,
            condition,
            key,
        } => {
            let node = element(*node);
            let condition = condition
                .as_ref()
                .map(|value| quote_spanned! {value.span()=> #value })
                .unwrap_or_else(|| quote! { true });
            let pair = key
                .as_ref()
                .map(|value| quote_spanned! {value.span()=> ({ #value }, __fusor_content) })
                .unwrap_or_else(|| quote! { ((), __fusor_content) });
            quote_spanned! {span=>
                __fusor_scope.slot_with(&#node, move || {
                    if #condition {
                        let __fusor_content: ::std::option::Option<::fusor::dom::Content> =
                            ::std::convert::Into::into({ #content });
                        __fusor_content.map(|__fusor_content| #pair)
                    } else {
                        ::std::option::Option::None
                    }
                })?;
            }
        }
    };
    let ready = has_ready.then(|| quote! { let ready = ::std::rc::Rc::clone(&ready); });
    let locals = clone_locals(locals);
    quote_spanned! {span=> {
        #locals
        #ready
        let state = ::std::rc::Rc::clone(&state);
        let __fusor_children = __fusor_children.clone();
        #operation
    }}
}

fn component(
    component: &Component,
    delivery_metadata: bool,
    components: &[Component],
    has_ready: bool,
) -> TokenStream {
    let locals = &component.async_locals;
    let local_clones = clone_locals(locals);
    let server = (component.render != RenderTarget::Browser && component.capture.is_none())
        .then(|| super::server::component(component, components));
    if component.render == RenderTarget::Server && component.capture.is_none() {
        return quote! { #server };
    }
    if component.inline
        && component.elements.is_empty()
        && component.texts.is_empty()
        && component.text_elements.is_empty()
    {
        if let [
            Binding::Invocation {
                ty,
                inputs,
                children,
                condition: None,
                key: None,
                ..
            },
        ] = component.bindings.as_slice()
        {
            let fields = inputs.iter().map(|input| {
                let name = &input.name;
                let InputValue::Expression(value) = &input.value else {
                    unreachable!("scoped content rejected by parser")
                };
                quote! { #name: { #value } }
            });
            let supplied = children_factory(*children, components, delivery_metadata, has_ready);
            return quote! {{
                #local_clones
                let state = ::std::rc::Rc::new(state);
                let __fusor_supplied = #supplied;
                __fusor_supplied.with(|| <#ty as ::fusor::dom::Component>::prepare(__fusor_parent, move |owner| {
                    type __FusorInputs = <#ty as ::fusor::dom::FromInputs>::Inputs;
                    <#ty as ::fusor::dom::FromInputs>::from_inputs(__FusorInputs { #(#fields),* }, owner)
                }))
            }};
        }
    }
    let template::TemplateCode {
        declarations,
        typed_handles,
        bundle,
    } = template::lower(component, has_ready);
    let ty = &component.ty;
    let span = ty.span();
    let expose_capture = component.capture.as_ref().map(|_| {
        quote! {
            let state = ::std::rc::Rc::clone(state.as_ref());
        }
    });
    let hash = if delivery_metadata {
        super::server::hash(component, components)
    } else {
        String::new()
    };
    let html = if delivery_metadata || component.fragment {
        component.html.as_str()
    } else {
        ""
    };
    let target = (component.render == RenderTarget::Shared)
        .then(|| quote! { #[cfg(target_arch = "wasm32")] });
    let browser_bindings: Vec<_> = component
        .bindings
        .iter()
        .map(|item| {
            browser_binding(
                item,
                component.render == RenderTarget::Shared,
                components,
                delivery_metadata,
                has_ready,
                locals,
            )
        })
        .collect();
    let shared_bindings = browser_bindings.iter().map(|binding| &binding.shared);
    let bindings = browser_bindings.iter().map(|binding| &binding.ordinary);
    let coherent_bindings = browser_bindings.iter().map(|binding| &binding.coherent);
    let adoptions = component.bindings.iter().filter_map(|binding| match binding {
        Binding::Input { node, kind, value } => {
            let node = element(*node);
            Some(match kind {
                InputKind::Value => quote_spanned! {value.span()=> (#value).set(#node.value()); },
                InputKind::Checked => quote_spanned! {value.span()=> (#value).set(#node.checked()); },
            })
        }
        Binding::Field { node, value } => {
            let node = element(*node);
            Some(quote_spanned! {value.span()=> ::fusor_std::forms::browser::adopt(&__fusor_scope, &#node, &(#value))?; })
        }
        _ => None,
    });
    let template_impl = (component.kind == RootKind::Template).then(|| {
        quote_spanned! {span=>
            #target
            impl ::fusor::dom::TemplateComponent for #ty {}
        }
    });
    let template_html =
        if component.inline || component.capture.is_some() || component.app.is_some() {
            quote! { #html }
        } else {
            quote! { Self::TEMPLATE_HTML }
        };
    let mount_method = if component.fragment {
        format_ident!("prepare_fragment")
    } else {
        format_ident!("prepare_with_points")
    };
    let mount = if bundle.is_some() {
        quote! { __FUSOR_TEMPLATE.prepare_with_binding_bundle({ #template_html }, parent)? }
    } else {
        quote! { __FUSOR_TEMPLATE.#mount_method({ #template_html }, __FUSOR_MOUNTS, parent)? }
    };
    let install = quote! {
        #typed_handles
        if __fusor_scope.is_hydrating() { #(#adoptions)* }
        #(#shared_bindings)*
        if __fusor_scope.is_coherent() {
            __fusor_scope.set_coherent_renderer(move |__fusor_frame| {
                let __fusor_attempt = __fusor_frame.attempt;
                #(#coherent_bindings)*
                ::std::result::Result::Ok(())
            });
        } else {
            #(#bindings)*
        }
    };
    let install = if let Some(bindings) = bundle {
        quote! {
            if let ::std::option::Option::Some(__fusor_bundle) = __fusor_nodes.take_binding_bundle() {
                #(#bindings)*
            } else {
                #install
            }
        }
    } else {
        install
    };
    let incoming = if component.capture.is_none() && !component.inline {
        quote! { let __fusor_children = ::fusor::dom::Children::take(); }
    } else {
        quote! { let __fusor_children = __fusor_children.clone(); }
    };
    let capture_ready = has_ready.then(|| quote! { let ready = ::std::rc::Rc::clone(&ready); });
    let javascript_inputs = component.javascript.as_ref().map(|_| {
        quote! {
            let __fusor_js_inputs = {
                use ::fusor::js::MaybeInputs as _;
                ::fusor::js::InputSource(&*state).inputs()
            };
        }
    });
    let javascript = component.javascript.as_ref().map(|module| {
        let id = &module.id;
        quote! { ::fusor::js::mount(&mut __fusor_scope, #id, __fusor_js_inputs)?; }
    });
    let prepare = quote! {
                #local_clones
                #capture_ready
                #incoming
                let __fusor_mount_guard = ::fusor::dom::MountGuard::enter()?;
                #declarations
                let (mut __fusor_scope, mut __fusor_nodes) = #mount;
                let state = if __fusor_scope.prepares_effects() {
                    ::fusor::coherence::prepare_state(__fusor_scope.owner(), make)?
                } else { make(__fusor_scope.owner())? };
                let state = __fusor_scope.retain_state(state);
                #expose_capture
                #javascript_inputs
                #install
                #javascript
                ::std::result::Result::Ok(__fusor_scope)
    };
    if let Some(expression) = &component.app {
        return quote_spanned! {expression.span()=>
            #[allow(unused_variables, unused_braces, unused_parens, clippy::let_and_return, clippy::needless_borrows_for_generic_args, clippy::clone_on_copy, clippy::unused_unit, clippy::unit_arg, clippy::needless_ifs, clippy::needless_else)]
            pub(crate) fn __fusor_mount() -> ::std::result::Result<(), ::fusor::dom::JsValue> {
                ::fusor_components::App::mount(|| {
                    let parent = ::std::option::Option::None;
                    let make = |owner: ::fusor::OwnerHandle| ::std::result::Result::<_, ::fusor::dom::JsValue>::Ok({ #expression });
                    #prepare
                })
            }
            #[::wasm_bindgen::prelude::wasm_bindgen(start)]
            pub fn __fusor_start() -> ::std::result::Result<(), ::fusor::dom::JsValue> {
                __fusor_mount()
            }
        };
    }
    if component.capture.is_some() {
        return quote! {{
            let parent = ::std::option::Option::Some(__fusor_parent);
            let make = move |_owner| ::std::result::Result::<_, ::fusor::dom::JsValue>::Ok(state);
            #prepare
        }};
    }
    if component.inline {
        return quote! {{
            let parent = ::std::option::Option::Some(__fusor_parent);
            let make = move |_owner| ::std::result::Result::<_, ::fusor::dom::JsValue>::Ok(state);
            #prepare
        }};
    }
    quote_spanned! {span=>
        #template_impl
        #server
        #target
        impl ::fusor::dom::Component for #ty {
            const TEMPLATE_HASH: &'static str = #hash;
            const TEMPLATE_HTML: &'static str = #html;
            #[allow(unused_variables, unused_braces, unused_parens, clippy::let_and_return, clippy::needless_borrows_for_generic_args, clippy::clone_on_copy, clippy::unused_unit, clippy::unit_arg, clippy::needless_ifs, clippy::needless_else)]
            fn mount(self) -> ::std::result::Result<::fusor::dom::Scope, ::fusor::dom::JsValue> {
                <Self as ::fusor::dom::Component>::try_mount_with(|_| ::std::result::Result::Ok(self))
            }
            #[allow(unused_variables, unused_braces, unused_parens, clippy::let_and_return, clippy::needless_borrows_for_generic_args, clippy::clone_on_copy, clippy::unused_unit, clippy::unit_arg, clippy::needless_ifs, clippy::needless_else)]
            fn prepare_component(
                parent: ::std::option::Option<&::fusor::OwnerHandle>,
                make: ::fusor::dom::ComponentFactory<'_, Self>,
            ) -> ::std::result::Result<::fusor::dom::Scope, ::fusor::dom::JsValue> {
                #prepare
            }
        }
    }
}

fn branch_factories(
    cases: &[CaseBranch],
    snapshots: &[(Rust, Rust)],
    components: &[Component],
    delivery_metadata: bool,
    has_ready: bool,
) -> TokenStream {
    let factories = cases.iter().enumerate().map(|(index, case)| {
        let projections = super::control::projections(case, index, snapshots);
        let body = component(
            &components[case.body],
            delivery_metadata,
            components,
            has_ready,
        );
        quote! { #index => { #projections #body }, }
    });
    quote! { #(#factories)* }
}

fn coherent_binding(
    binding: &Binding,
    has_ready: bool,
    components: &[Component],
    delivery_metadata: bool,
    locals: &[Rust],
) -> TokenStream {
    let span = binding.fragments()[0].span();
    match binding {
        Binding::Branch {
            point: id,
            value,
            cases,
            snapshots,
        } => {
            let slot = id.index();
            let point = point(*id);
            let selection = super::control::selection(value, cases, snapshots);
            let factories =
                branch_factories(cases, snapshots, components, delivery_metadata, has_ready);
            quote_spanned! {span=> __fusor_frame.branch_at(#slot, &#point, || { #selection },
            |__fusor_case, __fusor_data, __fusor_parent| {
                let state = ::std::rc::Rc::clone(&state);
                match __fusor_case { #factories _ => unreachable!("generated branch index") }
            })?; }
        }

        Binding::Children { point: id, .. } => {
            let slot = id.index();
            let point = point(*id);
            quote! { __fusor_frame.children_at(#slot, &#point, &__fusor_children)?; }
        }
        Binding::ForEach {
            node,
            items,
            key,
            body,
        } => {
            let slot = node.index();
            let node = element(*node);
            let row_constructor = if components[*body].item_only_row {
                quote! { ::fusor_components::ForEach::item_row }
            } else {
                quote! { ::fusor_components::ForEach::row }
            };
            let row = component(&components[*body], delivery_metadata, components, has_ready);
            quote_spanned! {span=> __fusor_frame.keyed(#slot, #node.as_ref(), || ::fusor_components::ForEach::entries({ #items }),
            |entry| ::fusor_components::ForEach::key(entry, #key), |entry, __fusor_parent| {
                let state = #row_constructor(::std::rc::Rc::clone(&state), entry);
                #row
            })?; }
        }
        Binding::Invocation { children, .. } => {
            let children = children_factory(*children, components, delivery_metadata, has_ready);
            invocation(
                binding,
                children,
                true,
                components,
                delivery_metadata,
                has_ready,
                locals,
            )
        }
        Binding::Region {
            value,
            await_value: true,
            alias,
            bindings,
            ..
        } => {
            let mut nested = locals.to_vec();
            if let Some(alias) = alias {
                nested.push(alias.clone());
            }
            let resolved = alias
                .as_ref()
                .map(|name| quote! { #name })
                .unwrap_or_else(|| quote! { ready });
            let bindings = bindings.iter().map(|binding| {
                coherent_binding(
                    binding,
                    has_ready || alias.is_none(),
                    components,
                    delivery_metadata,
                    &nested,
                )
            });
            quote_spanned! {span=>
                if let ::fusor_async::AsyncRead::Ready(#resolved) = (#value).read(__fusor_attempt)? {
                    #(#bindings)*
                }
            }
        }
        Binding::Region { .. } => {
            quote_spanned! {span=> __fusor_frame.reject("nested coherent boundaries are unsupported")?; }
        }
        Binding::Text { slot, value } => {
            let node = text(*slot);
            quote_spanned! {span=> __fusor_frame.text(&#node, &(#value))?; }
        }
        Binding::Attribute { node, name, value } => {
            let node = element(*node);
            let value = string(value);
            quote_spanned! {span=> __fusor_frame.attr(#node.as_ref(), #name, ::std::option::Option::Some(#value))?; }
        }
        Binding::Boolean { node, name, value } => {
            let node = element(*node);
            quote_spanned! {span=> __fusor_frame.attr(#node.as_ref(), #name, ({ #value }).then(::std::string::String::new))?; }
        }
        Binding::Class { node, name, value } => {
            let node = element(*node);
            quote_spanned! {span=> __fusor_frame.class(#node.as_ref(), #name, { #value })?; }
        }
        Binding::Event {
            node,
            name,
            handler,
        } => {
            let node = element(*node);
            let ready = has_ready.then(|| quote! { let ready = ::std::rc::Rc::clone(&ready); });
            let locals = clone_locals(locals);
            quote_spanned! {span=> {
                #locals
                let state = ::std::rc::Rc::clone(&state);
                #ready
                __fusor_frame.on(#node.as_ref(), #name, move |event| { #handler })?;
            }}
        }
        Binding::Router { .. }
        | Binding::Island { .. }
        | Binding::Property { .. }
        | Binding::Value { .. }
        | Binding::Checked { .. }
        | Binding::Input { .. }
        | Binding::Field { .. }
        | Binding::Slot { .. } => {
            quote_spanned! {span=> __fusor_frame.reject("editable controls, widgets, outlets and opaque content must remain outside coherent regions")?; }
        }
    }
}

pub(super) fn generate(
    source: &str,
    components: &[Component],
    rust: &mut String,
    delivery_metadata: bool,
) -> Vec<BindingLocation> {
    let mut origins = Origins::default();
    for component in components {
        origins.register(&component.ty);
        if let Some(app) = &component.app {
            origins.register(app);
        }
        for fragment in component.bindings.iter().flat_map(Binding::fragments) {
            origins.register(fragment);
        }
    }
    let mut locations = Vec::new();
    for item in components
        .iter()
        .filter(|item| !item.inline && item.capture.is_none())
    {
        locations.extend(tokens::emit(
            source,
            rust,
            component(item, delivery_metadata, components, false),
            &origins,
            item.ty.offset,
        ));
    }
    locations
}

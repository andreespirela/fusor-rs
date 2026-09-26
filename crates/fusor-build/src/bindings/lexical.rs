//! Rewrite lexical captures shared by lists, routes, Case and Await.
use super::{
    emit::{clone_locals, indexed},
    ir::*,
    tokens::Rust,
};
use quote::quote;

pub(super) fn rewrite(component: &mut Component) {
    if component.locals.is_empty()
        && component.route_locals.is_empty()
        && component.snapshot_locals.is_empty()
    {
        return;
    }
    let wrap = |value: &mut Rust| {
        let original = &value.tokens;
        let mut aliases = Vec::new();
        let params = &component.route_locals;
        let snapshots = &component.snapshot_locals;
        let snapshot_reads = quote! { #(let #snapshots = #snapshots.get();)* };
        let params = clone_locals(params);
        if component.locals.is_empty() {
            value.tokens = quote! {{ #snapshot_reads #params #original }};
            return;
        }
        aliases.push(quote! { #snapshot_reads #params });
        let depth = component.locals.len();
        let contexts: Vec<_> = (0..depth).map(|i| indexed("context", i)).collect();
        let first = &contexts[0];
        aliases.push(quote! { let #first = &state; });
        for i in 1..depth {
            let prev = &contexts[i - 1];
            let current = &contexts[i];
            aliases.push(quote! { let #current = &#prev.parent; });
        }
        for ((item, index), context) in component.locals.iter().zip(contexts.iter().rev()) {
            let index =
                (!component.item_only_row).then(|| quote! { let #index = &#context.index; });
            aliases.push(quote! { let #item = &#context.item; #index });
        }
        let outer = contexts.last().unwrap();
        value.tokens = quote! { { #(#aliases)* let state = &#outer.parent; #original } };
    };
    let wrap_text = |value: &mut Rust| {
        // Format before the lexical block ends: a string slice may borrow a
        // freshly read capture or a cloned route parameter inside that block.
        if !component.snapshot_locals.is_empty() || !component.route_locals.is_empty() {
            let original = &value.tokens;
            value.tokens = quote! { ::std::string::ToString::to_string(&(#original)) };
        }
        wrap(value);
    };
    fn visit(bindings: &mut [Binding], wrap: &impl Fn(&mut Rust), wrap_text: &impl Fn(&mut Rust)) {
        for binding in bindings {
            match binding {
                Binding::Children { .. } | Binding::Router { .. } => {}
                Binding::Branch { value, .. } => wrap(value),
                Binding::ForEach { items, key, .. } => {
                    wrap(items);
                    wrap(key);
                }
                Binding::Invocation {
                    inputs,
                    condition,
                    key,
                    ..
                } => {
                    for input in inputs {
                        if let InputValue::Expression(value) = &mut input.value {
                            wrap(value);
                        }
                    }
                    if let Some(v) = condition {
                        wrap(v)
                    }
                    if let Some(v) = key {
                        wrap(v)
                    }
                }
                Binding::Region {
                    value, bindings, ..
                } => {
                    wrap(value);
                    visit(bindings, wrap, wrap_text);
                }
                Binding::Text { value, .. } => wrap_text(value),
                Binding::Property { value, .. }
                | Binding::Boolean { value, .. }
                | Binding::Checked { value, .. }
                | Binding::Class { value, .. }
                | Binding::Input { value, .. }
                | Binding::Field { value, .. } => wrap(value),
                Binding::Attribute { value, .. } | Binding::Value { value, .. } => {
                    for part in &mut value.0 {
                        if let StringPart::Expression(value) = part {
                            wrap_text(value)
                        }
                    }
                }
                Binding::Event { handler, .. } => wrap(handler),
                Binding::Slot {
                    content: constructor,
                    condition,
                    key,
                    ..
                } => {
                    wrap(constructor);
                    if let Some(v) = condition {
                        wrap(v)
                    }
                    if let Some(v) = key {
                        wrap(v)
                    }
                }
                Binding::Island { inputs, .. } => {
                    for input in inputs {
                        if let InputValue::Expression(value) = &mut input.value {
                            wrap(value);
                        }
                    }
                }
            }
        }
    }
    visit(&mut component.bindings, &wrap, &wrap_text);
}

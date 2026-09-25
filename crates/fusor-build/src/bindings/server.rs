//! Native lowering walks a structural HTML token stream. Dynamic Rust remains
//! native token trees with the same source spans as the browser target.
use super::{ir::*, tokens::Rust};
use fusor::template::{self, ElementId, MountMarker, RootKind, TextId, TextMarker};
use html5gum::{DefaultEmitter, Token, Tokenizer};
use proc_macro2::TokenStream;
use quote::{quote, quote_spanned};
use sha2::{Digest, Sha256};

// Passed immediately to the escaped writer, so borrowed formatting arguments
// do not escape their source expression's statement.
fn string(value: &InterpolatedString) -> TokenStream {
    let mut format = String::new();
    let mut expressions = Vec::new();
    for part in &value.0 {
        match part {
            StringPart::Literal(text) => {
                format.push_str(&text.replace('{', "{{").replace('}', "}}"));
            }
            StringPart::Expression(expression) => {
                format.push_str("{}");
                expressions.push(expression);
            }
        }
    }
    quote! { ::std::format_args!(#format #(, (#expressions))*) }
}

pub(super) fn hash(component: &Component, components: &[Component]) -> String {
    let mut hash = Sha256::new();
    hash.update(component.html.as_bytes());
    for fragment in component.bindings.iter().flat_map(Binding::fragments) {
        hash.update(fragment.tokens.to_string());
        hash.update([0]);
    }
    fn descendants(bindings: &[Binding], components: &[Component], digest: &mut Sha256) {
        for binding in bindings {
            match binding {
                Binding::Invocation {
                    children: Some(index),
                    ..
                } => digest.update(self::hash(&components[*index], components)),
                Binding::Router { routes, .. } => {
                    for route in routes {
                        digest.update(route.path.as_deref().unwrap_or("<fallback>").as_bytes());
                        digest.update([0]);
                        if let Some(alias) = &route.params {
                            digest.update(alias.tokens.to_string());
                        }
                        digest.update(self::hash(&components[route.body], components));
                    }
                }
                Binding::Branch { cases, .. } => {
                    for case in cases {
                        digest.update(self::hash(&components[case.body], components));
                    }
                }
                Binding::ForEach { body, .. } => {
                    digest.update(self::hash(&components[*body], components))
                }
                Binding::Region { bindings, .. } => descendants(bindings, components, digest),
                _ => {}
            }
        }
    }
    descendants(&component.bindings, components, &mut hash);
    format!("{:x}", hash.finalize())
}

fn node(binding: &Binding) -> Option<ElementId> {
    match binding {
        Binding::Text { .. }
        | Binding::Invocation { .. }
        | Binding::Children { .. }
        | Binding::Router { .. }
        | Binding::Branch { .. } => None,
        Binding::ForEach { node, .. }
        | Binding::Island { node, .. }
        | Binding::Region { node, .. }
        | Binding::Attribute { node, .. }
        | Binding::Property { node, .. }
        | Binding::Boolean { node, .. }
        | Binding::Value { node, .. }
        | Binding::Checked { node, .. }
        | Binding::Class { node, .. }
        | Binding::Event { node, .. }
        | Binding::Input { node, .. }
        | Binding::Field { node, .. }
        | Binding::Slot { node, .. } => Some(*node),
    }
}

fn construct_child(
    ty: &Rust,
    inputs: &[Input],
    children: Option<usize>,
    components: &[Component],
    into: bool,
) -> TokenStream {
    let fields = inputs.iter().map(|input| {
        let name = &input.name;
        let InputValue::Expression(value) = &input.value else {
            unreachable!("server projected content rejected by parser")
        };
        quote_spanned! {name.span()=> #name: { #value } }
    });
    let body = children
        .filter(|index| !components[*index].empty)
        .map(|index| component_body(&components[index], components, false));
    let content = body.map(|body| quote! { Some(&(|__fusor_context: &mut ::fusor_server::Context<'_>| { #body }) as &::fusor_server::Children<'_>) }).unwrap_or_else(|| quote! { None });
    let method = if into {
        quote! { try_child_into_with_children }
    } else {
        quote! { try_child_with_children }
    };
    let writer = into.then(|| quote! { , __fusor_writer });
    quote_spanned! {ty.span()=> __fusor_context.#method(|owner| {
        type __FusorInputs = <#ty as ::fusor::dom::FromInputs>::Inputs;
        <#ty as ::fusor::dom::FromInputs>::from_inputs(__FusorInputs { #(#fields),* }, owner)
            .map_err(|_| ::std::string::String::from(concat!("component ", stringify!(#ty), " input construction failed")))
    }, #content #writer) }
}

/// Accumulate only static syntax. A dynamic statement flushes the exact prefix
/// before evaluating authored Rust, preserving output and error ordering.
#[derive(Default)]
struct Emission {
    statements: Vec<TokenStream>,
    markup: String,
    first_open: Option<usize>,
    editable: bool,
}

impl Emission {
    fn literal(&mut self, value: &str) {
        self.markup.push_str(value);
    }

    fn open(&mut self, tag: &str) {
        self.literal("<");
        self.literal(tag);
        self.first_open.get_or_insert(self.markup.len());
        self.editable |= matches!(tag, "input" | "textarea" | "select");
    }

    fn flush(&mut self) {
        if self.markup.is_empty() {
            return;
        }
        let markup = std::mem::take(&mut self.markup);
        let first_open = match self.first_open.take() {
            Some(offset) => quote! { ::std::option::Option::Some(#offset) },
            None => quote! { ::std::option::Option::None },
        };
        let editable = std::mem::take(&mut self.editable);
        self.statements.push(quote! {
            __fusor_writer.static_markup(#markup, #first_open, #editable);
        });
    }

    fn push(&mut self, statement: TokenStream) {
        if !statement.is_empty() {
            self.flush();
            self.statements.push(statement);
        }
    }

    fn finish(mut self) -> Vec<TokenStream> {
        self.flush();
        self.statements
    }
}

fn component_body(component: &Component, components: &[Component], into: bool) -> TokenStream {
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
            return construct_child(ty, inputs, *children, components, into);
        }
    }
    let mut emitter = DefaultEmitter::<usize>::new_with_span();
    emitter.naively_switch_states(true);
    let mut body = Emission::default();
    if component.fragment {
        body.literal("<!--fusor:fragment-->");
    }
    let mut depth = 0;
    let mut raw = false;
    let mut first = true;
    for token in Tokenizer::new_with_emitter(component.html.as_str(), emitter) {
        match token.expect("compiled HTML") {
            Token::StartTag(tag) => {
                let name = String::from_utf8_lossy(&tag.name).into_owned();
                if name == "template" && depth == 0 && component.kind == RootKind::Template {
                    depth += 1;
                    continue;
                }
                let id = tag
                    .attributes
                    .get(template::ELEMENT_ATTRIBUTE.as_bytes())
                    .and_then(|id| String::from_utf8_lossy(id).parse::<ElementId>().ok());
                let bindings: Vec<_> = component
                    .bindings
                    .iter()
                    .filter(|binding| id.is_some() && node(binding) == id)
                    .collect();
                let island = bindings
                    .iter()
                    .find(|binding| matches!(binding, Binding::Island { .. }));
                if let Some(Binding::Island {
                    descriptor,
                    props,
                    activation,
                    prefetch,
                    ..
                }) = island
                {
                    let span = descriptor.span();
                    let activation = syn::Ident::new(&title(activation), span);
                    let prefetch = syn::Ident::new(&title(prefetch), span);
                    let id = if let Some(Binding::Attribute { value, .. }) = bindings.iter().find(|binding| matches!(binding, Binding::Attribute { name, .. } if name == "id")) {
                        let value = super::codegen::string(value); quote! { ::std::option::Option::Some(#value) }
                    } else if let Some(value) = tag.attributes.get(b"id".as_slice()) {
                        let value = String::from_utf8_lossy(value).into_owned(); quote! { ::std::option::Option::Some(::std::string::String::from(#value)) }
                    } else { quote! { ::std::option::Option::<::std::string::String>::None } };
                    body.push(quote_spanned! {span=>
                        let __fusor_island_id = #id;
                        let __fusor_island = __fusor_context.prepare_island::<#descriptor>(__fusor_island_id.as_deref(), &{ #props }, ::fusor_islands::Activation::#activation, ::fusor_islands::Prefetch::#prefetch)?;
                    });
                }
                let sensitive = name == "input"
                    && tag.attributes.get(b"type".as_slice()).is_some_and(|value| {
                        matches!(
                            String::from_utf8_lossy(value).to_ascii_lowercase().as_str(),
                            "password" | "file"
                        )
                    });
                body.open(&name);
                let mut static_attributes = String::new();
                let mut editable = false;
                for (key, value) in &tag.attributes {
                    let key = String::from_utf8_lossy(key).into_owned();
                    let value = String::from_utf8_lossy(value).into_owned();
                    if sensitive && key == "value"
                        || island.is_some() && key == "id"
                        || key == "class"
                            && bindings
                                .iter()
                                .any(|binding| matches!(binding, Binding::Class { .. }))
                    {
                        continue;
                    }
                    editable |= static_attribute(&mut static_attributes, &key, &value);
                }
                if first && component.kind == RootKind::Template && !component.fragment {
                    let id = component.id.to_string();
                    let version = template::VERSION.to_string();
                    static_attribute(&mut static_attributes, "data-fusor-component", &id);
                    static_attribute(&mut static_attributes, "data-fusor-version", &version);
                }
                body.literal(&static_attributes);
                body.editable |= editable;
                first = false;
                let mut content = Vec::new();
                if let Some(slot) = tag
                    .attributes
                    .get(template::TEXT_ELEMENT_ATTRIBUTE.as_bytes())
                    .and_then(|id| String::from_utf8_lossy(id).parse::<TextId>().ok())
                {
                    if let Some(Binding::Text { value, .. }) = component.bindings.iter().find(
                        |binding| matches!(binding, Binding::Text { slot: text, .. } if *text == slot),
                    ) {
                        content.push(quote_spanned! {value.span()=> __fusor_writer.text(&(#value)); });
                    }
                }
                for binding in &bindings {
                    let span = binding.fragments()[0].span();
                    let operation = match binding {
                        Binding::Attribute { name, value, .. }
                            if island.is_none() || name != "id" =>
                        {
                            let value = string(value);
                            quote_spanned! {span=> __fusor_writer.attr(#name, #value); }
                        }
                        Binding::Boolean { name, value, .. } => {
                            quote_spanned! {span=> __fusor_writer.boolean(#name, { #value }); }
                        }
                        Binding::Checked { value, .. } => {
                            quote_spanned! {span=> __fusor_writer.boolean("checked", { #value }); }
                        }
                        Binding::Value { value, .. } if !sensitive => {
                            let value = string(value);
                            quote_spanned! {span=> __fusor_writer.attr("value", #value); }
                        }
                        Binding::Input {
                            kind: InputKind::Value,
                            value,
                            ..
                        } if !sensitive => {
                            quote_spanned! {span=> __fusor_writer.attr("value", (#value).get()); }
                        }
                        Binding::Input {
                            kind: InputKind::Checked,
                            value,
                            ..
                        } => {
                            quote_spanned! {span=> __fusor_writer.boolean("checked", (#value).get()); }
                        }
                        Binding::Field { value, .. } if !sensitive => {
                            if name == "textarea" {
                                content.push(
                                    quote_spanned! {span=> __fusor_writer.text((#value).raw()); },
                                );
                                quote! {}
                            } else {
                                quote_spanned! {span=> __fusor_writer.attr("value", (#value).raw()); }
                            }
                        }
                        Binding::ForEach {
                            items,
                            key,
                            body: row,
                            ..
                        } => {
                            let row_constructor = if components[*row].item_only_row {
                                quote! { ::fusor_components::ForEach::server_item_row }
                            } else {
                                quote! { ::fusor_components::ForEach::server_row }
                            };
                            let row = component_body(&components[*row], components, true);
                            content.push(quote_spanned! {span=> {
                                let __fusor_items = ::fusor_components::ForEach::entries({ #items });
                                let mut __fusor_keys = ::std::collections::BTreeSet::new();
                                for __fusor_entry in __fusor_items {
                                    let __fusor_key = ::fusor_components::ForEach::key(&__fusor_entry, #key);
                                    if !__fusor_keys.insert(__fusor_key.clone()) { return ::std::result::Result::Err("duplicate key in ForEach".into()); }
                                    __fusor_writer.keyed_child(&__fusor_key, |mut __fusor_writer| {
                                        let state = #row_constructor(state, __fusor_entry);
                                        let state = &state;
                                        #row
                                    })?;
                                }
                            }});
                            quote! {}
                        }
                        Binding::Island { .. } => {
                            // The same lowering serves owned root Writers and
                            // borrowed nested Writers; the latter reborrow is
                            // intentional. Scope the lint to framework calls.
                            content.push(
                                quote_spanned! {span=> #[allow(clippy::needless_borrow)] __fusor_island.contents(&mut __fusor_writer); },
                            );
                            quote! { #[allow(clippy::needless_borrow)] __fusor_island.attributes(&mut __fusor_writer); }
                        }
                        // Guarded values omit sensitive inputs and island-owned IDs.
                        Binding::Attribute { .. } | Binding::Value { .. }
                        | Binding::Input { kind: InputKind::Value, .. } | Binding::Field { .. }
                        // Text, classes and structural anchors are emitted separately.
                        | Binding::Text { .. } | Binding::Class { .. } | Binding::Branch { .. }
                        | Binding::Children { .. } | Binding::Invocation { .. }
                        // Events intentionally have no server effect; these browser-only
                        // operations are rejected by template validation where applicable.
                        | Binding::Event { .. } | Binding::Property { .. } | Binding::Region { .. }
                        | Binding::Router { .. } | Binding::Slot { .. } => quote! {},
                    };
                    body.push(operation);
                }
                if bindings
                    .iter()
                    .any(|binding| matches!(binding, Binding::Class { .. }))
                {
                    let initial = tag
                        .attributes
                        .get(b"class".as_slice())
                        .map(|value| String::from_utf8_lossy(value).into_owned())
                        .unwrap_or_default();
                    let classes = bindings.iter().filter_map(|binding| match binding { Binding::Class { name, value, .. } => Some(quote_spanned! {value.span()=> if #value { __fusor_classes.push(' '); __fusor_classes.push_str(#name); } }), _ => None });
                    body.push(quote! { { let mut __fusor_classes = ::std::string::String::from(#initial); #(#classes)* __fusor_writer.attr("class", __fusor_classes.trim()); } });
                }
                body.literal(">");
                body.push(quote! { #(#content)* });
                raw = matches!(name.as_str(), "script" | "style");
                if !super::tags::void_element(&name) {
                    depth += 1;
                }
            }
            Token::EndTag(tag) => {
                depth -= 1;
                let name = String::from_utf8_lossy(&tag.name).into_owned();
                if name == "template" && depth == 0 && component.kind == RootKind::Template {
                    continue;
                }
                body.literal(&format!("</{name}>"));
                raw = false;
            }
            Token::String(value) => {
                // Browser template mounting selects the single element root.
                // Formatting outside that root is not part of the component.
                if component.kind == RootKind::Template && !component.fragment && depth <= 1 {
                    continue;
                }
                let value = String::from_utf8_lossy(&value).into_owned();
                let value = if raw {
                    value
                } else {
                    let mut escaped = String::new();
                    template::escape_into(&mut escaped, &value, false);
                    escaped
                };
                body.literal(&value);
            }
            Token::Comment(comment) => {
                if component.kind == RootKind::Template && !component.fragment && depth <= 1 {
                    continue;
                }
                let value = String::from_utf8_lossy(&comment);
                let literal = format!("<!--{value}-->");
                body.literal(&literal);
                if let Ok(Some(MountMarker::Start(id))) = MountMarker::parse(&value) {
                    if let Some(Binding::Branch { value, cases, .. }) = component.bindings.iter().find(|binding| matches!(binding, Binding::Branch { point, .. } if *point == id)) {
                        let arms = cases.iter().enumerate().map(|(index, case)| {
                            let pattern = &case.pattern;
                            let captures = case.names.iter().map(|name| quote! { let #name = ::fusor::memo(move || #name.clone()); });
                            let child = component_body(&components[case.body], components, true);
                            let marker = format!("<!--fusor:branch:{index}-->");
                            quote! { #pattern => { #(#captures)* __fusor_writer.static_markup(#marker, None, false); #child?; } }
                        });
                        body.push(quote_spanned! {value.span()=> { let __fusor_value = { #value }; #[deny(non_snake_case)] match __fusor_value { #(#arms),* } } });
                    }
                    if component.bindings.iter().any(|binding| matches!(binding, Binding::Children { point, .. } if *point == id)) {
                        body.push(quote! { if let Some(children) = __fusor_children { let child = children(__fusor_context)?; __fusor_writer.child(&child); } });
                    }
                    if let Some(Binding::Invocation { ty, inputs, children, condition, .. }) = component.bindings.iter().find(|binding| matches!(binding, Binding::Invocation { point, .. } if *point == id)) {
                        let condition = condition.as_ref().map(|v| quote! { #v }).unwrap_or_else(|| quote! { true });
                        let child = construct_child(ty, inputs, *children, components, true);
                        body.push(quote_spanned! {ty.span()=> if #condition {
                            __fusor_writer.child_into(|__fusor_writer| #child)?;
                        } });
                    }
                }
                if let Ok(Some(TextMarker::Start(id))) = TextMarker::parse(&value) {
                    if let Some(Binding::Text { value, .. }) = component.bindings.iter().find(
                        |binding| matches!(binding, Binding::Text { slot, .. } if *slot == id),
                    ) {
                        body.push(quote_spanned! {value.span()=> __fusor_writer.text(&(#value)); });
                    }
                }
            }
            _ => {}
        }
    }
    if component.fragment {
        body.literal("<!--/fusor:fragment-->");
    }
    let body = body.finish();
    if into {
        quote! {{ #(#body)* ::std::result::Result::<(), ::std::string::String>::Ok(()) }}
    } else {
        quote! {{
            let mut __fusor_writer = ::fusor_server::Writer::new();
            #(#body)*
            ::std::result::Result::<_, ::std::string::String>::Ok(__fusor_writer.finish())
        }}
    }
}

pub(super) fn component(component: &Component, components: &[Component]) -> TokenStream {
    let ty = &component.ty;
    let span = ty.span();
    let hash = hash(component, components);
    let body = component_body(component, components, true);
    quote_spanned! {span=>
        #[cfg(not(target_arch = "wasm32"))]
        impl ::fusor_server::Render for #ty {
            const TEMPLATE_HASH: &'static str = #hash;
            #[allow(unused_variables, unused_braces, unused_parens, clippy::let_and_return, clippy::needless_borrows_for_generic_args, clippy::clone_on_copy)]
            fn render(&self, __fusor_context: &mut ::fusor_server::Context<'_>) -> ::fusor_server::Result<::fusor_server::Html> {
                self.render_with_children(__fusor_context, None)
            }
            #[allow(unused_variables, unused_braces, unused_parens, clippy::let_and_return, clippy::needless_borrows_for_generic_args, clippy::clone_on_copy)]
            fn render_with_children(&self, __fusor_context: &mut ::fusor_server::Context<'_>, __fusor_children: Option<&::fusor_server::Children<'_>>) -> ::fusor_server::Result<::fusor_server::Html> {
                let mut writer = ::fusor_server::Writer::new();
                ::fusor_server::Render::render_into(self, __fusor_context, __fusor_children, &mut writer)?;
                Ok(writer.finish())
            }
            #[allow(unused_variables, unused_mut, unused_braces, unused_parens, clippy::let_and_return, clippy::needless_borrows_for_generic_args, clippy::clone_on_copy)]
            fn render_into(&self, __fusor_context: &mut ::fusor_server::Context<'_>, __fusor_children: Option<&::fusor_server::Children<'_>>, mut __fusor_writer: &mut ::fusor_server::Writer) -> ::fusor_server::Result<()> {
                let state = self;
                #body
            }
        }
    }
}

fn title(value: &str) -> String {
    let mut value = value.to_owned();
    value[..1].make_ascii_uppercase();
    value
}

fn static_attribute(output: &mut String, name: &str, value: &str) -> bool {
    output.push(' ');
    output.push_str(name);
    output.push_str("=\"");
    template::escape_into(output, value, true);
    output.push('"');
    name == "contenteditable" && value != "false"
}

#[cfg(test)]
mod tests {
    use super::static_attribute;

    #[test]
    fn static_attributes_escape_values_and_preserve_preview_editability() {
        let mut output = String::new();
        assert!(!static_attribute(&mut output, "title", "\"<&>'日本語😀"));
        assert!(!static_attribute(&mut output, "contenteditable", "false"));
        assert_eq!(
            output,
            " title=\"&quot;&lt;&amp;&gt;&#39;日本語😀\" contenteditable=\"false\""
        );
        for value in ["", "true", "plaintext-only", "FALSE"] {
            assert!(static_attribute(
                &mut String::new(),
                "contenteditable",
                value
            ));
        }
    }
}

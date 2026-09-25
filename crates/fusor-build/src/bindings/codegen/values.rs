use crate::bindings::{
    ir::{InterpolatedString, StringPart},
    tokens::Rust,
};
use fusor::template::{ElementId, MountId, TextId};
use proc_macro2::{Ident, TokenStream};
use quote::{format_ident, quote};

pub(super) fn element(id: ElementId) -> Ident {
    format_ident!("__fusor_element_{}", id.index())
}

pub(super) fn point(id: MountId) -> Ident {
    format_ident!("__fusor_mount_{}", id.index())
}

pub(super) fn text(id: TextId) -> Ident {
    format_ident!("__fusor_text_{}", id.index())
}

// The typed path changes the generated closure's result representation. Keep
// the original String result when authored code can return from that closure;
// opaque macros and attributes may introduce such a return after expansion.
pub(super) fn typed_text_eligible(value: &Rust) -> bool {
    fn transparent(tokens: TokenStream) -> bool {
        tokens.into_iter().all(|token| match token {
            proc_macro2::TokenTree::Group(group) => transparent(group.stream()),
            proc_macro2::TokenTree::Ident(ident) => ident != "return",
            proc_macro2::TokenTree::Punct(punct) => !matches!(punct.as_char(), '!' | '#'),
            proc_macro2::TokenTree::Literal(_) => true,
        })
    }
    transparent(value.tokens.clone())
}

pub(in crate::bindings) fn string(value: &InterpolatedString) -> TokenStream {
    let mut format = String::new();
    let mut expressions = Vec::new();
    for part in &value.0 {
        match part {
            StringPart::Literal(text) => {
                format.push_str(&text.replace('{', "{{").replace('}', "}}"))
            }
            StringPart::Expression(expression) => {
                format.push_str("{}");
                expressions.push(expression);
            }
        }
    }
    if format == "{}" && expressions.len() == 1 {
        let expression = expressions[0];
        quote! { ::std::string::ToString::to_string(&(#expression)) }
    } else {
        quote! { ::std::format!(#format #(, (#expressions))*) }
    }
}

pub(super) fn clone_locals(locals: &[Rust]) -> TokenStream {
    quote! { #(let #locals = ::std::clone::Clone::clone(&#locals);)* }
}

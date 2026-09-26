//! Token helpers shared by the browser and server lowerings: generated names,
//! lint allowances and the statements both targets write the same way.
use super::tokens::Rust;
use fusor::template::{ElementId, MountId, TextId};
use proc_macro2::{Ident, Literal, Span, TokenStream};
use quote::{format_ident, quote, quote_spanned};

/// A generated local, `__fusor_<kind>_<index>`.
pub(super) fn indexed(kind: &str, index: usize) -> Ident {
    format_ident!("__fusor_{}_{}", kind, index)
}

/// A generated constant, `__FUSOR_<KIND>_<index>`.
pub(super) fn indexed_constant(kind: &str, index: usize) -> Ident {
    format_ident!("__FUSOR_{}_{}", kind, index)
}

pub(super) fn element(id: ElementId) -> Ident {
    indexed("element", id.index())
}

pub(super) fn point(id: MountId) -> Ident {
    indexed("mount", id.index())
}

pub(super) fn text(id: TextId) -> Ident {
    indexed("text", id.index())
}

/// Give a closure its own copy of each lexical local it reads.
pub(super) fn clone_locals(locals: &[Rust]) -> TokenStream {
    quote! { #(let #locals = ::std::clone::Clone::clone(&#locals);)* }
}

/// Lints that generated code trips by wrapping authored expressions in blocks
/// and cloning captures it may not use; `extra` lists target-specific ones.
pub(super) fn allow_generated(span: Span, extra: TokenStream) -> TokenStream {
    let extra = extra.into_iter().map(|mut token| {
        token.set_span(span);
        token
    });
    quote_spanned! {span=>
        #[allow(unused_variables, unused_braces, unused_parens, clippy::let_and_return, clippy::needless_borrows_for_generic_args, clippy::clone_on_copy #(#extra)*)]
    }
}

/// Construct a component from its typed inputs inside a `|owner|` closure.
pub(super) fn from_inputs(
    span: Span,
    ty: &Rust,
    fields: impl IntoIterator<Item = TokenStream>,
) -> TokenStream {
    let fields = fields.into_iter();
    quote_spanned! {span=>
        type __FusorInputs = <#ty as ::fusor::dom::FromInputs>::Inputs;
        <#ty as ::fusor::dom::FromInputs>::from_inputs(__FusorInputs { #(#fields),* }, owner)
    }
}

/// An authored optional expression, or the value used when it is absent.
pub(super) fn or(value: Option<&Rust>, absent: TokenStream) -> TokenStream {
    value.map_or(absent, |value| quote! { #value })
}

pub(super) fn option(value: Option<TokenStream>) -> TokenStream {
    match value {
        Some(value) => quote! { ::std::option::Option::Some(#value) },
        None => quote! { ::std::option::Option::None },
    }
}

/// Refuse a binding a coherent frame cannot render.
pub(super) fn reject(span: Span, message: &str) -> TokenStream {
    let message = literal(span, message);
    quote_spanned! {span=> __fusor_frame.reject(#message)?; }
}

/// Fail the mount of a binding the browser cannot install.
pub(super) fn fail(span: Span, message: &str) -> TokenStream {
    let message = literal(span, message);
    quote_spanned! {span=> return ::std::result::Result::Err(::fusor::dom::JsValue::from_str(#message)); }
}

fn literal(span: Span, value: &str) -> Literal {
    let mut literal = Literal::string(value);
    literal.set_span(span);
    literal
}

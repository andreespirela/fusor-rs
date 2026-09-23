//! Structural control flow. Rust patterns are parsed by syn and checked by rustc.
use super::{ir::*, tokens::Rust};
use crate::{ExtractError, error};
use html5gum::StartTag;
use proc_macro2::TokenStream;
use quote::{ToTokens, quote};
use syn::parse::Parser;

pub(super) fn expression(
    source: &str,
    tag: &StartTag<usize>,
    attr: &[u8],
) -> Result<Rust, ExtractError> {
    if tag.attributes.len() != 1 {
        return Err(error(
            source,
            tag.span.start,
            "If accepts only condition; Match accepts only value",
        ));
    }
    let value = tag.attributes.get(attr).ok_or_else(|| {
        error(
            source,
            tag.span.start,
            "If requires condition=\"{{ boolean }}\"; Match requires value=\"{{ expression }}\"",
        )
    })?;
    let text = String::from_utf8_lossy(value);
    let parts = super::interpolation::interpolations(source, &text, value.span.start, false)?;
    super::interpolation::exact_expression(
        source,
        &text,
        parts,
        value.span.start,
        "control-flow inputs require exactly one {{ Rust expression }}",
    )
}

pub(super) fn pattern(
    source: &str,
    tag: &StartTag<usize>,
) -> Result<(Rust, Vec<Rust>), ExtractError> {
    if tag.attributes.len() != 1 || !tag.attributes.contains_key(b"pattern".as_slice()) {
        return Err(error(
            source,
            tag.span.start,
            "Case requires only pattern=\"Rust pattern\"",
        ));
    }
    let value = &tag.attributes[b"pattern".as_slice()];
    let pattern = syn::Pat::parse_multi_with_leading_vert
        .parse_str(&String::from_utf8_lossy(value))
        .map_err(|e| {
            error(
                source,
                value.span.start,
                format!("invalid Rust pattern: {e}"),
            )
        })?;
    // Visit binding positions only. Paths and struct member names are not locals.
    fn names(pat: &syn::Pat, result: &mut Vec<syn::Ident>) -> Result<(), &'static str> {
        match pat {
            syn::Pat::Ident(p) => {
                if p.by_ref.is_some() || p.mutability.is_some() {
                    return Err(
                        "Case captures are owned read-only reactive values; use owned patterns without ref or mut",
                    );
                }
                // syn cannot resolve a bare identifier as a unit variant,
                // constant, or binding. Reserve snake_case for captures, as in
                // ordinary Rust style; leave capitalized names (including None)
                // to rustc's pattern/name resolution without inventing a local.
                if !p.ident.to_string().chars().any(char::is_uppercase)
                    && !result.contains(&p.ident)
                {
                    result.push(p.ident.clone());
                }
                if let Some((_, sub)) = &p.subpat {
                    names(sub, result)?;
                }
            }
            syn::Pat::Or(p) => {
                for case in &p.cases {
                    names(case, result)?;
                }
            }
            syn::Pat::Paren(p) => names(&p.pat, result)?,
            syn::Pat::Slice(p) => {
                for item in &p.elems {
                    names(item, result)?;
                }
            }
            syn::Pat::Struct(p) => {
                for field in &p.fields {
                    names(&field.pat, result)?;
                }
            }
            syn::Pat::Tuple(p) => {
                for item in &p.elems {
                    names(item, result)?;
                }
            }
            syn::Pat::TupleStruct(p) => {
                for item in &p.elems {
                    names(item, result)?;
                }
            }
            syn::Pat::Reference(_) | syn::Pat::Macro(_) | syn::Pat::Verbatim(_) => {
                return Err(
                    "Case requires an owned Rust pattern without references or pattern macros",
                );
            }
            _ => {}
        }
        Ok(())
    }
    let mut bindings = Vec::new();
    names(&pattern, &mut bindings).map_err(|e| error(source, value.span.start, e))?;
    let bindings = bindings
        .into_iter()
        .map(|name| {
            let text = name.to_string();
            let text = text.trim_start_matches("r#");
            if super::tags::reserved_scope_name(text) {
                return Err(error(
                    source,
                    value.span.start,
                    "Case bindings cannot shadow framework scope names",
                ));
            }
            Rust::new(source, name.into_token_stream(), value.span.start)
        })
        .collect::<Result<_, _>>()?;
    Ok((
        Rust::new(source, pattern.into_token_stream(), value.span.start)?,
        bindings,
    ))
}

pub(super) fn body(
    source: &str,
    components: &mut Vec<Component>,
    owner: usize,
    first: usize,
    offset: usize,
    locals: Vec<Rust>,
    aliases: Vec<Rust>,
) -> Result<usize, ExtractError> {
    let index = components.len();
    let id = fusor::template::ComponentId::new(first + index);
    components.push(Component {
        locals: components[owner].locals.clone(),
        async_locals: locals,
        route_locals: aliases,
        snapshot_locals: components[owner].snapshot_locals.clone(),
        ..Component::new(
            id,
            Rust::parse(source, &format!("__FusorBranch{}", id.index()), offset)?,
            ComponentShape::Fragment(components[owner].ty.clone()),
            components[owner].render,
            offset..offset,
        )
    });
    Ok(index)
}

// Nested pairs have no tuple-arity trait limit. Only the selected case owns data.
fn pair(values: impl DoubleEndedIterator<Item = TokenStream>) -> TokenStream {
    values
        .rev()
        .fold(quote! { () }, |tail, head| quote! { (#head, #tail) })
}
fn field(index: usize) -> TokenStream {
    let tails = (0..index).map(|_| quote! { .1 });
    quote! { #(#tails)* .0 }
}

pub(super) fn selection(
    value: &Rust,
    cases: &[CaseBranch],
    snapshots: &[(Rust, Rust)],
) -> TokenStream {
    let environment = pair(
        snapshots
            .iter()
            .map(|(_, value)| quote! { ::fusor_components::Captured::new(#value) }),
    );
    let arms = cases.iter().enumerate().map(|(index, case)| {
        let pattern = &case.pattern;
        let payload = pair(case.names.iter().map(|name| quote! { #name }));
        let data = pair(cases.iter().enumerate().map(|(other, _)| {
            if other == index {
                quote! { ::std::option::Option::Some(#payload) }
            } else {
                quote! { ::std::option::Option::None }
            }
        }));
        quote! { #pattern => (#index, (#data, #environment)) }
    });
    quote! {{ let __rf_value = { #value }; #[deny(non_snake_case)] match __rf_value { #(#arms),* } }}
}

/// Each lexical capture is a normal Memo, identical to ForEach's public contract.
pub(super) fn projections(
    case: &CaseBranch,
    index: usize,
    snapshots: &[(Rust, Rust)],
) -> TokenStream {
    let variant = field(index);
    let fields = case.names.iter().enumerate().map(|(i, name)| {
        let field = field(i);
        quote! {
            let #name = {
                let __rf_data = __rf_data.clone();
                ::fusor::memo(move || __rf_data.with(|__rf_data| {
                    __rf_data.0 #variant .as_ref().expect("active branch data") #field .clone()
                }))
            };
        }
    });
    let environment = snapshots.iter().enumerate().map(|(index, (name, _))| {
        let field = field(index);
        quote! { let #name = { let __rf_data = __rf_data.clone(); ::fusor::derived(move || __rf_data.with(|__rf_data| __rf_data.1 #field .get())) }; }
    });
    quote! { #(#fields)* #(#environment)* }
}

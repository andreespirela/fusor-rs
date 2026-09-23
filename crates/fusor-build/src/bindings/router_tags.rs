//! Validation for structural route declarations. Uses the runtime matcher grammar.
use super::{ir::RouteBranch, tokens::Rust};
use crate::{ExtractError, error};
use fusor_router::pattern::Pattern;
use html5gum::StartTag;

pub(super) struct Declaration {
    pub path: Option<String>,
    pub alias: Option<Rust>,
    pub names: Vec<Rust>,
}

pub(super) fn route(source: &str, tag: &StartTag<usize>) -> Result<Declaration, ExtractError> {
    let offset = tag.span.start;
    if tag
        .attributes
        .keys()
        .any(|name| !matches!(name.as_ref(), b"path" | b"let" | b"fallback"))
    {
        return Err(error(
            source,
            offset,
            "Route accepts path and optional let, or fallback",
        ));
    }
    let fallback = tag.attributes.contains_key(b"fallback".as_slice());
    if fallback {
        if tag.attributes.len() != 1
            || !tag
                .attributes
                .get(b"fallback".as_slice())
                .unwrap()
                .is_empty()
        {
            return Err(error(
                source,
                offset,
                "write <Route fallback> without path or let",
            ));
        }
        return Ok(Declaration {
            path: None,
            alias: None,
            names: Vec::new(),
        });
    }
    let path = tag
        .attributes
        .get(b"path".as_slice())
        .ok_or_else(|| error(source, offset, "Route requires a path or fallback"))?;
    let path = String::from_utf8_lossy(path).into_owned();
    let pattern = Pattern::new(&path).map_err(|e| error(source, offset, e.to_string()))?;
    let alias = tag
        .attributes
        .get(b"let".as_slice())
        .map(|v| {
            let value = String::from_utf8_lossy(v);
            if super::tags::reserved_scope_name(&value) {
                return Err(error(
                    source,
                    offset,
                    "Route let cannot shadow framework scope names",
                ));
            }
            super::tags::field(source, &value, offset)
        })
        .transpose()?;
    let names = pattern
        .names()
        .map(|name| super::tags::field(source, name, offset))
        .collect::<Result<_, _>>()?;
    Ok(Declaration {
        path: Some(path),
        alias,
        names,
    })
}
pub(super) fn validate(
    source: &str,
    offset: usize,
    routes: &[RouteBranch],
    path: Option<&str>,
) -> Result<(), ExtractError> {
    for route in routes {
        let conflict = match (path, route.path.as_deref()) {
            (Some(a), Some(b)) => Pattern::new(a)
                .unwrap()
                .conflicts(&Pattern::new(b).unwrap()),
            (None, None) => true,
            _ => false,
        };
        if conflict {
            return Err(error(
                source,
                offset,
                "Router contains ambiguous route patterns or multiple fallbacks",
            ));
        }
    }
    Ok(())
}

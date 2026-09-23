//! Lower HTML bindings to a typed plan, then emit ordinary Rust.
mod application;
mod async_tags;
mod codegen;
mod control;
mod foreach;
mod hydration;
mod interpolation;
mod ir;
mod lexical;
mod parse;
mod router_tags;
mod server;
mod tags;
mod tokens;

use crate::{BindingLocation, ExtractError, RustBlock};
pub(crate) use ir::Edit;

pub(crate) struct Compiled {
    pub edits: Vec<Edit>,
    pub templates: String,
    pub locations: Vec<BindingLocation>,
    pub component_count: usize,
    pub app_offset: Option<usize>,
    pub fingerprint: String,
    pub javascript: Vec<crate::JavaScriptModule>,
}

pub(crate) fn compile(
    source: &str,
    blocks: &[RustBlock],
    rust: &mut String,
    first_component: usize,
) -> Result<Compiled, ExtractError> {
    let plan = parse::parse(source, blocks, first_component)?;
    let app = plan
        .components
        .iter()
        .find_map(|component| component.app.as_ref().map(|app| app.offset));
    let mut fingerprint = rust.clone();
    // Static delivery templates belong to the immutable unit protocol. Ordinary
    // app refresh compares executable native bindings, generated from the same
    // IR without delivery literals, rather than parsing those literals back out.
    codegen::generate(source, &plan.components, &mut fingerprint, false);
    let locations = codegen::generate(source, &plan.components, rust, true);
    Ok(Compiled {
        javascript: plan
            .components
            .iter()
            .filter_map(|component| component.javascript.clone())
            .collect(),
        edits: plan.edits,
        templates: plan.templates,
        locations,
        component_count: plan.components.len(),
        app_offset: app,
        fingerprint,
    })
}

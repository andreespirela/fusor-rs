use crate::bindings::ir::{Binding, Component, InputValue, RenderTarget};
use crate::{ExtractError, error};
use std::collections::BTreeMap;

// Validation consumes authored bindings and root counts; it does not rewrite IR.
pub(super) fn components(
    source: &str,
    components: &[Component],
    template_roots: &BTreeMap<usize, usize>,
) -> Result<(), ExtractError> {
    if let Some(app) = components
        .iter()
        .filter_map(|component| component.app())
        .nth(1)
    {
        return Err(error(
            source,
            app.offset,
            "declare exactly one App boundary per application",
        ));
    }
    for (&index, &roots) in template_roots {
        if roots != 1 {
            return Err(error(
                source,
                components[index].ty.offset,
                "a component template requires exactly one root element (Rust script blocks do not count)",
            ));
        }
    }
    for component in components {
        for binding in &component.bindings {
            if component.render == RenderTarget::Shared
                && matches!(binding, Binding::Value { .. } | Binding::Checked { .. })
            {
                return Err(error(
                    source,
                    binding.origin().offset,
                    "shared templates require bind:value, bind:checked or bind:field for editable controls so activation can adopt native edits",
                ));
            }
            if component.render == RenderTarget::Browser
                && matches!(binding, Binding::Island { .. })
            {
                return Err(error(
                    source,
                    binding.origin().offset,
                    super::HYDRATE_SERVER,
                ));
            }
            if component.render != RenderTarget::Browser
                && matches!(
                    binding,
                    Binding::Region { .. }
                        | Binding::Property { .. }
                        | Binding::Slot { .. }
                        | Binding::Router { .. }
                )
            {
                return Err(error(
                    source,
                    binding.origin().offset,
                    "server templates need resolved data; move async regions, widgets, opaque content and outlets into a browser preview",
                ));
            }
        }
    }
    fn count_children(bindings: &[Binding], components: &[Component]) -> usize {
        bindings
            .iter()
            .map(|binding| match binding {
                Binding::Children { .. } => 1,
                Binding::Branch { cases, .. } => cases
                    .iter()
                    .map(|case| count_children(&components[case.body].bindings, components))
                    .max()
                    .unwrap_or(0),
                // Sibling routes are exclusive placements of the caller's children.
                Binding::Router { routes, .. } => routes
                    .iter()
                    .map(|route| count_children(&components[route.body].bindings, components))
                    .max()
                    .unwrap_or(0),
                Binding::Region { bindings, .. } => count_children(bindings, components),
                Binding::Invocation {
                    children, inputs, ..
                } => {
                    children.map_or(0, |index| {
                        count_children(&components[index].bindings, components)
                    }) + inputs
                        .iter()
                        .map(|input| match input.value {
                            InputValue::Content { component, .. } => {
                                count_children(&components[component].bindings, components)
                            }
                            _ => 0,
                        })
                        .sum::<usize>()
                }
                _ => 0,
            })
            .sum()
    }
    for component in components.iter().filter(|component| !component.fragment()) {
        let placements = count_children(&component.bindings, components);
        if component.app().is_some() && placements > 0 {
            return Err(error(
                source,
                component.ty.offset,
                "App has no incoming Children; use Children in a reusable component",
            ));
        }
        if placements > 1 {
            return Err(error(
                source,
                component.ty.offset,
                "a component can place Children only once, including forwarded children",
            ));
        }
    }
    Ok(())
}

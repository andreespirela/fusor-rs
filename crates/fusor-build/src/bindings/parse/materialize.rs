use crate::RustBlock;
use crate::bindings::ir::{Component, Edit};
use crate::bindings::markup;
use std::ops::Range;

fn encloses(outer: &Range<usize>, inner: &Range<usize>) -> bool {
    outer.start <= inner.start && inner.end <= outer.end
}

// Materialize each component from source-relative edits, excluding nested captures.
pub(super) fn components(
    source: &str,
    blocks: &[RustBlock],
    edits: &[Edit],
    content_ranges: &[Range<usize>],
    components: &mut [Component],
) {
    for component in components {
        let mut replacements: Vec<_> = edits
            .iter()
            .filter(|edit| {
                encloses(&component.range, &edit.range)
                    && !content_ranges.iter().any(|range| {
                        range != &component.range
                            && encloses(&component.range, range)
                            && encloses(range, &edit.range)
                    })
            })
            .map(|edit| (edit.range.clone(), edit.replacement.clone()))
            .collect();
        replacements.extend(
            content_ranges
                .iter()
                .filter(|range| *range != &component.range && encloses(&component.range, range))
                .filter(|range| {
                    !content_ranges.iter().any(|outer| {
                        outer != &component.range
                            && outer != *range
                            && component.range.start <= outer.start
                            && encloses(outer, range)
                    })
                })
                .map(|range| (range.clone(), String::new())),
        );
        replacements.extend(
            blocks
                .iter()
                .filter(|block| encloses(&component.range, &block.element))
                .map(|block| (block.element.clone(), String::new())),
        );
        // Stable ordering preserves equal-offset insertions. Applying edits in
        // reverse keeps every remaining range relative to the authored source.
        replacements.sort_by_key(|(range, _)| range.start);
        let mut html = source[component.range.clone()].to_owned();
        for (range, value) in replacements.into_iter().rev() {
            html.replace_range(
                range.start - component.range.start..range.end - component.range.start,
                &value,
            );
        }
        component.empty = html.trim().is_empty();
        component.html = if component.fragment() {
            format!("{}{html}</template>", markup::template_open(component.id))
        } else {
            html
        };
    }
}

// Captured bodies now live in templates; remove their outermost ranges from the page.
pub(super) fn remove_captured_edits(edits: &mut Vec<Edit>, content_ranges: &[Range<usize>]) {
    edits.retain(|edit| {
        !content_ranges
            .iter()
            .any(|range| encloses(range, &edit.range))
    });
    for range in content_ranges {
        if !content_ranges
            .iter()
            .any(|outer| outer != range && encloses(outer, range))
        {
            edits.push(Edit {
                range: range.clone(),
                replacement: String::new(),
            });
        }
    }
}

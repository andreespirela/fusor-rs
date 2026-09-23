use fusor_build::{BindingLocation, SourceMap, extract};
use proc_macro2::{TokenStream, TokenTree};

fn find_identifier(tokens: TokenStream, name: &str) -> Option<usize> {
    for token in tokens {
        match token {
            TokenTree::Ident(ident) if ident == name => return Some(ident.span().start().line),
            TokenTree::Group(group) => {
                if let Some(line) = find_identifier(group.stream(), name) {
                    return Some(line);
                }
            }
            _ => {}
        }
    }
    None
}

#[test]
fn foreach_items_key_and_component_input_keep_html_origins() {
    let html = r#"<script type="text/rust">struct List;</script>
<ul rust:component="List"><ForEach
    items="{{ state.items.get() }}"
    key="{{ |item| item.identity }}">
    <Row item="{{ item.clone() }}"></Row>
</ForEach></ul>"#;
    let page = extract(html).unwrap();
    let map = SourceMap::new(page.locations).unwrap();
    for (expression, expected_line) in [("items", 3), ("identity", 4), ("Row", 5)] {
        let generated_line = find_identifier(page.rust.parse().unwrap(), expression).unwrap();
        let origin = map.lookup(generated_line).unwrap();
        assert_eq!(origin.line, expected_line, "{expression}");
        assert!(origin.column >= 5);
    }
    // Type errors in generated call scaffolding still identify the owning binding.
    let setup = find_identifier(page.rust.parse().unwrap(), "__rf_key_state").unwrap();
    assert_eq!(map.lookup(setup).unwrap().line, 3);
    let serialized = map.to_string();
    assert_eq!(serialized.parse::<SourceMap>().unwrap(), map);
}

#[test]
fn lookup_respects_range_boundaries_and_unmapped_lines() {
    let map = SourceMap::new(vec![
        BindingLocation {
            generated_start: 10,
            generated_end: 12,
            line: 3,
            column: 7,
        },
        BindingLocation {
            generated_start: 15,
            generated_end: 16,
            line: 5,
            column: 2,
        },
    ])
    .unwrap();
    for line in [0, 1, 9, 12, 14, 16, usize::MAX] {
        assert!(map.lookup(line).is_none());
    }
    for line in [10, 11] {
        assert_eq!(map.lookup(line).unwrap().line, 3);
    }
    assert_eq!(map.lookup(15).unwrap().line, 5);
    assert_eq!(
        SourceMap::default()
            .to_string()
            .parse::<SourceMap>()
            .unwrap(),
        SourceMap::default()
    );
}

#[test]
fn rejects_corrupt_unsupported_and_ambiguous_source_maps() {
    for input in [
        "",
        "fusor-source-map-v2\n",
        "10\t12\t3\t7\n",
        "fusor-source-map-v1\n10\t12\t3\n",
        "fusor-source-map-v1\n10\t12\tno\t7\n",
        "fusor-source-map-v1\n0\t12\t3\t7\n",
        "fusor-source-map-v1\n10\t10\t3\t7\n",
        "fusor-source-map-v1\n10\t12\t0\t7\n",
        "fusor-source-map-v1\n10\t12\t3\t0\n",
        "fusor-source-map-v1\n10\t12\t3\t7\n11\t13\t5\t2\n",
        "fusor-source-map-v1\n10\t12\t3\t7\n1\t3\t5\t2\n",
        "fusor-source-map-v1\n\n",
    ] {
        assert!(input.parse::<SourceMap>().is_err(), "accepted {input:?}");
    }
}

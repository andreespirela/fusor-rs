use super::{
    ElementHandle, Handles, Mounts, Resolution, Slot, TextPosition, element_text, invalid,
    text_slot,
};
use crate::dom::{MountPoint, document, is_html, strings};
use crate::template::{
    self, ChildPolicy, ElementId, MountId, MountMarker, TemplateDescriptor, TextId, TextMarker,
};
use std::{collections::BTreeMap, fmt::Display};
use wasm_bindgen::{JsCast, JsValue};
use web_sys::{Element, Node};

pub(super) fn resolve(
    descriptor: &TemplateDescriptor,
    expected_mounts: &'static [MountId],
    root: &Element,
    fragment: Option<&MountPoint>,
    hydrating: bool,
) -> Result<Resolution, JsValue> {
    let document = document()?;

    let mut elements = BTreeMap::new();
    let mut starts = BTreeMap::new();
    let mut ends = BTreeMap::new();
    let mut text_elements = BTreeMap::new();
    let mut mount_starts = BTreeMap::new();
    let mut mount_ends = BTreeMap::new();
    let managed: std::collections::BTreeSet<_> = descriptor
        .elements
        .iter()
        .filter(|element| element.children == ChildPolicy::Managed)
        .map(|element| element.id)
        .collect();
    // SHOW_ALL is the DOM API's default. Include the root, then visit its
    // descendants once; inert template contents are separate DOM trees.
    let traversal_root: Node = fragment
        .and_then(|point| point.start.parent_node())
        .unwrap_or_else(|| root.clone().into());
    let walker = document.create_tree_walker(&traversal_root)?;
    let mut current: Option<Node> = if let Some(point) = fragment {
        walker.set_current_node(&point.start);
        walker.next_node()?
    } else {
        Some(root.clone().into())
    };
    let mut skip_mount = None;
    while let Some(node) = current {
        if fragment.is_some_and(|point| node.is_same_node(Some(&point.end))) {
            break;
        }
        // A server-rendered child resolves its own descriptor. Skip whole
        // sibling subtrees, including any nested component anchor pairs.
        if let Some(id) = skip_mount {
            if node.node_type() == Node::COMMENT_NODE
                && MountMarker::parse(&node.node_value().unwrap_or_default()).map_err(invalid)?
                    == Some(MountMarker::End(id))
            {
                skip_mount = None;
            } else {
                current = walker.next_sibling()?;
                continue;
            }
        }
        let mut skip_children = false;
        match node.node_type() {
            Node::ELEMENT_NODE => {
                let element: Element = node.unchecked_into();
                if let Some(value) = element.get_attribute(template::TEXT_ELEMENT_ATTRIBUTE) {
                    let id: TextId = value.parse().map_err(invalid)?;
                    insert(&mut text_elements, id, element.clone(), "text element")?;
                }
                if let Some(value) = strings::attribute(&element, strings::Name::Element) {
                    let id: ElementId = value.parse().map_err(invalid)?;
                    skip_children = managed.contains(&id);
                    insert(&mut elements, id, element, "element")?;
                }
            }
            Node::COMMENT_NODE => {
                let value = node.node_value().unwrap_or_default();
                if let Some(marker) = MountMarker::parse(&value).map_err(invalid)? {
                    let (id, anchors) = match marker {
                        MountMarker::Start(id) => {
                            if hydrating {
                                skip_mount = Some(id);
                            }
                            (id, &mut mount_starts)
                        }
                        MountMarker::End(id) => (id, &mut mount_ends),
                    };
                    insert(anchors, id, node, "component anchor")?;
                } else if let Some(marker) = TextMarker::parse(&value).map_err(invalid)? {
                    let (id, anchors, kind) = match marker {
                        TextMarker::Start(id) => (id, &mut starts, "text start"),
                        TextMarker::End(id) => (id, &mut ends, "text end"),
                    };
                    insert(anchors, id, node, kind)?;
                }
            }
            _ => {}
        }
        current = if skip_children {
            loop {
                if let Some(sibling) = walker.next_sibling()? {
                    break Some(sibling);
                }
                if walker.parent_node()?.is_none() {
                    break None;
                }
            }
        } else {
            walker.next_node()?
        };
    }

    let mut handles = Handles::new();
    for expected in descriptor.elements {
        let element = take(&mut elements, expected.id, "element")?;
        if element.local_name() != expected.tag || !is_html(&element) {
            return Err(invalid(format_args!(
                "element {} must be <{}>, found <{}>",
                expected.id,
                expected.tag,
                element.local_name()
            )));
        }
        let handle = if expected.tag == "input" {
            ElementHandle::Input(
                element
                    .dyn_into()
                    .map_err(|_| invalid("expected an HTML input"))?,
            )
        } else {
            ElementHandle::Element(element)
        };
        handles.insert(expected.id, handle);
    }
    if !elements.is_empty() {
        return Err(invalid("unexpected element identifiers"));
    }
    handles.finish();

    // Validate every pair before adding text nodes or subscribing effects.
    let mut slots = Vec::new();
    for id in descriptor.texts {
        let start = take(&mut starts, *id, "text start")?;
        let end = take(&mut ends, *id, "text end")?;
        let text = text_slot(*id, &start, &end)?;
        slots.push(Slot {
            id: *id,
            position: TextPosition::Anchored { start, end },
            existing: text,
        });
    }
    if !starts.is_empty() || !ends.is_empty() {
        return Err(invalid("unexpected text identifiers"));
    }
    for expected in descriptor.text_elements {
        let id = expected.id;
        let element = take(&mut text_elements, id, "or mismatched text element")?;
        if element.local_name() != expected.tag || !is_html(&element) {
            return Err(invalid(format_args!(
                "text element {id} must be <{}>",
                expected.tag
            )));
        }
        if let Some(host) = expected.host {
            if !matches!(handles.get(&host), Some(ElementHandle::Element(bound)) if element.is_same_node(Some(bound)))
            {
                return Err(invalid(format_args!("mismatched text host {host}")));
            }
        }
        let existing = element_text(id, &element)?;
        slots.push(Slot {
            id,
            position: TextPosition::Element(element),
            existing,
        });
    }
    if !text_elements.is_empty() {
        return Err(invalid("unexpected text elements"));
    }
    let mut mounts = Mounts::new();
    for id in expected_mounts {
        let point = MountPoint {
            start: take(&mut mount_starts, *id, "component start")?,
            end: take(&mut mount_ends, *id, "component end")?,
        };
        if hydrating {
            point.validate()?;
        } else if !point
            .start
            .next_sibling()
            .is_some_and(|next| next.is_same_node(Some(&point.end)))
        {
            return Err(invalid(format_args!(
                "component mount {id} must initially be empty and paired"
            )));
        }
        mounts.insert(*id, point);
    }
    if !mount_starts.is_empty() || !mount_ends.is_empty() {
        return Err(invalid("unexpected component anchors"));
    }
    Ok((handles, slots, mounts))
}

fn insert<K: Ord + Display + Copy, V>(
    found: &mut BTreeMap<K, V>,
    id: K,
    value: V,
    what: &str,
) -> Result<(), JsValue> {
    if found.insert(id, value).is_some() {
        return Err(invalid(format_args!("duplicate {what} {id}")));
    }
    Ok(())
}

fn take<K: Ord + Display, V>(found: &mut BTreeMap<K, V>, id: K, what: &str) -> Result<V, JsValue> {
    found
        .remove(&id)
        .ok_or_else(|| invalid(format_args!("missing {what} {id}")))
}

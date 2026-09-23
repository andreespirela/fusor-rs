#[cfg(feature = "islands")]
use super::delivery;
use super::{ElementTarget, JsValue, Scope, document, reconcile, strings};
use crate::{Signal, signal, untrack};
use std::{cell::Cell, collections::BTreeMap};
use wasm_bindgen::JsCast;
use web_sys::{HtmlElement, HtmlInputElement};

type EncodeKey<K> = dyn Fn(&K) -> Result<String, JsValue>;

impl Scope {
    /// Reconcile a list by stable keys, retaining nodes, focus, and row scopes.
    /// `render` runs once per inserted key. The row signal delivers later values.
    /// This binding owns the container's children. Duplicate keys are errors.
    pub fn keyed<T, K>(
        &mut self,
        target: impl ElementTarget,
        items: impl Fn() -> Vec<T> + 'static,
        key: impl Fn(&T) -> K + 'static,
        render: impl Fn(Signal<T>) -> Result<Scope, JsValue> + 'static,
    ) -> Result<(), JsValue>
    where
        T: Clone + PartialEq + 'static,
        K: Ord + Clone + 'static,
    {
        self.keyed_inner(target, items, key, render, None)
    }

    /// Generated shared templates carry serialized row identities in native HTML.
    #[doc(hidden)]
    pub fn keyed_hydrated<T, K>(
        &mut self,
        target: impl ElementTarget,
        items: impl Fn() -> Vec<T> + 'static,
        key: impl Fn(&T) -> K + 'static,
        render: impl Fn(Signal<T>) -> Result<Scope, JsValue> + 'static,
        encode: impl Fn(&K) -> Result<String, JsValue> + 'static,
    ) -> Result<(), JsValue>
    where
        T: Clone + PartialEq + 'static,
        K: Ord + Clone + 'static,
    {
        self.keyed_inner(target, items, key, render, Some(Box::new(encode)))
    }

    fn keyed_inner<T, K>(
        &mut self,
        target: impl ElementTarget,
        items: impl Fn() -> Vec<T> + 'static,
        key: impl Fn(&T) -> K + 'static,
        render: impl Fn(Signal<T>) -> Result<Scope, JsValue> + 'static,
        encode: Option<Box<EncodeKey<K>>>,
    ) -> Result<(), JsValue>
    where
        T: Clone + PartialEq + 'static,
        K: Ord + Clone + 'static,
    {
        let hydrating = self.is_hydrating();
        let container = target.resolve(self)?;
        let mut rows: BTreeMap<K, (Signal<T>, Scope, Cell<usize>)> = BTreeMap::new();
        let mut initialized = false;
        self.bind(move || {
            let items = items();
            untrack(|| {
                let keys: Vec<K> = items.iter().map(&key).collect();
                let mut unique = reconcile::SortedKeys::new(&keys)
                    .ok_or_else(|| JsValue::from_str("fusor: duplicate key in list"))?;
                let mut native_rows = Vec::new();
                if hydrating && !initialized {
                    let encode = encode.as_ref().ok_or_else(|| {
                        JsValue::from_str("hydrated lists require generated key metadata")
                    })?;
                    let mut node = container.first_element_child();
                    for key in &keys {
                        let row = node
                            .take()
                            .ok_or_else(|| JsValue::from_str("missing native row"))?;
                        if strings::attribute(&row, strings::Attribute::Key).as_deref()
                            != Some(encode(key)?.as_str())
                        {
                            return Err(JsValue::from_str("native row key mismatch"));
                        }
                        node = row.next_element_sibling();
                        native_rows.push(row);
                    }
                    if node.is_some() {
                        return Err(JsValue::from_str("unexpected native row"));
                    }
                }
                let document = document()?;
                let focused = document
                    .active_element()
                    .filter(|node| container.contains(Some(node)))
                    .and_then(|node| node.dyn_into::<HtmlElement>().ok());
                let selection = focused
                    .as_ref()
                    .and_then(|node| node.dyn_ref::<HtmlInputElement>())
                    .and_then(|input| {
                        Some((
                            input.selection_start().ok()??,
                            input.selection_end().ok()??,
                            input.selection_direction().ok()??,
                        ))
                    });
                // Stage new scopes before touching the visible list. A failing
                // render drops all staged listeners and leaves old rows intact.
                let mut staged = BTreeMap::new();
                for (index, (key, item)) in keys.iter().zip(&items).enumerate() {
                    if !rows.contains_key(key) {
                        let state = signal(item.clone());
                        #[cfg(feature = "islands")]
                        let scope = if let Some(root) = native_rows.get(index) {
                            delivery::with_root(root, || render(state.clone()))?
                        } else {
                            render(state.clone())?
                        };
                        #[cfg(not(feature = "islands"))]
                        let scope = {
                            let _ = index;
                            render(state.clone())?
                        };
                        // Server rows already occupy their final positions. Newly
                        // rendered roots are detached and must be inserted.
                        let position = if native_rows.is_empty() {
                            usize::MAX
                        } else {
                            index
                        };
                        staged.insert(key.clone(), (state, scope, Cell::new(position)));
                    }
                }
                for (_, row, _) in staged.values() {
                    row.finish_prepare()?;
                }
                if !initialized {
                    if !hydrating {
                        #[cfg(feature = "islands")]
                        delivery::dispose_tree(&container);
                        container.set_text_content(None);
                    }
                    initialized = true;
                }
                rows.retain(|key, (_, row, _)| {
                    let keep = unique.contains_next(key);
                    if !keep {
                        #[cfg(feature = "islands")]
                        delivery::dispose_tree(&row.root);
                        row.root.remove();
                    }
                    keep
                });
                if rows.is_empty() {
                    rows = staged;
                } else if staged.len() <= rows.len() / (rows.len().ilog2() as usize + 1) {
                    // Avoid rebuilding the existing tree for sparse additions;
                    // keep the linear sorted merge when insertions are dense.
                    rows.extend(staged);
                } else {
                    rows.append(&mut staged);
                }
                let ordered: Vec<_> = keys.iter().map(|key| &rows[key]).collect();
                let positions: Vec<_> = ordered.iter().map(|row| row.2.get()).collect();
                let stationary = reconcile::stationary(&positions);
                for (index, ((state, _, position), item)) in ordered.iter().zip(items).enumerate() {
                    state.set(item);
                    position.set(index);
                }
                let mut anchor: Option<&web_sys::Node> = None;
                for ((_, row, _), keep) in ordered.iter().zip(stationary).rev() {
                    if !keep {
                        container.insert_before(&row.root, anchor)?;
                    }
                    anchor = Some(row.root.as_ref());
                }
                for (_, row, _) in rows.values() {
                    row.commit();
                }
                // insertBefore can blur a node even when moving it within the
                // same list. Restore focus only if that original node survives.
                if let Some(focused) = focused.filter(|node| container.contains(Some(node))) {
                    if !document
                        .active_element()
                        .is_some_and(|node| node.is_same_node(Some(&focused)))
                    {
                        focused.focus()?;
                        if let (Some(input), Some((start, end, direction))) =
                            (focused.dyn_ref::<HtmlInputElement>(), selection)
                        {
                            input.set_selection_range_with_direction(start, end, &direction)?;
                        }
                    }
                }
                Ok(())
            })
        })
    }
}

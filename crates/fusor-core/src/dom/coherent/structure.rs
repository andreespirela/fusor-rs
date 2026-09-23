use super::{Frame, SlotId, Structure, allowed, error, visit};
use crate::dom::{Children, Component, MountPoint, Scope};
use crate::{OwnerHandle, Signal, signal, versions::Versions};
use std::{cell::RefCell, collections::BTreeMap, rc::Rc};
use wasm_bindgen::JsValue;
use web_sys::Element;

impl Frame<'_> {
    pub fn component_at<C: Component, K: PartialEq + 'static>(
        &mut self,
        slot: usize,
        target: &MountPoint,
        key: Option<K>,
        make: impl FnOnce(OwnerHandle) -> Result<C, JsValue>,
        children: Children,
    ) -> Result<(), String> {
        target.validate().map_err(error)?;
        for anchor in [&target.start, &target.end] {
            self.publication
                .targets
                .push((self.tree.clone(), anchor.clone()));
        }
        let state = {
            let mut slots = self.tree.slots.borrow_mut();
            slots
                .entry(SlotId::Component(slot))
                .or_insert_with(|| Rc::new(ChildSlot::<Rc<K>>::default()))
                .clone()
        }
        .downcast::<ChildSlot<Rc<K>>>()
        .map_err(|_| "coherent child key type changed")?;
        let retired =
            if state.epoch.replace(Some(self.attempt.epoch())) != Some(self.attempt.epoch()) {
                state.candidate.take()
            } else {
                None
            };
        drop(retired);
        let next = match key.map(Rc::new) {
            None => None,
            Some(key) => {
                let existing = state
                    .current
                    .borrow()
                    .as_ref()
                    .filter(|(old, _)| old == &key)
                    .cloned()
                    .or_else(|| {
                        state
                            .candidate
                            .borrow()
                            .as_ref()
                            .filter(|(old, _)| old == &key)
                            .cloned()
                    });
                let instance = match existing {
                    Some((_, instance)) => instance,
                    None => {
                        let scope = children
                            .with(|| C::prepare(&self.tree.owner, make))
                            .map_err(error)?;
                        if scope.render_tree.is_none() {
                            return Err("a coherent child requires generated HTML bindings".into());
                        }
                        Rc::new(scope)
                    }
                };
                visit(
                    instance.render_tree.as_ref().expect("validated"),
                    self.attempt,
                    self.publication,
                )?;
                Some((key, instance))
            }
        };
        let old = state.candidate.replace(next.clone());
        drop(old);
        self.publication.structures.push(Box::new(ChildPlan {
            target: target.clone(),
            state,
            next,
        }));
        Ok(())
    }
}

struct ChildSlot<K> {
    epoch: std::cell::Cell<Option<u64>>,
    current: RefCell<Option<(K, Rc<Scope>)>>,
    candidate: RefCell<Option<(K, Rc<Scope>)>>,
}
impl<K> Default for ChildSlot<K> {
    fn default() -> Self {
        Self {
            epoch: std::cell::Cell::new(None),
            current: RefCell::new(None),
            candidate: RefCell::new(None),
        }
    }
}
struct ChildPlan<K> {
    target: MountPoint,
    state: Rc<ChildSlot<K>>,
    next: Option<(K, Rc<Scope>)>,
}
impl<K: Clone + 'static> Structure for ChildPlan<K> {
    fn validate(&self) -> Result<(), String> {
        self.target.validate().map_err(error)?;
        if let Some((_, scope)) = &self.next {
            if scope.owner().is_disposed() {
                return Err("prepared child was disposed".into());
            }
            // Widget setup is forbidden by generated coherent lowering.
            scope.finish_prepare().map_err(error)?;
        }
        Ok(())
    }
    fn apply(&self) -> Result<(), String> {
        let next = self.next.as_ref().map(|(_, scope)| scope.root());
        if let Some(next) = next {
            let parent = self
                .target
                .end
                .parent_node()
                .ok_or("detached component anchor")?;
            if !next
                .next_sibling()
                .is_some_and(|node| node.is_same_node(Some(&self.target.end)))
            {
                parent
                    .insert_before(next, Some(&self.target.end))
                    .map_err(error)?;
            }
        }
        if let Some((_, old)) = self.state.current.borrow().as_ref() {
            if next.is_none_or(|next| !next.is_same_node(Some(old.root()))) {
                old.root().remove();
            }
        }
        Ok(())
    }
    fn finish(self: Box<Self>) {
        let old = self.state.current.replace(self.next.clone());
        self.state.candidate.take();
        if let Some((_, scope)) = &self.next {
            scope.commit();
        }
        drop(old);
    }
}

impl Frame<'_> {
    pub fn children_at(
        &mut self,
        slot: usize,
        target: &MountPoint,
        children: &Children,
    ) -> Result<(), String> {
        target.validate().map_err(error)?;
        for anchor in [&target.start, &target.end] {
            self.publication
                .targets
                .push((self.tree.clone(), anchor.clone()));
        }
        let existing = self
            .tree
            .slots
            .borrow()
            .get(&SlotId::Children(slot))
            .cloned();
        let state = if let Some(existing) = existing {
            existing
                .downcast::<ChildrenState>()
                .map_err(|_| "coherent children slot type changed")?
        } else {
            let scope = children
                .prepare(&self.tree.owner)
                .map_err(error)?
                .map(Rc::new);
            let state = Rc::new(ChildrenState(scope));
            self.tree
                .slots
                .borrow_mut()
                .insert(SlotId::Children(slot), state.clone());
            state
        };
        if let Some(scope) = &state.0 {
            visit(
                scope
                    .render_tree
                    .as_ref()
                    .ok_or("children require generated coherent HTML")?,
                self.attempt,
                self.publication,
            )?;
            self.publication.structures.push(Box::new(ChildrenPlan {
                target: target.clone(),
                scope: scope.clone(),
            }));
        }
        Ok(())
    }
}

struct ChildrenState(Option<Rc<Scope>>);
struct ChildrenPlan {
    target: MountPoint,
    scope: Rc<Scope>,
}
impl Structure for ChildrenPlan {
    fn validate(&self) -> Result<(), String> {
        self.target.validate().map_err(error)?;
        self.scope.finish_prepare().map_err(error)
    }
    fn apply(&self) -> Result<(), String> {
        let fragment = self
            .scope
            .fragment
            .as_ref()
            .ok_or("missing children range")?;
        if !fragment
            .end
            .next_sibling()
            .is_some_and(|node| node.is_same_node(Some(&self.target.end)))
        {
            self.scope.attach_fragment(&self.target).map_err(error)?;
        }
        Ok(())
    }
    fn finish(self: Box<Self>) {
        self.scope.commit();
    }
}

impl Frame<'_> {
    /// Compiler-owned exclusive fragment with reactive, branch-local captures.
    pub fn branch_at<T: Clone + PartialEq + 'static>(
        &mut self,
        slot: usize,
        target: &MountPoint,
        read: impl FnOnce() -> (usize, T),
        prepare: impl FnOnce(usize, Signal<T>, &OwnerHandle) -> Result<Scope, JsValue>,
    ) -> Result<(), String> {
        target.validate().map_err(error)?;
        for anchor in [&target.start, &target.end] {
            self.publication
                .targets
                .push((self.tree.clone(), anchor.clone()));
        }
        let ((key, data), inputs) = Versions::capture(read);
        let state = {
            let mut slots = self.tree.slots.borrow_mut();
            slots
                .entry(SlotId::Branch(slot))
                .or_insert_with(|| Rc::new(BranchSlot::<T>::default()))
                .clone()
        }
        .downcast::<BranchSlot<T>>()
        .map_err(|_| "coherent branch capture type changed")?;
        if state.epoch.replace(Some(self.attempt.epoch())) != Some(self.attempt.epoch()) {
            let retired = state.candidate.take();
            drop(retired);
        }
        let existing = state
            .current
            .borrow()
            .as_ref()
            .filter(|(old, _, _)| *old == key)
            .cloned()
            .or_else(|| {
                state
                    .candidate
                    .borrow()
                    .as_ref()
                    .filter(|(old, _, _)| *old == key)
                    .cloned()
            });
        let (value, scope) = match existing {
            Some((_, value, scope)) => (value, scope),
            None => {
                let value = signal(data.clone());
                let scope = prepare(key, value.clone(), &self.tree.owner).map_err(error)?;
                if scope.render_tree.is_none() {
                    return Err("coherent branches require generated HTML".into());
                }
                (value, Rc::new(scope))
            }
        };
        let data = Rc::new(data);
        value.with_render_value(data.clone(), inputs, || {
            visit(
                scope.render_tree.as_ref().expect("validated"),
                self.attempt,
                self.publication,
            )
        })?;
        let next = (key, value, scope);
        let old = state.candidate.replace(Some(next.clone()));
        drop(old);
        self.publication.structures.push(Box::new(BranchPlan {
            target: target.clone(),
            state,
            next,
            data,
        }));
        Ok(())
    }
}

type BranchInstance<T> = (usize, Signal<T>, Rc<Scope>);
struct BranchSlot<T> {
    epoch: std::cell::Cell<Option<u64>>,
    current: RefCell<Option<BranchInstance<T>>>,
    candidate: RefCell<Option<BranchInstance<T>>>,
}
impl<T> Default for BranchSlot<T> {
    fn default() -> Self {
        Self {
            epoch: std::cell::Cell::new(None),
            current: RefCell::new(None),
            candidate: RefCell::new(None),
        }
    }
}
struct BranchPlan<T> {
    target: MountPoint,
    state: Rc<BranchSlot<T>>,
    next: BranchInstance<T>,
    data: Rc<T>,
}
impl<T: Clone + PartialEq + 'static> Structure for BranchPlan<T> {
    fn validate(&self) -> Result<(), String> {
        self.target.validate().map_err(error)?;
        if self.next.2.owner().is_disposed() {
            return Err("prepared branch was disposed".into());
        }
        self.next.2.finish_prepare().map_err(error)
    }
    fn apply(&self) -> Result<(), String> {
        let fragment = self
            .next
            .2
            .fragment
            .as_ref()
            .ok_or("missing branch range")?;
        if !fragment
            .end
            .next_sibling()
            .is_some_and(|node| node.is_same_node(Some(&self.target.end)))
        {
            self.next.2.attach_fragment(&self.target).map_err(error)?;
        }
        if let Some((_, _, old)) = self.state.current.borrow().as_ref() {
            if !Rc::ptr_eq(old, &self.next.2) {
                old.fragment
                    .as_ref()
                    .ok_or("missing previous branch range")?
                    .remove();
            }
        }
        Ok(())
    }
    fn finish(self: Box<Self>) {
        let old = self.state.current.replace(Some(self.next.clone()));
        self.state.candidate.take();
        self.next.1.set((*self.data).clone());
        drop(old);
        self.next.2.commit();
    }
}

impl Frame<'_> {
    pub fn keyed<T, K>(
        &mut self,
        slot: usize,
        container: &Element,
        items: impl FnOnce() -> Vec<T>,
        key: impl Fn(&T) -> K,
        render: impl Fn(Signal<T>, &OwnerHandle) -> Result<Scope, JsValue>,
    ) -> Result<(), String>
    where
        T: Clone + PartialEq + 'static,
        K: Ord + Clone + 'static,
    {
        allowed(container)?;
        let (items, inputs) = Versions::capture(items);
        let keys: Vec<_> = items.iter().map(key).collect();
        if keys.iter().collect::<std::collections::BTreeSet<_>>().len() != keys.len() {
            return Err("duplicate key in coherent list".into());
        }
        let state = {
            let mut slots = self.tree.slots.borrow_mut();
            slots
                .entry(SlotId::List(slot))
                .or_insert_with(|| Rc::new(ListSlot::<K, T>::default()))
                .clone()
        }
        .downcast::<ListSlot<K, T>>()
        .map_err(|_| "coherent list item/key type changed")?;
        let retired =
            if state.epoch.replace(Some(self.attempt.epoch())) != Some(self.attempt.epoch()) {
                state.candidate.take()
            } else {
                BTreeMap::new()
            };
        drop(retired);
        let mut next = BTreeMap::new();
        let mut updates = Vec::new();
        for (key, item) in keys.iter().zip(items) {
            let existing = state
                .current
                .borrow()
                .get(key)
                .cloned()
                .or_else(|| state.candidate.borrow().get(key).cloned());
            let (value, scope) = match existing {
                Some(existing) => existing,
                None => {
                    let value = signal(item.clone());
                    let scope = render(value.clone(), &self.tree.owner).map_err(error)?;
                    if scope.render_tree.is_none() {
                        return Err("coherent rows require generated HTML bindings".into());
                    }
                    (value, Rc::new(scope))
                }
            };
            let item = Rc::new(item);
            value.with_render_value(item.clone(), inputs.clone(), || {
                visit(
                    scope.render_tree.as_ref().expect("validated"),
                    self.attempt,
                    self.publication,
                )
            })?;
            updates.push((value.clone(), item));
            next.insert(key.clone(), (value, scope));
        }
        let old = state.candidate.replace(next.clone());
        drop(old);
        let remove = state
            .current
            .borrow()
            .iter()
            .filter(|(key, _)| !next.contains_key(*key))
            .map(|(_, (_, scope))| scope.root().clone())
            .collect();
        let order = keys.iter().map(|key| next[key].1.root().clone()).collect();
        self.publication.structures.push(Box::new(ListPlan {
            container: container.clone(),
            state,
            next,
            updates,
            remove,
            order,
        }));
        Ok(())
    }
}

type Rows<K, T> = BTreeMap<K, (Signal<T>, Rc<Scope>)>;
struct ListSlot<K, T> {
    epoch: std::cell::Cell<Option<u64>>,
    current: RefCell<Rows<K, T>>,
    candidate: RefCell<Rows<K, T>>,
}
impl<K, T> Default for ListSlot<K, T> {
    fn default() -> Self {
        Self {
            epoch: std::cell::Cell::new(None),
            current: RefCell::new(BTreeMap::new()),
            candidate: RefCell::new(BTreeMap::new()),
        }
    }
}
struct ListPlan<K, T> {
    container: Element,
    state: Rc<ListSlot<K, T>>,
    next: Rows<K, T>,
    updates: Vec<(Signal<T>, Rc<T>)>,
    remove: Vec<Element>,
    order: Vec<Element>,
}
impl<K: Ord + Clone + 'static, T: Clone + PartialEq + 'static> Structure for ListPlan<K, T> {
    fn validate(&self) -> Result<(), String> {
        for (_, scope) in self.next.values() {
            scope.finish_prepare().map_err(error)?;
        }
        Ok(())
    }
    fn apply(&self) -> Result<(), String> {
        for root in &self.remove {
            root.remove();
        }
        let mut cursor = self.container.first_child();
        for root in &self.order {
            if !cursor
                .as_ref()
                .is_some_and(|node| node.is_same_node(Some(root)))
            {
                self.container
                    .insert_before(root, cursor.as_ref())
                    .map_err(error)?;
            }
            cursor = root.next_sibling();
        }
        Ok(())
    }
    fn finish(self: Box<Self>) {
        let old = self.state.current.replace(self.next.clone());
        self.state.candidate.take();
        // Rust publications and lifecycle callbacks occur after every DOM patch.
        for (value, item) in self.updates {
            value.set((*item).clone());
        }
        for (_, scope) in self.next.values() {
            scope.commit();
        }
        drop(old);
    }
}

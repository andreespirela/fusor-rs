//! Validate the serialized template once and resolve native DOM handles.

use super::{JsValue, MountPoint, Scope, document, strings};
use crate::OwnerHandle;
use crate::template::{
    self, ChildPolicy, ElementId, MountId, RootKind, TemplateDescriptor, TextId,
};
use std::{collections::BTreeMap, rc::Rc};
use wasm_bindgen::JsCast;
use web_sys::{Element, HtmlInputElement, HtmlTemplateElement, Node, Text};

mod cache;
mod flat;
mod scan;

// Collect sparse IDs without shifting existing entries, then finalize before
// lookup. Generated code can move handles out of the compact sorted storage.
struct Nodes<K, V>(Vec<(K, Option<V>)>);
impl<K: Ord, V> Nodes<K, V> {
    fn new() -> Self {
        Self(Vec::new())
    }
    fn insert(&mut self, id: K, value: V) {
        self.0.push((id, Some(value)));
    }
    fn finish(&mut self) {
        // Most element descriptors are already ordered. Text descriptors can
        // interleave anchored and direct slots, which arrive in separate groups.
        if self.0.windows(2).all(|pair| pair[0].0 < pair[1].0) {
            return;
        }
        // Stable sorting retains insertion order among equal IDs. Keep the
        // first key and last value, as BTreeMap::insert did before compaction.
        self.0.sort_by(|left, right| left.0.cmp(&right.0));
        self.0.dedup_by(|next, previous| {
            if next.0 == previous.0 {
                previous.1 = next.1.take();
                true
            } else {
                false
            }
        });
    }
    fn get(&self, id: &K) -> Option<&V> {
        self.0
            .binary_search_by(|(key, _)| key.cmp(id))
            .ok()
            .and_then(|index| self.0[index].1.as_ref())
    }
    fn take(&mut self, id: &K) -> Option<V> {
        self.0
            .binary_search_by(|(key, _)| key.cmp(id))
            .ok()
            .and_then(|index| self.0[index].1.take())
    }
    fn iter(&self) -> impl Iterator<Item = (&K, &V)> {
        self.0
            .iter()
            .filter_map(|(id, value)| value.as_ref().map(|value| (id, value)))
    }
}
type Handles = Nodes<ElementId, ElementHandle>;
struct Slot {
    id: TextId,
    position: TextPosition,
    existing: Option<Text>,
}
enum TextPosition {
    Anchored { start: Node, end: Node },
    Element(Element),
}
type Mounts = BTreeMap<MountId, MountPoint>;
type Resolution = (Handles, Vec<Slot>, Mounts);

enum ElementHandle {
    Element(Element),
    Input(HtmlInputElement),
}

/// All handles have been checked against the compiler's template descriptor.
pub struct TemplateNodes {
    binding_bundle: Option<Rc<JsValue>>,
    elements: Handles,
    texts: Nodes<TextId, Text>,
    mounts: Mounts,
}

fn invalid(message: impl std::fmt::Display) -> JsValue {
    JsValue::from_str(&format!("fusor: template mismatch: {message}"))
}

impl TemplateNodes {
    /// A generated flat component can retain native targets without unpacking
    /// every handle across the Wasm boundary. Typed consumers use the old path.
    #[doc(hidden)]
    pub fn take_binding_bundle(&mut self) -> Option<Rc<JsValue>> {
        self.binding_bundle.take()
    }

    #[doc(hidden)]
    pub fn take_element(&mut self, id: ElementId) -> Result<Element, JsValue> {
        match self.elements.take(&id) {
            Some(ElementHandle::Element(element)) => Ok(element),
            Some(ElementHandle::Input(input)) => Ok(input.into()),
            None => Err(invalid(format_args!("missing element {id}"))),
        }
    }

    #[doc(hidden)]
    pub fn take_input(&mut self, id: ElementId) -> Result<HtmlInputElement, JsValue> {
        match self.elements.take(&id) {
            Some(ElementHandle::Input(input)) => Ok(input),
            _ => Err(invalid(format_args!("element {id} is not an HTML input"))),
        }
    }

    #[doc(hidden)]
    pub fn take_text(&mut self, id: TextId) -> Result<Text, JsValue> {
        self.texts
            .take(&id)
            .ok_or_else(|| invalid(format_args!("missing text {id}")))
    }

    #[doc(hidden)]
    pub fn take_mount_point(&mut self, id: MountId) -> Result<MountPoint, JsValue> {
        self.mounts
            .remove(&id)
            .ok_or_else(|| invalid(format_args!("missing component mount {id}")))
    }

    pub fn mount_point(&self, id: MountId) -> Result<MountPoint, JsValue> {
        self.mounts
            .get(&id)
            .cloned()
            .ok_or_else(|| invalid(format_args!("missing component mount {id}")))
    }

    pub fn element(&self, id: ElementId) -> Result<Element, JsValue> {
        match self.elements.get(&id) {
            Some(ElementHandle::Element(element)) => Ok(element.clone()),
            Some(ElementHandle::Input(input)) => Ok(input.clone().into()),
            None => Err(invalid(format_args!("missing element {id}"))),
        }
    }

    pub fn input(&self, id: ElementId) -> Result<HtmlInputElement, JsValue> {
        match self.elements.get(&id) {
            Some(ElementHandle::Input(input)) => Ok(input.clone()),
            _ => Err(invalid(format_args!("element {id} is not an HTML input"))),
        }
    }

    pub fn text(&self, id: TextId) -> Result<Text, JsValue> {
        self.texts
            .get(&id)
            .cloned()
            .ok_or_else(|| invalid(format_args!("missing text {id}")))
    }
}

#[derive(Clone, Copy)]
enum MountMode<'a> {
    Active,
    Prepared(Option<&'a OwnerHandle>),
    Bundled(Option<&'a OwnerHandle>),
}

impl MountMode<'_> {
    fn bundled(self) -> bool {
        matches!(self, Self::Bundled(_))
    }

    fn scope(self, root: Element) -> Scope {
        match self {
            Self::Active => Scope::new(root),
            Self::Prepared(parent) | Self::Bundled(parent) => Scope::new_prepared(root, parent),
        }
    }

    fn clone_template(self, template: &HtmlTemplateElement) -> Result<Scope, JsValue> {
        Ok(self.scope(Scope::clone_template_root(template)?))
    }
}

impl TemplateDescriptor {
    #[doc(hidden)]
    pub fn mount_with_html(&self, html: &'static str) -> Result<(Scope, TemplateNodes), JsValue> {
        self.mount_with_points(html, &[])
    }

    #[doc(hidden)]
    pub fn mount_with_points(
        &self,
        html: &'static str,
        mounts: &'static [MountId],
    ) -> Result<(Scope, TemplateNodes), JsValue> {
        self.mount_with_points_mode(html, mounts, MountMode::Active)
    }

    /// Compiler entry point: retain the final prepared owner and readiness token.
    /// Integration setup still follows successful native descriptor validation.
    #[doc(hidden)]
    pub fn prepare_with_points(
        &self,
        html: &'static str,
        mounts: &'static [MountId],
        parent: Option<&OwnerHandle>,
    ) -> Result<(Scope, TemplateNodes), JsValue> {
        let (mut scope, nodes) =
            self.mount_with_points_mode(html, mounts, MountMode::Prepared(parent))?;
        scope.finish_owner_preparation(parent);
        Ok((scope, nodes))
    }

    /// Generated ordinary flat bindings retain the validated native bundle.
    /// Coherent preparation keeps the existing typed patch targets.
    #[doc(hidden)]
    pub fn prepare_with_binding_bundle(
        &self,
        html: &'static str,
        parent: Option<&OwnerHandle>,
    ) -> Result<(Scope, TemplateNodes), JsValue> {
        let mode = if super::coherent::parent_is_coherent(parent) {
            MountMode::Prepared(parent)
        } else {
            MountMode::Bundled(parent)
        };
        let (mut scope, nodes) = self.mount_with_points_mode(html, &[], mode)?;
        scope.finish_owner_preparation(parent);
        Ok((scope, nodes))
    }

    fn mount_with_points_mode(
        &self,
        _html: &'static str,
        mounts: &'static [MountId],
        mode: MountMode<'_>,
    ) -> Result<(Scope, TemplateNodes), JsValue> {
        #[cfg(feature = "islands")]
        {
            if let Some(root) = super::delivery::take_root() {
                let metadata = strings::descriptor(self.component, self.version);
                if !metadata.version_matches(&root) || !metadata.component_matches(&root) {
                    return Err(invalid(
                        "server root identity differs from the browser template",
                    ));
                }
                let mut scope = mode.scope(root);
                scope.hydrating = true;
                return self.resolve(scope, mounts, mode.bundled());
            }
            if super::delivery::enabled() {
                let wrapper = document()?
                    .create_element("template")?
                    .dyn_into::<HtmlTemplateElement>()?;
                wrapper.set_inner_html(_html);
                let root = wrapper
                    .content()
                    .first_element_child()
                    .ok_or_else(|| invalid("empty delivery template"))?;
                let scope = match self.kind {
                    RootKind::Template => mode.clone_template(
                        &root
                            .dyn_into::<HtmlTemplateElement>()
                            .map_err(|_| invalid("expected embedded HTML template"))?,
                    )?,
                    RootKind::Existing => mode.scope(root),
                };
                return self.resolve(scope, mounts, mode.bundled());
            }
        }
        self.mount_points(mounts, mode)
    }

    /// Resolve a wrapper-free child group, adopting an existing native range
    /// during hydration without moving or replacing its nodes.
    #[doc(hidden)]
    pub fn mount_fragment(
        &self,
        html: &'static str,
        mounts: &'static [MountId],
    ) -> Result<(Scope, TemplateNodes), JsValue> {
        self.mount_fragment_mode(html, mounts, MountMode::Active)
    }

    #[doc(hidden)]
    pub fn prepare_fragment(
        &self,
        html: &'static str,
        mounts: &'static [MountId],
        parent: Option<&OwnerHandle>,
    ) -> Result<(Scope, TemplateNodes), JsValue> {
        let (mut scope, nodes) =
            self.mount_fragment_mode(html, mounts, MountMode::Prepared(parent))?;
        scope.finish_owner_preparation(parent);
        Ok((scope, nodes))
    }

    fn mount_fragment_mode(
        &self,
        html: &'static str,
        mounts: &'static [MountId],
        mode: MountMode<'_>,
    ) -> Result<(Scope, TemplateNodes), JsValue> {
        let document = document()?;
        if let Some(target) = super::children::take_hydration() {
            target.validate()?;
            let start = target
                .start
                .next_sibling()
                .ok_or_else(|| invalid("missing children start"))?;
            let end = target
                .end
                .previous_sibling()
                .ok_or_else(|| invalid("missing children end"))?;
            if start.node_value().as_deref() != Some("fusor:fragment")
                || end.node_value().as_deref() != Some("/fusor:fragment")
            {
                return Err(invalid("server children fragment mismatch"));
            }
            let fragment = MountPoint { start, end };
            fragment.validate()?;
            let mut scope = mode.scope(document.create_element("div")?);
            scope.fragment = Some(fragment);
            scope.hydrating = true;
            return self.resolve(scope, mounts, mode.bundled());
        }
        let wrapper = document
            .create_element("template")?
            .dyn_into::<HtmlTemplateElement>()?;
        wrapper.set_inner_html(html);
        let template = wrapper
            .content()
            .first_element_child()
            .ok_or_else(|| invalid("missing children template"))?
            .dyn_into::<HtmlTemplateElement>()
            .map_err(|_| invalid("expected a children template"))?;
        let root = document.create_element("div")?;
        root.append_child(&template.content().clone_node_with_deep(true)?)?;
        let (mut scope, nodes) = self.resolve(mode.scope(root), mounts, mode.bundled())?;
        let start: Node = document.create_comment("fusor:fragment").into();
        let end: Node = document.create_comment("/fusor:fragment").into();
        scope
            .root()
            .insert_before(&start, scope.root().first_child().as_ref())?;
        scope.root().append_child(&end)?;
        scope.fragment = Some(MountPoint { start, end });
        Ok((scope, nodes))
    }

    pub fn mount(&self) -> Result<(Scope, TemplateNodes), JsValue> {
        self.mount_points(&[], MountMode::Active)
    }

    fn mount_points(
        &self,
        mounts: &'static [MountId],
        mode: MountMode<'_>,
    ) -> Result<(Scope, TemplateNodes), JsValue> {
        if self.version != template::VERSION {
            return Err(invalid(
                "unsupported descriptor version; rebuild the application",
            ));
        }
        let document = document()?;
        let metadata = strings::descriptor(self.component, self.version);
        let roots = metadata.roots(&document)?;
        if roots.length() != 1 {
            return Err(invalid(format_args!(
                "component {} requires exactly one root, found {}",
                self.component,
                roots.length()
            )));
        }
        let root: Element = roots.item(0).expect("one root").dyn_into()?;
        if !metadata.version_matches(&root) {
            return Err(invalid(
                "HTML schema version differs from Wasm; rebuild the application",
            ));
        }
        let scope = match self.kind {
            RootKind::Existing => {
                if root.is_instance_of::<HtmlTemplateElement>() {
                    return Err(invalid("expected an existing element, found a template"));
                }
                mode.scope(root)
            }
            RootKind::Template => {
                let template = root
                    .dyn_into::<HtmlTemplateElement>()
                    .map_err(|_| invalid("expected an HTML template"))?;
                mode.clone_template(&template)?
            }
        };

        self.resolve(scope, mounts, mode.bundled())
    }

    fn resolve(
        &self,
        scope: Scope,
        expected_mounts: &'static [MountId],
        bundled: bool,
    ) -> Result<(Scope, TemplateNodes), JsValue> {
        if self.version != template::VERSION {
            return Err(invalid("unsupported template version"));
        }
        if bundled
            && expected_mounts.is_empty()
            && scope.fragment.is_none()
            && self.elements.iter().all(|element| {
                element.children == ChildPolicy::Static
                    && !matches!(element.tag, "input" | "textarea" | "select")
            })
            && self
                .text_elements
                .iter()
                .all(|element| !matches!(element.tag, "input" | "textarea" | "select"))
        {
            let cached = self.kind == RootKind::Template && !scope.hydrating;
            let binding_bundle = flat::resolve_bundle(self, scope.root(), cached)?;
            strings::descriptor(self.component, self.version).mark_instance(scope.root())?;
            return Ok((
                scope,
                TemplateNodes {
                    binding_bundle: Some(Rc::new(binding_bundle)),
                    elements: Handles::new(),
                    texts: Nodes::new(),
                    mounts: Mounts::new(),
                },
            ));
        }
        #[cfg(feature = "islands")]
        if scope.hydrating
            && expected_mounts.is_empty()
            && scope.fragment.is_none()
            && self
                .elements
                .iter()
                .all(|element| element.children == ChildPolicy::Static)
        {
            let (handles, slots, mounts) = flat::resolve(self, scope.root())?;
            return self.finish_resolution(scope, handles, slots, mounts);
        }
        let cached_template = self.kind == RootKind::Template && !scope.hydrating;
        if cached_template {
            if let Some((handles, slots, mounts)) =
                cache::resolve(self, expected_mounts, scope.root())?
            {
                return self.finish_resolution(scope, handles, slots, mounts);
            }
        }
        let (handles, slots, mounts) = scan::resolve(
            self,
            expected_mounts,
            scope.root(),
            scope.fragment.as_ref(),
            scope.is_hydrating(),
        )?;
        if cached_template {
            // Cache construction is optional; inability to retain an inert
            // certificate must not make a correctly validated mount fail.
            let _ = cache::remember(
                self,
                expected_mounts,
                scope.root(),
                &handles,
                &slots,
                &mounts,
            );
        }
        self.finish_resolution(scope, handles, slots, mounts)
    }

    fn finish_resolution(
        &self,
        scope: Scope,
        handles: Handles,
        slots: Vec<Slot>,
        mounts: Mounts,
    ) -> Result<(Scope, TemplateNodes), JsValue> {
        let document = document()?;
        let mut texts = Nodes::new();
        for Slot {
            id,
            position,
            existing,
        } in slots
        {
            let text = match existing {
                Some(text) => text,
                None => {
                    let text = document.create_text_node("");
                    match position {
                        TextPosition::Anchored { start, end } => {
                            start
                                .parent_node()
                                .ok_or_else(|| invalid("detached text anchor"))?
                                .insert_before(&text, Some(&end))?;
                        }
                        TextPosition::Element(element) => {
                            element.append_child(&text)?;
                        }
                    }
                    text
                }
            };
            texts.insert(id, text);
        }
        texts.finish();
        strings::descriptor(self.component, self.version).mark_instance(scope.root())?;
        Ok((
            scope,
            TemplateNodes {
                binding_bundle: None,
                elements: handles,
                texts,
                mounts,
            },
        ))
    }
}

fn text_slot(id: TextId, start: &Node, end: &Node) -> Result<Option<Text>, JsValue> {
    let next = start
        .next_sibling()
        .ok_or_else(|| invalid(format_args!("unpaired text slot {id}")))?;
    if next.is_same_node(Some(end)) {
        return Ok(None);
    }
    if !next
        .next_sibling()
        .is_some_and(|node| node.is_same_node(Some(end)))
    {
        return Err(invalid(format_args!("unexpected nodes in text slot {id}")));
    }
    next.dyn_into::<Text>()
        .map(Some)
        .map_err(|_| invalid(format_args!("expected a text node in slot {id}")))
}

/// The compiler proves this host has no static or managed child content.
/// Validate before insertion so a malformed later slot cannot partially bind it.
fn element_text(id: TextId, element: &Element) -> Result<Option<Text>, JsValue> {
    let Some(child) = element.first_child() else {
        return Ok(None);
    };
    if child.next_sibling().is_some() {
        return Err(invalid(format_args!(
            "unexpected nodes in text element {id}"
        )));
    }
    child
        .dyn_into::<Text>()
        .map(Some)
        .map_err(|_| invalid(format_args!("expected a text node in text element {id}")))
}

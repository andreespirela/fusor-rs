//! HTML traversal and validation. This module does not generate Rust code.
mod component_modules;
mod materialize;
mod native_attributes;
mod validate;

use component_modules::PendingModule;

use super::interpolation::interpolations;
use super::ir::*;
use super::tags::void_element;
use super::tokens::Rust;
use crate::{ExtractError, RustBlock, error};
use fusor::template::{
    self, ChildPolicy, ComponentId, ElementId, MountId, MountMarker, RootKind, TextId, TextMarker,
};
use html5gum::{DefaultEmitter, Token, Tokenizer};
use std::collections::{BTreeMap, BTreeSet};
use std::ops::Range;

// Async wrappers collect a single native root; a second root stays invalid
// until closing, preserving the authored closing-tag diagnostic.
enum AsyncRoot {
    Missing,
    One(ElementId),
    Multiple,
}

struct AsyncFrame {
    start: usize,
    declaration: super::async_tags::Declaration,
    root: AsyncRoot,
}

struct Region {
    start: usize,
    node: ElementId,
    directive: RegionDirective,
}

enum RegionDirective {
    Async(Rust),
    Await(Rust),
}

#[derive(Clone, Copy)]
struct BranchRef {
    owner: usize,
    binding: usize,
}

#[derive(Clone, Copy)]
enum IfPhase {
    Then,
    Else,
}

enum ControlFrame {
    If { branch: BranchRef, phase: IfPhase },
    Else,
    Match { branch: BranchRef },
    Case { aliases: Vec<Rust> },
}

impl ControlFrame {
    fn spelling(&self) -> &'static str {
        match self {
            Self::If { .. } => "If",
            Self::Else => "Else",
            Self::Match { .. } => "Match",
            Self::Case { .. } => "Case",
        }
    }
}

// Tag identity, lexical owner and the element cursor are traversal metadata.
// Feature payloads are exclusive; independent native-element properties coexist.
struct Frame {
    name: String,
    owner: Option<usize>,
    node: ElementId,
    kind: FrameKind,
}

enum FrameKind {
    Element(ElementFrame),
    Control(ControlFrame),
    Async(AsyncFrame),
    App,
    ForEach,
    Children,
    Router { binding: usize },
    Route { alias: Option<Rust> },
    Invocation(InvocationFrame),
    Hydrated { authored: String },
}

struct ElementFrame {
    text_host: Option<TextHost>,
    inert: bool,
    owns_children: bool,
    field_value: bool,
    component_root: bool,
    region: Option<Region>,
}

struct InvocationFrame {
    binding: usize,
    authored: String,
    caller: usize,
}

impl Frame {
    fn owned(name: String, owner: usize, node: ElementId, kind: FrameKind) -> Self {
        Self {
            name,
            owner: Some(owner),
            node,
            kind,
        }
    }

    fn inert(&self) -> bool {
        matches!(
            self.kind,
            FrameKind::Element(ElementFrame { inert: true, .. })
        )
    }

    fn owns_children(&self) -> bool {
        matches!(
            self.kind,
            FrameKind::Children
                | FrameKind::Hydrated { .. }
                | FrameKind::Element(ElementFrame {
                    owns_children: true,
                    ..
                })
        )
    }

    fn is_template_or_app(&self) -> bool {
        matches!(self.kind, FrameKind::App)
            || (self.name == "template"
                && matches!(
                    self.kind,
                    FrameKind::Element(ElementFrame {
                        component_root: true,
                        ..
                    })
                ))
    }

    fn requires_native_root(&self) -> bool {
        self.is_template_or_app() || matches!(self.kind, FrameKind::ForEach)
    }

    fn rendered_owner(&self) -> Option<usize> {
        self.owner
            .filter(|_| !self.inert() && !self.owns_children())
    }

    fn in_coherent_region(&self) -> bool {
        matches!(
            self.kind,
            FrameKind::Async(_)
                | FrameKind::Element(ElementFrame {
                    region: Some(_),
                    ..
                })
        )
    }

    fn lexical_aliases(&self, include_await: bool) -> &[Rust] {
        match &self.kind {
            FrameKind::Control(ControlFrame::Case { aliases }) => aliases,
            FrameKind::Route { alias } => alias.as_slice(),
            FrameKind::Async(region) if include_await => {
                region.declaration.alias().map_or(&[], std::slice::from_ref)
            }
            _ => &[],
        }
    }
}

struct TextHost {
    opening: Range<usize>,
    opening_edit: Option<usize>,
    element: Option<ElementId>,
}

pub(super) fn parse(
    source: &str,
    blocks: &[RustBlock],
    first_component: usize,
) -> Result<Plan, ExtractError> {
    let mut emitter = DefaultEmitter::<usize>::new_with_span();
    emitter.naively_switch_states(true);
    let foreach_hosts = super::foreach::hosts(source)?;
    let mut stack: Vec<Frame> = Vec::new();
    let mut components: Vec<Component> = Vec::new();
    let mut named_content = BTreeSet::new();
    let mut component_types = std::collections::BTreeSet::new();
    let mut template_roots = BTreeMap::new();
    let mut edits = Vec::new();
    let mut text_edits: BTreeMap<(usize, usize), (TextId, usize)> = BTreeMap::new();
    let mut node = 0;
    let mut slot = 0;
    let mut mount = 0;
    let mut pending_module: Option<(usize, PendingModule)> = None;
    for token in Tokenizer::new_with_emitter(source, emitter) {
        let token = token.expect("in-memory HTML");
        if pending_module.is_some() {
            if let Token::EndTag(tag) = &token {
                if &*tag.name == b"script" {
                    let (owner, pending) = pending_module.take().unwrap();
                    let (module, edit) = pending.finish(source, tag.span.start..tag.span.end)?;
                    edits.push(edit);
                    components[owner].javascript = Some(module);
                }
            }
            continue;
        }
        match token {
            Token::StartTag(tag) => {
                let name = String::from_utf8_lossy(&tag.name).into_owned();
                let parent = stack
                    .iter()
                    .rev()
                    .find(|frame| !matches!(frame.kind, FrameKind::Async(_)));
                let snapshot_locals = parent
                    .and_then(|frame| frame.owner)
                    .map(|owner| components[owner].snapshot_locals.clone())
                    .unwrap_or_default();
                // Route parameters and Case handles are ordinary lexical
                // captures; Await snapshots need refreshable retained storage.
                let route_locals: Vec<Rust> = stack
                    .iter()
                    .flat_map(|frame| frame.lexical_aliases(false))
                    .cloned()
                    .collect();
                let async_locals: Vec<Rust> = stack
                    .iter()
                    .flat_map(|frame| frame.lexical_aliases(true))
                    .cloned()
                    .collect();
                if parent.is_some_and(|frame| {
                    (matches!(frame.kind, FrameKind::Control(ControlFrame::Match { .. }))
                        && name != "case")
                        || matches!(
                            frame.kind,
                            FrameKind::Control(ControlFrame::If {
                                phase: IfPhase::Else,
                                ..
                            })
                        )
                }) {
                    return Err(error(
                        source,
                        tag.span.start,
                        "Match accepts only Case children; Else must be last in If",
                    ));
                }
                if name == "script"
                    && tag.attributes.get(b"type".as_slice()).is_some_and(|value| {
                        value.as_ref().trim_ascii().eq_ignore_ascii_case(b"module")
                    })
                {
                    if let Some(owner) = parent.and_then(|frame| frame.owner) {
                        let component = &components[owner];
                        if !parent.is_some_and(|frame| frame.is_template_or_app())
                            || component.capture.is_some()
                            || component.inline
                            || component.render != RenderTarget::Browser
                            || stack.iter().any(|frame| frame.in_coherent_region())
                        {
                            return Err(error(
                                source,
                                tag.span.start,
                                "component modules must be direct children of a browser component template or App, outside coherent Async, projected content, and server/island delivery",
                            ));
                        }
                        pending_module =
                            Some((owner, PendingModule::begin(source, &tag, component)?));
                        continue;
                    }
                }
                let projected = tag.attributes.get(b"rust:content".as_slice());
                let authored_name = super::tags::name(source, tag.span.start);
                if matches!(name.as_str(), "async" | "await") {
                    let awaiting = name == "await";
                    let expected = if awaiting { "Await" } else { "Async" };
                    if authored_name != expected {
                        return Err(error(
                            source,
                            tag.span.start,
                            "async built-ins are spelled Async and Await",
                        ));
                    }
                    let owner = parent.and_then(Frame::rendered_owner).ok_or_else(|| {
                        error(
                            source,
                            tag.span.start,
                            "Async and Await require rendered component HTML",
                        )
                    })?;
                    if components[owner].render != RenderTarget::Browser {
                        return Err(error(
                            source,
                            tag.span.start,
                            "Async and Await require browser templates; use an island for server-rendered async views",
                        ));
                    }
                    if !awaiting && stack.iter().any(|frame| frame.in_coherent_region()) {
                        return Err(error(
                            source,
                            tag.span.start,
                            "nested Async boundaries are unsupported; Await automatically uses its enclosing boundary",
                        ));
                    }
                    let declaration = super::async_tags::inputs(source, &tag, awaiting)?;
                    if declaration.alias().is_some_and(|alias| {
                        async_locals
                            .iter()
                            .any(|name| name.tokens.to_string() == alias.tokens.to_string())
                            || components[owner].locals.iter().any(|(item, index)| {
                                item.tokens.to_string() == alias.tokens.to_string()
                                    || index.tokens.to_string() == alias.tokens.to_string()
                            })
                    }) {
                        return Err(error(
                            source,
                            tag.span.start,
                            "choose a distinct Await name; it cannot shadow an enclosing Await or ForEach binding",
                        ));
                    }
                    edits.push(Edit {
                        range: tag.span.start..tag.span.end,
                        replacement: String::new(),
                    });
                    stack.push(Frame::owned(
                        name,
                        owner,
                        ElementId::new(node),
                        FrameKind::Async(AsyncFrame {
                            start: components[owner].bindings.len(),
                            declaration,
                            root: AsyncRoot::Missing,
                        }),
                    ));
                    continue;
                }
                if stack
                    .last()
                    .is_some_and(|frame| matches!(frame.kind, FrameKind::Async(_)))
                    && (super::tags::is_component(authored_name)
                        || matches!(name.as_str(), "template" | "script" | "style" | "select"))
                {
                    return Err(error(
                        source,
                        tag.span.start,
                        "Async and Await require one native rendered HTML root; place components inside that root",
                    ));
                }
                if name == "app" && authored_name != "App" {
                    return Err(error(
                        source,
                        tag.span.start,
                        "the built-in application boundary is spelled App",
                    ));
                }
                if authored_name == "App" {
                    if parent.is_some_and(|frame| {
                        frame.owner.is_some()
                            || frame.inert()
                            || matches!(frame.kind, FrameKind::Invocation(_))
                    }) || stack.iter().any(|frame| {
                        matches!(frame.name.as_str(), "svg" | "math" | "select" | "template")
                    }) {
                        return Err(error(
                            source,
                            tag.span.start,
                            "App must be a top-level application boundary, outside components and inert or foreign HTML",
                        ));
                    }
                    let state = super::application::state(source, &tag)?;
                    let id = ComponentId::new(first_component + components.len());
                    let owner = components.len();
                    components.push(Component {
                        async_locals: async_locals.clone(),
                        route_locals: route_locals.clone(),
                        snapshot_locals: snapshot_locals.clone(),
                        ..Component::new(
                            id,
                            Rust::parse(source, "__FusorApp", tag.span.start)?,
                            ComponentShape::App(state),
                            RenderTarget::Browser,
                            tag.span.start..tag.span.end,
                        )
                    });
                    template_roots.insert(owner, 0);
                    edits.push(Edit {
                        range: tag.span.start..tag.span.end,
                        replacement: String::new(),
                    });
                    stack.push(Frame::owned(
                        name,
                        owner,
                        ElementId::new(node),
                        FrameKind::App,
                    ));
                    continue;
                }
                if super::tags::name(source, tag.span.start) == "ForEach" {
                    let parent = parent.ok_or_else(|| {
                        error(
                            source,
                            tag.span.start,
                            "ForEach requires a native HTML list container",
                        )
                    })?;
                    let caller = parent.rendered_owner().ok_or_else(|| {
                        error(
                            source,
                            tag.span.start,
                            "ForEach requires rendered HTML inside a component",
                        )
                    })?;
                    if parent.is_template_or_app() {
                        return Err(error(
                            source,
                            tag.span.start,
                            "put ForEach inside a native HTML container",
                        ));
                    }
                    let (items, key, item, index) = super::foreach::inputs(source, &tag)?;
                    if async_locals.iter().any(|alias| {
                        alias.tokens.to_string() == item.tokens.to_string()
                            || alias.tokens.to_string() == index.tokens.to_string()
                    }) {
                        return Err(error(
                            source,
                            tag.span.start,
                            "ForEach names cannot shadow an enclosing Await binding",
                        ));
                    }

                    let id = ComponentId::new(first_component + components.len());
                    let body = components.len();
                    let mut locals = components[caller].locals.clone();
                    locals.push((item, index));
                    let render = components[caller].render;
                    components[caller].bindings.push(Binding::ForEach {
                        node: parent.node,
                        items,
                        key,
                        body,
                    });
                    components.push(Component {
                        locals,
                        async_locals: async_locals.clone(),
                        route_locals: route_locals.clone(),
                        snapshot_locals: snapshot_locals.clone(),
                        ..Component::new(
                            id,
                            Rust::parse(
                                source,
                                &format!("__FusorForEach{}", id.index()),
                                tag.span.start,
                            )?,
                            ComponentShape::Row,
                            render,
                            tag.span.start..tag.span.end,
                        )
                    });
                    template_roots.insert(body, 0);
                    edits.push(Edit {
                        range: tag.span.start..tag.span.end,
                        replacement: format!(
                            "<template {}=\"{}\" {}=\"{}\">",
                            template::COMPONENT_ATTRIBUTE,
                            id,
                            template::VERSION_ATTRIBUTE,
                            template::VERSION
                        ),
                    });
                    stack.push(Frame::owned(
                        "foreach".into(),
                        body,
                        parent.node,
                        FrameKind::ForEach,
                    ));
                    continue;
                }
                let caller = parent.and_then(|frame| match &frame.kind {
                    FrameKind::Invocation(invocation) => Some(invocation),
                    _ => None,
                });
                if projected.is_some() && (caller.is_none() || name != "template") {
                    return Err(error(
                        source,
                        tag.span.start,
                        "named content must be declared directly inside a component tag",
                    ));
                }
                if projected.is_none()
                    && parent.is_some_and(|frame| {
                        matches!(frame.kind, FrameKind::Invocation(_))
                            && named_content.contains(&frame.owner.unwrap())
                    })
                {
                    return Err(error(
                        source,
                        tag.span.start,
                        "do not mix named content and ordinary children in one invocation",
                    ));
                }
                let authored_name = super::tags::name(source, tag.span.start);
                if name == "children" && authored_name != "Children" {
                    return Err(error(
                        source,
                        tag.span.start,
                        "the built-in is spelled Children",
                    ));
                }
                if matches!(name.as_str(), "if" | "else" | "match" | "case") {
                    let expected = match name.as_str() {
                        "if" => "If",
                        "else" => "Else",
                        "match" => "Match",
                        _ => "Case",
                    };
                    if authored_name != expected || tag.self_closing {
                        return Err(error(
                            source,
                            tag.span.start,
                            "If, Else, Match and Case require exact spelling and explicit closing tags",
                        ));
                    }
                    let owner = parent.and_then(Frame::rendered_owner).ok_or_else(|| {
                        error(
                            source,
                            tag.span.start,
                            "control flow requires rendered component HTML",
                        )
                    })?;
                    let mut frame_owner = owner;
                    let control;
                    let mut replacement = String::new();
                    if matches!(name.as_str(), "if" | "match") {
                        if parent.is_some_and(|frame| {
                            matches!(frame.kind, FrameKind::Router { .. })
                                || frame.requires_native_root()
                        }) || stack.iter().any(|frame| {
                            matches!(
                                frame.name.as_str(),
                                "select"
                                    | "option"
                                    | "svg"
                                    | "math"
                                    | "textarea"
                                    | "table"
                                    | "tbody"
                                    | "thead"
                                    | "tfoot"
                                    | "tr"
                            )
                        }) {
                            return Err(error(
                                source,
                                tag.span.start,
                                "If and Match belong inside an ordinary native HTML container, outside table/select/SVG/MathML parsing contexts",
                            ));
                        }
                        let value = super::control::expression(
                            source,
                            &tag,
                            if name == "if" { b"condition" } else { b"value" },
                        )?;
                        let point = MountId::new(mount);
                        mount += 1;
                        let binding = components[owner].bindings.len();
                        let snapshots = stack
                            .iter()
                            .filter_map(|frame| match &frame.kind {
                                FrameKind::Async(region) => region.declaration.alias().cloned(),
                                _ => None,
                            })
                            .map(|alias| {
                                let method = if snapshot_locals.iter().any(|local| {
                                    local.tokens.to_string() == alias.tokens.to_string()
                                }) {
                                    "get"
                                } else {
                                    "clone"
                                };
                                let read = Rust::parse(
                                    source,
                                    &format!("{}.{}()", alias.tokens, method),
                                    alias.offset,
                                )?;
                                Ok((alias, read))
                            })
                            .collect::<Result<Vec<_>, ExtractError>>()?;
                        components[owner].bindings.push(Binding::Branch {
                            point,
                            value,
                            cases: Vec::new(),
                            snapshots,
                        });
                        let branch = BranchRef { owner, binding };
                        replacement = format!(
                            "<!--{}--><!--{}-->",
                            MountMarker::Start(point),
                            MountMarker::End(point)
                        );
                        if name == "if" {
                            frame_owner = super::control::body(
                                source,
                                &mut components,
                                owner,
                                first_component,
                                tag.span.end,
                                async_locals.clone(),
                                route_locals.clone(),
                            )?;
                            if let Binding::Branch { snapshots, .. } =
                                &components[owner].bindings[binding]
                            {
                                components[frame_owner].snapshot_locals =
                                    snapshots.iter().map(|(name, _)| name.clone()).collect();
                            }
                            let Binding::Branch { cases, .. } =
                                &mut components[owner].bindings[binding]
                            else {
                                unreachable!()
                            };
                            cases.push(CaseBranch {
                                pattern: Rust::parse(source, "true", tag.span.start)?,
                                names: Vec::new(),
                                body: frame_owner,
                            });
                            control = ControlFrame::If {
                                branch,
                                phase: IfPhase::Then,
                            };
                        } else {
                            control = ControlFrame::Match { branch };
                        }
                    } else {
                        let branch = match parent.map(|frame| &frame.kind) {
                            Some(FrameKind::Control(ControlFrame::Match { branch }))
                                if name == "case" =>
                            {
                                *branch
                            }
                            Some(FrameKind::Control(ControlFrame::If { branch, .. }))
                                if name == "else" =>
                            {
                                *branch
                            }
                            _ => {
                                return Err(error(
                                    source,
                                    tag.span.start,
                                    "Case must be a direct child of Match; Else must be the final direct child of If",
                                ));
                            }
                        };
                        let BranchRef {
                            owner: caller,
                            binding,
                        } = branch;
                        let (pattern, names) = if name == "case" {
                            super::control::pattern(source, &tag)?
                        } else {
                            if !tag.attributes.is_empty() {
                                return Err(error(
                                    source,
                                    tag.span.start,
                                    "Else takes no attributes",
                                ));
                            }
                            (Rust::parse(source, "false", tag.span.start)?, Vec::new())
                        };
                        for alias in &names {
                            if async_locals
                                .iter()
                                .chain(components[caller].locals.iter().flat_map(|(a, b)| [a, b]))
                                .any(|other| other.tokens.to_string() == alias.tokens.to_string())
                            {
                                return Err(error(
                                    source,
                                    alias.offset,
                                    "Case binding cannot shadow an enclosing local",
                                ));
                            }
                        }
                        let mut captures = async_locals.clone();
                        captures.extend(names.clone());
                        let mut lexical = route_locals.clone();
                        lexical.extend(names.clone());
                        frame_owner = super::control::body(
                            source,
                            &mut components,
                            caller,
                            first_component,
                            tag.span.end,
                            captures,
                            lexical,
                        )?;
                        if let Binding::Branch { snapshots, .. } =
                            &components[caller].bindings[binding]
                        {
                            components[frame_owner].snapshot_locals =
                                snapshots.iter().map(|(name, _)| name.clone()).collect();
                        }
                        let Binding::Branch { cases, .. } =
                            &mut components[caller].bindings[binding]
                        else {
                            unreachable!()
                        };
                        cases.push(CaseBranch {
                            pattern,
                            names: names.clone(),
                            body: frame_owner,
                        });
                        control = if name == "case" {
                            ControlFrame::Case { aliases: names }
                        } else {
                            ControlFrame::Else
                        };
                        if name == "else" {
                            let parent = stack.last_mut().expect("If frame");
                            components[parent.owner.unwrap()].range.end = tag.span.start;
                            let FrameKind::Control(ControlFrame::If { phase, .. }) =
                                &mut parent.kind
                            else {
                                unreachable!("validated If parent")
                            };
                            *phase = IfPhase::Else;
                        }
                    }
                    edits.push(Edit {
                        range: tag.span.start..tag.span.end,
                        replacement,
                    });
                    stack.push(Frame::owned(
                        name,
                        frame_owner,
                        ElementId::new(node),
                        FrameKind::Control(control),
                    ));
                    continue;
                }

                if authored_name == "Children" {
                    let owner = parent.and_then(Frame::rendered_owner).ok_or_else(|| {
                        error(
                            source,
                            tag.span.start,
                            "Children requires a reusable component template",
                        )
                    })?;
                    if tag.self_closing || !tag.attributes.is_empty() {
                        return Err(error(
                            source,
                            tag.span.start,
                            "write <Children></Children> without attributes",
                        ));
                    }
                    if parent.is_some_and(|frame| frame.requires_native_root()) {
                        return Err(error(
                            source,
                            tag.span.start,
                            "Children belongs inside the component's native HTML root",
                        ));
                    }
                    if components[owner].app.is_some() || !components[owner].locals.is_empty() {
                        return Err(error(
                            source,
                            tag.span.start,
                            "Children requires a reusable component and cannot be repeated inside ForEach",
                        ));
                    }
                    let point = MountId::new(mount);
                    mount += 1;
                    components[owner].bindings.push(Binding::Children {
                        point,
                        origin: Rust::parse(source, "Children", tag.span.start)?,
                    });
                    edits.push(Edit {
                        range: tag.span.start..tag.span.end,
                        replacement: format!(
                            "<!--{}--><!--{}-->",
                            MountMarker::Start(point),
                            MountMarker::End(point)
                        ),
                    });
                    stack.push(Frame::owned(
                        name,
                        owner,
                        ElementId::new(node),
                        FrameKind::Children,
                    ));
                    continue;
                }
                if matches!(name.as_str(), "router" | "route") {
                    let expected = if name == "router" { "Router" } else { "Route" };
                    if authored_name != expected || tag.self_closing {
                        return Err(error(
                            source,
                            tag.span.start,
                            "Router and Route require exact spelling and explicit closing tags",
                        ));
                    }
                    let owner = parent.and_then(|frame| frame.owner).ok_or_else(|| {
                        error(
                            source,
                            tag.span.start,
                            "Router and Route require component HTML",
                        )
                    })?;
                    if components[owner].render != RenderTarget::Browser
                        || stack.iter().any(|frame| frame.in_coherent_region())
                    {
                        return Err(error(
                            source,
                            tag.span.start,
                            "Router requires browser HTML outside coherent Async regions",
                        ));
                    }
                    if name == "router" {
                        if !tag.attributes.is_empty()
                            || stack.iter().any(|frame| {
                                matches!(frame.name.as_str(), "select" | "svg" | "math")
                            })
                            || parent.is_some_and(|frame| {
                                matches!(frame.kind, FrameKind::Router { .. })
                                    || frame.owns_children()
                                    || frame.is_template_or_app()
                            })
                        {
                            return Err(error(
                                source,
                                tag.span.start,
                                "Router takes Route children and belongs inside a native HTML root",
                            ));
                        }
                        let point = MountId::new(mount);
                        mount += 1;
                        let binding = components[owner].bindings.len();
                        components[owner].bindings.push(Binding::Router {
                            point,
                            origin: Rust::parse(source, "Router", tag.span.start)?,
                            routes: Vec::new(),
                        });
                        edits.push(Edit {
                            range: tag.span.start..tag.span.end,
                            replacement: format!(
                                "<!--{}--><!--{}-->",
                                MountMarker::Start(point),
                                MountMarker::End(point)
                            ),
                        });
                        stack.push(Frame::owned(
                            name,
                            owner,
                            ElementId::new(node),
                            FrameKind::Router { binding },
                        ));
                    } else {
                        let Some(FrameKind::Router { binding }) = parent.map(|frame| &frame.kind)
                        else {
                            return Err(error(
                                source,
                                tag.span.start,
                                "Route must be a direct child of Router",
                            ));
                        };
                        let binding = *binding;
                        let super::router_tags::Declaration { path, alias, names } =
                            super::router_tags::route(source, &tag)?;
                        let Binding::Router { routes, .. } = &components[owner].bindings[binding]
                        else {
                            unreachable!()
                        };
                        super::router_tags::validate(
                            source,
                            tag.span.start,
                            routes,
                            path.as_deref(),
                        )?;
                        if alias.as_ref().is_some_and(|alias| {
                            async_locals
                                .iter()
                                .any(|other| other.tokens.to_string() == alias.tokens.to_string())
                                || components[owner].locals.iter().any(|(a, b)| {
                                    [a, b].iter().any(|other| {
                                        other.tokens.to_string() == alias.tokens.to_string()
                                    })
                                })
                        }) {
                            return Err(error(
                                source,
                                tag.span.start,
                                "Route binding cannot shadow an enclosing local",
                            ));
                        }
                        let index = components.len();
                        let id = ComponentId::new(first_component + index);
                        let mut captured_locals = async_locals.clone();
                        let mut route_bindings = route_locals.clone();
                        if let Some(alias) = &alias {
                            captured_locals.push(alias.clone());
                            route_bindings.push(alias.clone());
                        }
                        components.push(Component {
                            locals: components[owner].locals.clone(),
                            async_locals: captured_locals,
                            route_locals: route_bindings,
                            snapshot_locals: snapshot_locals.clone(),
                            ..Component::new(
                                id,
                                Rust::parse(
                                    source,
                                    &format!("__FusorRoute{}", id.index()),
                                    tag.span.start,
                                )?,
                                ComponentShape::Fragment(components[owner].ty.clone()),
                                RenderTarget::Browser,
                                tag.span.end..tag.span.end,
                            )
                        });
                        let Binding::Router { routes, .. } =
                            &mut components[owner].bindings[binding]
                        else {
                            unreachable!()
                        };
                        routes.push(RouteBranch {
                            path,
                            params: alias.clone(),
                            names,
                            body: index,
                        });
                        edits.push(Edit {
                            range: tag.span.start..tag.span.end,
                            replacement: String::new(),
                        });
                        stack.push(Frame::owned(
                            name,
                            index,
                            ElementId::new(node),
                            FrameKind::Route { alias },
                        ));
                    }
                    continue;
                }
                if parent.is_some_and(|frame| matches!(frame.kind, FrameKind::Router { .. })) {
                    return Err(error(
                        source,
                        tag.span.start,
                        "Router accepts only direct Route children",
                    ));
                }
                if tag.attributes.contains_key(b"hydrate".as_slice()) {
                    let owner = parent.and_then(Frame::rendered_owner).ok_or_else(|| {
                        error(
                            source,
                            tag.span.start,
                            "hydrate requires rendered HTML inside a server component",
                        )
                    })?;
                    if components[owner].render != RenderTarget::Server {
                        return Err(error(
                            source,
                            tag.span.start,
                            "hydrate requires a server-rendered component",
                        ));
                    }
                    if parent.is_some_and(|frame| frame.requires_native_root())
                        || stack
                            .iter()
                            .any(|frame| matches!(frame.name.as_str(), "svg" | "math"))
                        || parent.is_some_and(|frame| {
                            matches!(
                                frame.name.as_str(),
                                "table"
                                    | "thead"
                                    | "tbody"
                                    | "tfoot"
                                    | "tr"
                                    | "select"
                                    | "optgroup"
                                    | "p"
                                    | "head"
                                    | "html"
                            )
                        })
                    {
                        return Err(error(
                            source,
                            tag.span.start,
                            "a hydrated component renders a div boundary; place it inside a native HTML container that accepts div children",
                        ));
                    }
                    let element = ElementId::new(node);
                    let replacement =
                        super::hydration::lower(source, &tag, element, &mut components[owner])?;
                    node += 1;
                    edits.push(Edit {
                        range: tag.span.start..tag.span.end,
                        replacement,
                    });
                    stack.push(Frame::owned(
                        name,
                        owner,
                        element,
                        FrameKind::Hydrated {
                            authored: authored_name.to_owned(),
                        },
                    ));
                    continue;
                }
                if super::tags::is_component(authored_name) {
                    let owner = parent.and_then(Frame::rendered_owner).ok_or_else(|| {
                        error(
                            source,
                            tag.span.start,
                            "component tags require rendered HTML inside a rust:component",
                        )
                    })?;
                    if parent.is_some_and(|frame| frame.is_template_or_app()) {
                        return Err(error(
                            source,
                            tag.span.start,
                            "a template requires a native HTML root; component tags belong inside that root",
                        ));
                    }
                    if parent.is_some_and(|frame| matches!(frame.kind, FrameKind::ForEach)) {
                        *template_roots.entry(owner).or_insert(0) += 1;
                    }
                    if tag.self_closing {
                        return Err(error(
                            source,
                            tag.span.start,
                            "component tags need an explicit closing tag",
                        ));
                    }
                    let point = MountId::new(mount);
                    mount += 1;
                    let binding = components[owner].bindings.len();
                    let mut invocation = super::tags::invocation(source, &tag, point)?;
                    if parent.is_some_and(|frame| matches!(frame.kind, FrameKind::ForEach))
                        && matches!(
                            &invocation,
                            Binding::Invocation {
                                condition: Some(_),
                                ..
                            } | Binding::Invocation { key: Some(_), .. }
                        )
                    {
                        return Err(error(
                            source,
                            tag.span.start,
                            "ForEach owns row identity; put conditional or separately keyed components inside a native row element",
                        ));
                    }
                    let fragment_index = components.len();
                    let id = ComponentId::new(first_component + fragment_index);
                    let capture = components[owner].ty.clone();
                    let render = components[owner].render;
                    let locals = components[owner].locals.clone();
                    components.push(Component {
                        locals,
                        async_locals: async_locals.clone(),
                        route_locals: route_locals.clone(),
                        snapshot_locals: snapshot_locals.clone(),
                        ..Component::new(
                            id,
                            Rust::parse(
                                source,
                                &format!("__FusorChildren{}", id.index()),
                                tag.span.start,
                            )?,
                            ComponentShape::Fragment(capture),
                            render,
                            tag.span.end..tag.span.end,
                        )
                    });
                    if let Binding::Invocation { children, .. } = &mut invocation {
                        *children = Some(fragment_index);
                    }
                    components[owner].bindings.push(invocation);
                    edits.push(Edit {
                        range: tag.span.start..tag.span.end,
                        replacement: format!(
                            "<!--{}--><!--{}-->",
                            MountMarker::Start(point),
                            MountMarker::End(point)
                        ),
                    });
                    stack.push(Frame::owned(
                        name,
                        fragment_index,
                        ElementId::new(node),
                        FrameKind::Invocation(InvocationFrame {
                            binding,
                            authored: authored_name.to_owned(),
                            caller: owner,
                        }),
                    ));
                    continue;
                }
                if parent.is_some_and(|frame| frame.owns_children()) {
                    return Err(error(
                        source,
                        tag.span.start,
                        "an owned host or hydrated component must be empty; its registered renderer supplies its contents",
                    ));
                }
                if let Some(parent) = parent.filter(|parent| parent.requires_native_root()) {
                    if !blocks
                        .iter()
                        .any(|block| block.element.start == tag.span.start)
                    {
                        *template_roots
                            .entry(parent.owner.expect("component root"))
                            .or_insert(0) += 1;
                    }
                }
                let mut owner = parent.and_then(|frame| frame.owner);
                let mut inert = parent.is_some_and(|frame| frame.inert());
                let declaration = tag.attributes.get(b"rust:component".as_slice());
                // Rust script source metadata is consumed by the extractor, not
                // by the component binding language, including scripts in templates.
                let rust_script = blocks
                    .iter()
                    .any(|block| block.element.start == tag.span.start);
                if rust_script
                    && owner.is_some_and(|index| {
                        components[index].capture.is_some()
                            || components[index].inline
                            || components[index].app.is_some()
                    })
                {
                    return Err(error(
                        source,
                        tag.span.start,
                        "Rust scripts do not belong inside App, projected content, or ForEach rows; keep state in the associated Rust module",
                    ));
                }
                let app_root = parent.is_some_and(|frame| matches!(frame.kind, FrameKind::App));
                if app_root
                    && (rust_script
                        || matches!(
                            name.as_str(),
                            "template" | "script" | "style" | "svg" | "math"
                        ))
                {
                    return Err(error(
                        source,
                        tag.span.start,
                        "App requires a native rendered HTML root; put Rust scripts outside App",
                    ));
                }
                let mut component_id = app_root.then(|| components[owner.expect("App owner")].id);
                if projected.is_some()
                    && owner.is_some_and(|index| !components[index].locals.is_empty())
                {
                    return Err(error(
                        source,
                        tag.span.start,
                        "named content inside ForEach is not supported yet; pass the row to a reusable component",
                    ));
                }
                if projected.is_some()
                    && (components[owner.expect("content owner")].render != RenderTarget::Browser
                        || stack.iter().any(|frame| frame.in_coherent_region()))
                {
                    return Err(error(
                        source,
                        tag.span.start,
                        "projected content requires a browser template outside coherent regions; pass typed data inputs to shared or coherent components",
                    ));
                }
                if let Some(content) = projected {
                    let fragment = &mut components[owner.expect("fragment owner")];
                    if !named_content.contains(&owner.expect("fragment owner"))
                        && !source[fragment.range.start..tag.span.start]
                            .trim()
                            .is_empty()
                    {
                        return Err(error(
                            source,
                            tag.span.start,
                            "do not mix named content and ordinary children in one invocation",
                        ));
                    }
                    named_content.insert(owner.expect("fragment owner"));
                    if tag.attributes.len() != 1 {
                        return Err(error(
                            source,
                            tag.span.start,
                            "a projected template only accepts rust:content; put HTML attributes on its root",
                        ));
                    }
                    let field = super::tags::field(
                        source,
                        &String::from_utf8_lossy(content),
                        content.span.start,
                    )?;
                    let caller_owner = caller.expect("validated caller").caller;
                    let id = ComponentId::new(first_component + components.len());
                    let ty = Rust::parse(
                        source,
                        &format!("__FusorContent{}", id.index()),
                        tag.span.start,
                    )?;
                    let capture = components[caller_owner]
                        .capture
                        .as_ref()
                        .unwrap_or(&components[caller_owner].ty)
                        .clone();
                    let content_index = components.len();
                    let Binding::Invocation { inputs, .. } = &mut components[caller_owner].bindings
                        [caller.expect("validated invocation").binding]
                    else {
                        unreachable!()
                    };
                    if inputs
                        .iter()
                        .any(|input| input.name.tokens.to_string() == field.tokens.to_string())
                    {
                        return Err(error(
                            source,
                            content.span.start,
                            "duplicate component input or content name",
                        ));
                    }
                    inputs.push(Input {
                        name: field,
                        value: InputValue::Content {
                            component: content_index,
                            origin: ty.clone(),
                        },
                    });
                    owner = Some(components.len());
                    component_id = Some(id);
                    template_roots.insert(components.len(), 0);
                    components.push(Component {
                        async_locals: async_locals.clone(),
                        route_locals: route_locals.clone(),
                        snapshot_locals: snapshot_locals.clone(),
                        ..Component::new(
                            id,
                            ty,
                            ComponentShape::Content(capture),
                            RenderTarget::Browser,
                            tag.span.start..tag.span.end,
                        )
                    });
                } else if let Some(ty) = declaration {
                    if owner.is_some() || inert {
                        return Err(error(
                            source,
                            tag.span.start,
                            "declare components separately; compose them with component tags",
                        ));
                    }
                    let ty = String::from_utf8_lossy(ty).trim().to_owned();
                    if ty.is_empty() || !component_types.insert(ty.clone()) {
                        return Err(error(
                            source,
                            tag.span.start,
                            "rust:component needs a unique Rust type; use a template for repeated instances",
                        ));
                    }
                    owner = Some(components.len());
                    component_id = owner.map(|index| ComponentId::new(first_component + index));
                    components.push(Component {
                        async_locals: async_locals.clone(),
                        route_locals: route_locals.clone(),
                        snapshot_locals: snapshot_locals.clone(),
                        ..Component::new(
                            component_id.expect("declared component"),
                            Rust::parse(source, &ty, tag.span.start)?,
                            ComponentShape::Declared(if name == "template" {
                                RootKind::Template
                            } else {
                                RootKind::Existing
                            }),
                            RenderTarget::Browser,
                            tag.span.start..tag.span.end,
                        )
                    });
                    if name == "template" {
                        template_roots
                            .entry(owner.expect("declared component"))
                            .or_insert(0);
                    }
                } else if name == "template" {
                    inert = true;
                }
                if tag
                    .attributes
                    .keys()
                    .any(|key| template::reserved_attribute(&String::from_utf8_lossy(key)))
                {
                    return Err(error(
                        source,
                        tag.span.start,
                        "data-fusor-* attributes are reserved for the HTML compiler",
                    ));
                }
                let has_directive = !rust_script
                    && tag
                        .attributes
                        .keys()
                        .any(|key| native_attributes::is_directive(&String::from_utf8_lossy(key)));
                let has_interpolation = owner.is_some()
                    && tag
                        .attributes
                        .values()
                        .any(|value| value.windows(2).any(|bytes| bytes == b"{{"));
                let async_attr = tag.attributes.get(b"rust:async".as_slice());
                let await_attr = tag.attributes.get(b"rust:await".as_slice());
                if async_attr.is_some()
                    && (await_attr.is_some()
                        || stack.iter().any(|frame| frame.in_coherent_region()))
                {
                    return Err(error(
                        source,
                        tag.span.start,
                        "nested async boundaries and async/await on the same host are unsupported",
                    ));
                }
                let in_coherent =
                    async_attr.is_some() || stack.iter().any(|frame| frame.in_coherent_region());
                if in_coherent
                    && (name.contains('-')
                        || tag.attributes.keys().any(|key| {
                            matches!(
                                key.as_ref(),
                                b"is"
                                    | b"bind:value"
                                    | b"bind:checked"
                                    | b"bind:field"
                                    | b"rust:slot"
                            )
                        }))
                {
                    return Err(error(
                        source,
                        tag.span.start,
                        "coherent regions support generated display bindings; put editable controls, widgets, outlets, opaque content and islands outside rust:async",
                    ));
                }
                let region = if let Some(value) = async_attr.or(await_attr) {
                    let index = owner.ok_or_else(|| {
                        error(source, tag.span.start, "async regions require a component")
                    })?;
                    let value =
                        Rust::parse(source, &String::from_utf8_lossy(value), value.span.start)?;
                    Some(Region {
                        start: components[index].bindings.len(),
                        node: ElementId::new(node),
                        directive: if await_attr.is_some() {
                            RegionDirective::Await(value)
                        } else {
                            RegionDirective::Async(value)
                        },
                    })
                } else {
                    None
                };
                if (has_directive || has_interpolation) && (owner.is_none() || inert) {
                    return Err(error(
                        source,
                        tag.span.start,
                        "bindings require a rust:component and cannot live in an inert template",
                    ));
                }
                if name == "template"
                    && component_id.is_some()
                    && projected.is_none()
                    && (has_interpolation
                        || tag.attributes.keys().any(|key| {
                            key.as_ref() != b"rust:component"
                                && key.as_ref() != b"rust:render"
                                && native_attributes::is_directive(&String::from_utf8_lossy(key))
                        }))
                {
                    return Err(error(
                        source,
                        tag.span.start,
                        "put template bindings on the root element inside the template",
                    ));
                }
                let element_id = ElementId::new(node);
                if let Some(index) = owner.filter(|_| !inert && !rust_script) {
                    if matches!(name.as_str(), "svg" | "math") {
                        return Err(error(
                            source,
                            tag.span.start,
                            "this first pass supports HTML components; SVG/MathML bindings are not supported",
                        ));
                    }
                    if tag.self_closing && !void_element(&name) {
                        return Err(error(
                            source,
                            tag.span.start,
                            "non-void component elements need explicit closing tags",
                        ));
                    }
                    if projected.is_some() {
                        edits.push(Edit {
                            range: tag.span.start..tag.span.end,
                            replacement: format!(
                                "<template {}=\"{}\" {}=\"{}\">",
                                template::COMPONENT_ATTRIBUTE,
                                component_id.expect("content component"),
                                template::VERSION_ATTRIBUTE,
                                template::VERSION
                            ),
                        });
                    } else if let Some(replacement) = native_attributes::lower(
                        source,
                        &tag,
                        &mut components[index],
                        ElementId::new(node),
                        component_id,
                        foreach_hosts.contains(&tag.span.start),
                        stack
                            .last()
                            .is_some_and(|frame| matches!(frame.kind, FrameKind::Async(_))),
                    )? {
                        edits.push(Edit {
                            range: tag.span.start..tag.span.end,
                            replacement,
                        });
                        node += 1;
                    }
                }
                for frame in stack.iter_mut().rev() {
                    let FrameKind::Async(region) = &mut frame.kind else {
                        break;
                    };
                    if void_element(&name) {
                        return Err(error(
                            source,
                            tag.span.start,
                            "Async and Await need a non-void native root",
                        ));
                    }
                    region.root = match region.root {
                        AsyncRoot::Missing => AsyncRoot::One(element_id),
                        AsyncRoot::One(_) | AsyncRoot::Multiple => AsyncRoot::Multiple,
                    };
                }
                if !void_element(&name) && !tag.self_closing {
                    let text_host = owner
                        .filter(|index| {
                            !inert
                                && !rust_script
                                && !name.contains('-')
                                && !matches!(
                                    name.as_str(),
                                    "template"
                                        | "pre"
                                        | "listing"
                                        | "script"
                                        | "style"
                                        | "textarea"
                                        | "title"
                                        | "xmp"
                                        | "iframe"
                                        | "noembed"
                                        | "noframes"
                                        | "noscript"
                                        | "plaintext"
                                )
                                && !foreach_hosts.contains(&tag.span.start)
                                && !tag.attributes.contains_key(b"rust:slot".as_slice())
                                && !tag.attributes.contains_key(b"bind:field".as_slice())
                                && !tag.attributes.contains_key(b"is".as_slice())
                                && !components[*index].elements.last().is_some_and(|element| {
                                    element.id == element_id
                                        && element.children == ChildPolicy::Managed
                                })
                        })
                        .map(|index| TextHost {
                            opening: tag.span.start..tag.span.end,
                            opening_edit: edits
                                .last()
                                .filter(|edit| edit.range == (tag.span.start..tag.span.end))
                                .map(|_| edits.len() - 1),
                            // An unbound frame's node is only the next available
                            // ID. Record an actual descriptor before its children
                            // can consume that ID.
                            element: components[index]
                                .elements
                                .last()
                                .filter(|element| element.id == element_id)
                                .map(|element| element.id),
                        });
                    stack.push(Frame {
                        name,
                        owner,
                        node: element_id,
                        kind: FrameKind::Element(ElementFrame {
                            text_host,
                            inert,
                            owns_children: tag.attributes.contains_key(b"rust:slot".as_slice()),
                            field_value: tag.attributes.contains_key(b"bind:field".as_slice()),
                            component_root: component_id.is_some(),
                            region,
                        }),
                    });
                }
            }
            Token::EndTag(tag) => {
                let name = String::from_utf8_lossy(&tag.name);
                if let Some(index) = stack.iter().rposition(|frame| frame.name == name) {
                    if stack.last().is_some_and(|frame| frame.owner.is_some())
                        && index + 1 != stack.len()
                    {
                        return Err(error(
                            source,
                            tag.span.start,
                            "component HTML requires explicitly nested closing tags",
                        ));
                    }
                    stack.truncate(index + 1);
                    let frame = stack.pop().expect("matched closing tag");
                    // Spelling and close edits belong to the selected role. Native
                    // HTML keeps its existing case-insensitive closing behavior.
                    let closing = match &frame.kind {
                        FrameKind::Element(_) => None,
                        FrameKind::Control(control) => Some((
                            control.spelling(),
                            "control-flow closing tags must match their spelling",
                            "",
                        )),
                        FrameKind::Async(region) => Some((
                            match region.declaration {
                                super::async_tags::Declaration::Async { .. } => "Async",
                                super::async_tags::Declaration::Await { .. } => "Await",
                            },
                            "Async and Await require matching closing tags and exactly one native HTML root",
                            "",
                        )),
                        FrameKind::Router { .. } => Some((
                            "Router",
                            "Router and Route closing tags must match their spelling",
                            "",
                        )),
                        FrameKind::Route { .. } => Some((
                            "Route",
                            "Router and Route closing tags must match their spelling",
                            "",
                        )),
                        FrameKind::Invocation(invocation) => Some((
                            invocation.authored.as_str(),
                            "component closing tags must match the Rust type path's case",
                            "",
                        )),
                        FrameKind::Hydrated { authored } => Some((
                            authored.as_str(),
                            "hydrated component closing tags must match their Rust spelling",
                            "</div>",
                        )),
                        FrameKind::App => {
                            Some(("App", "App closing tag must match its spelling", ""))
                        }
                        FrameKind::ForEach => Some((
                            "ForEach",
                            "ForEach closing tag must match its spelling",
                            "</template>",
                        )),
                        FrameKind::Children => Some((
                            "Children",
                            "Children closing tag must match its spelling",
                            "",
                        )),
                    };
                    if let Some((expected, message, replacement)) = closing {
                        if super::tags::name(source, tag.span.start) != expected {
                            return Err(error(source, tag.span.start, message));
                        }
                        edits.push(Edit {
                            range: tag.span.start..tag.span.end,
                            replacement: replacement.into(),
                        });
                    }
                    match frame.kind {
                        FrameKind::Control(control) => match control {
                            ControlFrame::If {
                                branch,
                                phase: IfPhase::Then,
                            } => {
                                components[frame.owner.unwrap()].range.end = tag.span.start;
                                let body = super::control::body(
                                    source,
                                    &mut components,
                                    branch.owner,
                                    first_component,
                                    tag.span.start,
                                    Vec::new(),
                                    Vec::new(),
                                )?;
                                let Binding::Branch { cases, .. } =
                                    &mut components[branch.owner].bindings[branch.binding]
                                else {
                                    unreachable!()
                                };
                                cases.push(CaseBranch {
                                    pattern: Rust::parse(source, "false", tag.span.start)?,
                                    names: Vec::new(),
                                    body,
                                });
                            }
                            ControlFrame::Match { branch } => {
                                let Binding::Branch { cases, .. } =
                                    &components[branch.owner].bindings[branch.binding]
                                else {
                                    unreachable!()
                                };
                                if cases.is_empty() {
                                    return Err(error(
                                        source,
                                        tag.span.start,
                                        "Match requires at least one Case",
                                    ));
                                }
                            }
                            ControlFrame::Else | ControlFrame::Case { .. } => {
                                components[frame.owner.unwrap()].range.end = tag.span.start
                            }
                            ControlFrame::If {
                                phase: IfPhase::Else,
                                ..
                            } => {}
                        },
                        FrameKind::Element(element) => {
                            if let Some(host) = element.text_host {
                                if let Some((text_id, text_edit)) =
                                    text_edits.remove(&(host.opening.end, tag.span.start))
                                {
                                    let component =
                                        &mut components[frame.owner.expect("native text host")];
                                    let marker = format!(
                                        " {}=\"{text_id}\"",
                                        template::TEXT_ELEMENT_ATTRIBUTE
                                    );
                                    let opening = if let Some(index) = host.opening_edit {
                                        &mut edits[index].replacement
                                    } else {
                                        edits.push(Edit {
                                            replacement: source[host.opening.clone()].to_owned(),
                                            range: host.opening,
                                        });
                                        &mut edits.last_mut().unwrap().replacement
                                    };
                                    opening.insert_str(opening.len() - 1, &marker);
                                    edits[text_edit].replacement.clear();
                                    // Exact authored contents exclude whitespace,
                                    // comments and siblings, so this is the last text
                                    // slot collected for this component.
                                    let last = component.texts.pop();
                                    debug_assert_eq!(last, Some(text_id));
                                    component.text_elements.push(TextElement {
                                        id: text_id,
                                        host: host.element,
                                        tag: frame.name.clone(),
                                    });
                                }
                            }

                            if element.component_root {
                                components[frame.owner.expect("component root")].range.end =
                                    tag.span.end;
                            }
                            if let Some(region) = element.region {
                                let component =
                                    &mut components[frame.owner.expect("region component")];
                                let bindings = component.bindings.split_off(region.start);
                                let (value, await_value) = match region.directive {
                                    RegionDirective::Async(value) => (value, false),
                                    RegionDirective::Await(value) => (value, true),
                                };
                                component.bindings.push(Binding::Region {
                                    node: region.node,
                                    value,
                                    await_value,
                                    alias: None,
                                    bindings,
                                });
                            }
                        }
                        FrameKind::Async(region) => {
                            let AsyncRoot::One(node) = region.root else {
                                return Err(error(
                                    source,
                                    tag.span.start,
                                    "Async and Await require matching closing tags and exactly one native HTML root",
                                ));
                            };
                            let (value, alias) = match region.declaration {
                                super::async_tags::Declaration::Async { value } => (value, None),
                                super::async_tags::Declaration::Await { value, alias } => {
                                    (value, Some(alias))
                                }
                            };
                            let component = &mut components[frame.owner.expect("async owner")];
                            let bindings = component.bindings.split_off(region.start);
                            component.bindings.push(Binding::Region {
                                node,
                                value,
                                await_value: alias.is_some(),
                                alias,
                                bindings,
                            });
                        }
                        FrameKind::Invocation(_) => {
                            let owner = frame.owner.expect("children fragment");
                            let fragment = &mut components[owner];
                            fragment.range.end = if named_content.contains(&owner) {
                                fragment.range.start
                            } else {
                                tag.span.start
                            };
                        }
                        FrameKind::Route { .. } => {
                            components[frame.owner.expect("route body")].range.end = tag.span.start
                        }
                        FrameKind::App | FrameKind::ForEach => {
                            components[frame.owner.expect("component root")].range.end =
                                tag.span.end
                        }
                        FrameKind::Router { .. }
                        | FrameKind::Hydrated { .. }
                        | FrameKind::Children => {}
                    }
                }
            }
            Token::String(text) => {
                let Some(frame) = stack.last() else {
                    continue;
                };
                let Some(index) = frame.owner else {
                    continue;
                };
                if matches!(
                    frame.kind,
                    FrameKind::Control(
                        ControlFrame::Match { .. }
                            | ControlFrame::If {
                                phase: IfPhase::Else,
                                ..
                            }
                    )
                ) && !String::from_utf8_lossy(&text).trim().is_empty()
                {
                    return Err(error(
                        source,
                        text.span.start,
                        "Match accepts only Case children; Else must be last in If",
                    ));
                }
                if matches!(frame.kind, FrameKind::Router { .. })
                    && !String::from_utf8_lossy(&text).trim().is_empty()
                {
                    return Err(error(
                        source,
                        text.span.start,
                        "Router accepts Route children, not text",
                    ));
                }
                if matches!(frame.kind, FrameKind::Invocation(_))
                    && named_content.contains(&index)
                    && !String::from_utf8_lossy(&text).trim().is_empty()
                {
                    return Err(error(
                        source,
                        text.span.start,
                        "do not mix named content and ordinary children in one invocation",
                    ));
                }
                if matches!(
                    frame.kind,
                    FrameKind::Element(ElementFrame {
                        field_value: true,
                        ..
                    })
                ) && !String::from_utf8_lossy(&text).trim().is_empty()
                {
                    return Err(error(
                        source,
                        text.span.start,
                        "bind:field owns the textarea value; leave its contents empty and initialize the field in Rust",
                    ));
                }
                if frame.owns_children() && !String::from_utf8_lossy(&text).trim().is_empty() {
                    return Err(error(
                        source,
                        text.span.start,
                        "an owned host or hydrated component must be empty; its registered renderer supplies its contents",
                    ));
                }
                if matches!(frame.name.as_str(), "script" | "style") {
                    continue;
                }
                let raw = &source[text.span.start..text.span.end];
                if (matches!(frame.kind, FrameKind::Async(_)) || frame.requires_native_root())
                    && !String::from_utf8_lossy(&text).trim().is_empty()
                {
                    return Err(error(
                        source,
                        text.span.start,
                        "a component template must put its text inside its single root element",
                    ));
                }
                if !raw.contains("{{") {
                    continue;
                }
                if frame.inert()
                    || matches!(
                        frame.name.as_str(),
                        "textarea"
                            | "title"
                            | "xmp"
                            | "iframe"
                            | "noembed"
                            | "noframes"
                            | "plaintext"
                    )
                {
                    return Err(error(
                        source,
                        text.span.start,
                        "text interpolation requires rendered HTML text; raw-text elements cannot contain bindings",
                    ));
                }
                for part in interpolations(source, raw, text.span.start, true)? {
                    let offset = text.span.start + part.range.start;
                    let text_id = TextId::new(slot);
                    text_edits.insert(
                        (offset, text.span.start + part.range.end),
                        (text_id, edits.len()),
                    );
                    edits.push(Edit {
                        range: offset..text.span.start + part.range.end,
                        replacement: format!(
                            "<!--{}--><!--{}-->",
                            TextMarker::Start(text_id),
                            TextMarker::End(text_id)
                        ),
                    });
                    components[index].bindings.push(Binding::Text {
                        slot: text_id,
                        value: Rust {
                            tokens: part.tokens,
                            offset,
                        },
                    });
                    components[index].texts.push(text_id);
                    slot += 1;
                }
            }
            Token::Comment(comment)
                if template::reserved_comment(&String::from_utf8_lossy(&comment)) =>
            {
                return Err(error(
                    source,
                    comment.span.start,
                    "fusor: comment markers are reserved for the HTML compiler",
                ));
            }
            _ => {}
        }
    }
    if pending_module.is_some() {
        return Err(error(
            source,
            source.len(),
            "component module is missing its closing </script> tag",
        ));
    }
    if stack.iter().any(|frame| frame.owner.is_some()) {
        return Err(error(
            source,
            source.len(),
            "component HTML is missing a closing tag",
        ));
    }
    validate::components(source, &components, &template_roots)?;
    let content_ranges: Vec<_> = components
        .iter()
        .filter(|component| component.capture.is_some() || component.inline)
        .map(|component| component.range.clone())
        .filter(|range| !range.is_empty())
        .collect();
    materialize::components(source, blocks, &edits, &content_ranges, &mut components);
    // Row analysis consumes authored bindings. Scope rewriting inserts lexical
    // aliases, so it must run only after the item-only proof has been recorded.
    super::foreach::mark_item_only_rows(&mut components);
    for component in &mut components {
        super::lexical::rewrite(component);
    }
    let templates = components
        .iter()
        .filter(|component| (component.capture.is_some() || component.inline) && !component.empty)
        .map(|component| component.html.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    materialize::remove_captured_edits(&mut edits, &content_ranges);
    Ok(Plan {
        edits,
        components,
        templates,
    })
}

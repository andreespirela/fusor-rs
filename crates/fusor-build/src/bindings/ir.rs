//! Private compiler contract. Rust token trees retain their native spans.

use super::tokens::Rust;
use fusor::template::{ChildPolicy, ComponentId, ElementId, MountId, RootKind, TextId};
use proc_macro2::Span;
use std::ops::Range;

pub(crate) struct Edit {
    pub range: Range<usize>,
    pub replacement: String,
}

pub(super) struct Plan {
    pub edits: Vec<Edit>,
    pub components: Vec<Component>,
    pub templates: String,
}

pub(super) struct Component {
    pub id: ComponentId,
    pub ty: Rust,
    pub shape: ComponentShape,
    pub empty: bool,
    pub locals: Vec<(Rust, Rust)>,
    // Proven before lexical aliases are inserted; only direct forwarding rows.
    pub item_only_row: bool,
    pub async_locals: Vec<Rust>,
    pub route_locals: Vec<Rust>,
    pub snapshot_locals: Vec<Rust>,
    pub elements: Vec<Element>,
    pub texts: Vec<TextId>,
    pub text_elements: Vec<TextElement>,
    pub bindings: Vec<Binding>,
    pub render: RenderTarget,
    pub range: Range<usize>,
    pub html: String,
    pub javascript: Option<crate::JavaScriptModule>,
}

/// What kind of component this is, which decides how it mounts and what it captures.
pub(super) enum ComponentShape {
    /// A `rust:component` declaration, mounted from a template or an existing root.
    Declared(RootKind),
    /// The `App` boundary, built by this state expression.
    App(Rust),
    /// A ForEach row, rendered inline by its list.
    Row,
    /// Children of a component tag, a Branch case or a Route body; captures its caller's state.
    Fragment(Rust),
    /// A `rust:content` template passed to a component; captures its caller's state.
    Content(Rust),
}

impl Component {
    pub fn new(
        id: ComponentId,
        ty: Rust,
        shape: ComponentShape,
        render: RenderTarget,
        range: Range<usize>,
    ) -> Self {
        Self {
            id,
            ty,
            shape,
            empty: false,
            locals: Vec::new(),
            item_only_row: false,
            async_locals: Vec::new(),
            route_locals: Vec::new(),
            snapshot_locals: Vec::new(),
            elements: Vec::new(),
            texts: Vec::new(),
            text_elements: Vec::new(),
            bindings: Vec::new(),
            render,
            range,
            html: String::new(),
            javascript: None,
        }
    }

    /// The state a fragment or content component borrows from its caller.
    pub fn capture(&self) -> Option<&Rust> {
        match &self.shape {
            ComponentShape::Fragment(capture) | ComponentShape::Content(capture) => Some(capture),
            ComponentShape::Declared(_) | ComponentShape::App(_) | ComponentShape::Row => None,
        }
    }

    pub fn inline(&self) -> bool {
        matches!(self.shape, ComponentShape::Row)
    }

    pub fn fragment(&self) -> bool {
        matches!(self.shape, ComponentShape::Fragment(_))
    }

    pub fn kind(&self) -> RootKind {
        match self.shape {
            ComponentShape::Declared(kind) => kind,
            ComponentShape::App(_) => RootKind::Existing,
            ComponentShape::Row | ComponentShape::Fragment(_) | ComponentShape::Content(_) => {
                RootKind::Template
            }
        }
    }

    pub fn app(&self) -> Option<&Rust> {
        match &self.shape {
            ComponentShape::App(state) => Some(state),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum RenderTarget {
    Browser,
    Server,
    Shared,
}

pub(super) struct Element {
    pub id: ElementId,
    pub tag: String,
    pub children: ChildPolicy,
}

pub(super) struct TextElement {
    pub id: TextId,
    pub host: Option<ElementId>,
    pub tag: String,
}

pub(super) enum StringPart {
    Literal(String),
    Expression(Rust),
}

pub(super) struct InterpolatedString(pub Vec<StringPart>);

pub(super) enum InputKind {
    Value,
    Checked,
}

pub(super) struct Input {
    pub name: Rust,
    pub value: InputValue,
}

pub(super) enum InputValue {
    /// `name="{{ expression }}"`.
    Expression(Rust),
    /// `name="text"`: a string literal token.
    Literal(Rust),
    /// A `rust:content` template passed as this input.
    Content { component: usize, origin: Rust },
}

impl InputValue {
    /// The Rust this input evaluates, unless it is projected content.
    pub fn value(&self) -> Option<&Rust> {
        match self {
            Self::Expression(value) | Self::Literal(value) => Some(value),
            Self::Content { .. } => None,
        }
    }
}

/// When an island's code starts, from `hydrate="…"`.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Activation {
    Load,
    Visible,
    Idle,
    Interaction,
    Manual,
}

impl Activation {
    pub fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "load" => Self::Load,
            "visible" => Self::Visible,
            "idle" => Self::Idle,
            "interaction" => Self::Interaction,
            "manual" => Self::Manual,
            _ => return None,
        })
    }

    /// The matching `fusor_islands::Activation` variant.
    pub fn variant(self) -> &'static str {
        match self {
            Self::Load => "Load",
            Self::Visible => "Visible",
            Self::Idle => "Idle",
            Self::Interaction => "Interaction",
            Self::Manual => "Manual",
        }
    }
}

/// When an island's code downloads before activation, from `hydrate:prefetch="…"`.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Prefetch {
    None,
    Load,
    Visible,
    Idle,
}

impl Prefetch {
    pub fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "none" => Self::None,
            "load" => Self::Load,
            "visible" => Self::Visible,
            "idle" => Self::Idle,
            _ => return None,
        })
    }

    /// The matching `fusor_islands::Prefetch` variant.
    pub fn variant(self) -> &'static str {
        match self {
            Self::None => "None",
            Self::Load => "Load",
            Self::Visible => "Visible",
            Self::Idle => "Idle",
        }
    }
}

/// What a coherent region does with its value.
pub(super) enum RegionKind {
    /// `Async` or `rust:async`: the value is the region's async boundary.
    Boundary,
    /// `Await` or `rust:await`: the value is read; `Await` names the result.
    Await { alias: Option<Rust> },
}

impl RegionKind {
    pub fn alias(&self) -> Option<&Rust> {
        match self {
            Self::Boundary => None,
            Self::Await { alias } => alias.as_ref(),
        }
    }
}

pub(super) struct RouteBranch {
    pub path: Option<String>,
    pub params: Option<Rust>,
    pub names: Vec<Rust>,
    pub body: usize,
}

pub(super) struct CaseBranch {
    pub pattern: Rust,
    pub names: Vec<Rust>,
    pub body: usize,
}

pub(super) enum Binding {
    Branch {
        point: MountId,
        value: Rust,
        cases: Vec<CaseBranch>,
        snapshots: Vec<(Rust, Rust)>,
    },
    Router {
        point: MountId,
        origin: Rust,
        routes: Vec<RouteBranch>,
    },
    ForEach {
        node: ElementId,
        items: Rust,
        key: Rust,
        body: usize,
    },
    Children {
        point: MountId,
        origin: Rust,
    },
    Invocation {
        point: MountId,
        ty: Rust,
        inputs: Vec<Input>,
        children: Option<usize>,
        condition: Option<Rust>,
        key: Option<Rust>,
    },
    Island {
        node: ElementId,
        descriptor: Rust,
        /// The component's inputs, which become its serialized props.
        inputs: Vec<Input>,
        activation: Activation,
        prefetch: Prefetch,
    },
    Region {
        node: ElementId,
        value: Rust,
        kind: RegionKind,
        bindings: Vec<Binding>,
    },
    Text {
        slot: TextId,
        value: Rust,
    },
    Attribute {
        node: ElementId,
        name: String,
        value: InterpolatedString,
    },
    Property {
        node: ElementId,
        name: String,
        value: Rust,
    },
    Boolean {
        node: ElementId,
        name: String,
        value: Rust,
    },
    Value {
        node: ElementId,
        value: InterpolatedString,
    },
    Checked {
        node: ElementId,
        value: Rust,
    },
    Class {
        node: ElementId,
        name: String,
        value: Rust,
    },
    Event {
        node: ElementId,
        name: String,
        handler: Rust,
    },
    Input {
        node: ElementId,
        kind: InputKind,
        value: Rust,
    },
    Field {
        node: ElementId,
        value: Rust,
    },
    Slot {
        node: ElementId,
        content: Rust,
        condition: Option<Rust>,
        key: Option<Rust>,
    },
}

impl Binding {
    /// The fragment that locates this binding in the HTML; code generated for the
    /// binding is spanned to it. It is always the first of `fragments()`.
    pub fn origin(&self) -> &Rust {
        match self {
            Self::Branch { value, .. }
            | Self::Region { value, .. }
            | Self::Text { value, .. }
            | Self::Property { value, .. }
            | Self::Boolean { value, .. }
            | Self::Checked { value, .. }
            | Self::Class { value, .. }
            | Self::Input { value, .. }
            | Self::Field { value, .. } => value,
            Self::Children { origin, .. } | Self::Router { origin, .. } => origin,
            Self::ForEach { items, .. } => items,
            Self::Invocation { ty, .. } => ty,
            Self::Island { descriptor, .. } => descriptor,
            Self::Event { handler, .. } => handler,
            Self::Slot { content, .. } => content,
            Self::Attribute { value, .. } | Self::Value { value, .. } => value
                .0
                .iter()
                .find_map(|part| match part {
                    StringPart::Expression(expression) => Some(expression),
                    StringPart::Literal(_) => None,
                })
                .expect("an interpolated attribute has an expression"),
        }
    }

    pub fn span(&self) -> Span {
        self.origin().span()
    }

    pub fn fragments(&self) -> Vec<&Rust> {
        match self {
            Self::Branch { value, cases, .. } => std::iter::once(value)
                .chain(cases.iter().map(|case| &case.pattern))
                .collect(),
            Self::Children { origin, .. } | Self::Router { origin, .. } => vec![origin],
            Self::ForEach { items, key, .. } => vec![items, key],
            Self::Invocation {
                ty,
                inputs,
                condition,
                key,
                ..
            } => std::iter::once(ty)
                .chain(inputs.iter().flat_map(|input| {
                    let (InputValue::Expression(value)
                    | InputValue::Literal(value)
                    | InputValue::Content { origin: value, .. }) = &input.value;
                    [&input.name, value]
                }))
                .chain(condition)
                .chain(key)
                .collect(),
            Self::Island {
                descriptor, inputs, ..
            } => std::iter::once(descriptor)
                .chain(
                    inputs
                        .iter()
                        .flat_map(|input| [&input.name].into_iter().chain(input.value.value())),
                )
                .collect(),
            Self::Region {
                value,
                kind,
                bindings,
                ..
            } => std::iter::once(value)
                .chain(kind.alias())
                .chain(bindings.iter().flat_map(Self::fragments))
                .collect(),
            Self::Text { value, .. }
            | Self::Boolean { value, .. }
            | Self::Checked { value, .. }
            | Self::Class { value, .. }
            | Self::Field { value, .. }
            | Self::Input { value, .. } => vec![value],
            Self::Event { handler, .. } => vec![handler],
            Self::Slot {
                content: constructor,
                condition,
                key,
                ..
            } => std::iter::once(constructor)
                .chain(condition)
                .chain(key)
                .collect(),
            Self::Property { value, .. } => vec![value],
            Self::Attribute { value, .. } | Self::Value { value, .. } => value
                .0
                .iter()
                .filter_map(|part| match part {
                    StringPart::Expression(expression) => Some(expression),
                    _ => None,
                })
                .collect(),
        }
    }
}

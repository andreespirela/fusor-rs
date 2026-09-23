//! Private compiler contract. Rust token trees retain their native spans.

use super::tokens::Rust;
use fusor::template::{ChildPolicy, ComponentId, ElementId, MountId, RootKind, TextId};
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
    pub capture: Option<Rust>,
    pub inline: bool,
    pub fragment: bool,
    pub empty: bool,
    pub locals: Vec<(Rust, Rust)>,
    // Proven before lexical aliases are inserted; only direct forwarding rows.
    pub item_only_row: bool,
    pub async_locals: Vec<Rust>,
    pub route_locals: Vec<Rust>,
    pub snapshot_locals: Vec<Rust>,
    pub kind: RootKind,
    pub app: Option<Rust>,
    pub elements: Vec<Element>,
    pub texts: Vec<TextId>,
    pub text_elements: Vec<TextElement>,
    pub bindings: Vec<Binding>,
    pub render: RenderTarget,
    pub range: Range<usize>,
    pub html: String,
    pub javascript: Option<crate::JavaScriptModule>,
}

// Construction choices only: lowering still consumes the existing Component IR.
pub(super) enum ComponentShape {
    Declared(RootKind),
    App(Rust),
    Row,
    Fragment(Rust),
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
        let (capture, inline, fragment, kind, app) = match shape {
            ComponentShape::Declared(kind) => (None, false, false, kind, None),
            ComponentShape::App(state) => (None, false, false, RootKind::Existing, Some(state)),
            ComponentShape::Row => (None, true, false, RootKind::Template, None),
            ComponentShape::Fragment(capture) => {
                (Some(capture), false, true, RootKind::Template, None)
            }
            ComponentShape::Content(capture) => {
                (Some(capture), false, false, RootKind::Template, None)
            }
        };
        Self {
            id,
            ty,
            capture,
            inline,
            fragment,
            empty: false,
            locals: Vec::new(),
            item_only_row: false,
            async_locals: Vec::new(),
            route_locals: Vec::new(),
            snapshot_locals: Vec::new(),
            kind,
            app,
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
    Expression(Rust),
    Content { component: usize, origin: Rust },
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
        props: Rust,
        activation: String,
        prefetch: String,
    },
    Region {
        node: ElementId,
        value: Rust,
        await_value: bool,
        alias: Option<Rust>,
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
            } => {
                std::iter::once(ty)
                    .chain(inputs.iter().flat_map(|input| {
                        let (InputValue::Expression(value)
                        | InputValue::Content { origin: value, .. }) = &input.value;
                        [&input.name, value]
                    }))
                    .chain(condition)
                    .chain(key)
                    .collect()
            }
            Self::Island {
                descriptor, props, ..
            } => vec![descriptor, props],
            Self::Region {
                value,
                alias,
                bindings,
                ..
            } => std::iter::once(value)
                .chain(alias)
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

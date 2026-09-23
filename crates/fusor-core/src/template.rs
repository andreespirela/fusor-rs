//! The serialized HTML boundary. Compiler and runtime use these same types.
//!
//! Marker strings exist only at this boundary. Binding operations use typed IDs
//! during compilation and native DOM handles after a template has been resolved.

use std::{fmt, str::FromStr};

pub const VERSION: u32 = 2;
pub const VERSION_ATTRIBUTE: &str = "data-rf-version";
pub const COMPONENT_ATTRIBUTE: &str = "data-rf-component";
pub const ELEMENT_ATTRIBUTE: &str = "data-rf-node";
/// An element whose only child is a compiler-owned dynamic Text node.
pub const TEXT_ELEMENT_ATTRIBUTE: &str = "data-rf-text";
/// Child regions controlled by components, lists, slots, or external widgets.
pub const MANAGED_ATTRIBUTE: &str = "data-rf-managed";
/// A subtree whose contents belong to an external integration.
pub const EXTERNAL_ATTRIBUTE: &str = "data-rf-external";
/// Runtime component roots, including clones of a declared HTML template.
pub const INSTANCE_ATTRIBUTE: &str = "data-rf-instance";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InvalidId;

impl fmt::Display for InvalidId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("expected a canonical unsigned template identifier")
    }
}

impl std::error::Error for InvalidId {}

macro_rules! identifier {
    ($name:ident) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
        pub struct $name(usize);

        impl $name {
            pub const fn new(index: usize) -> Self {
                Self(index)
            }

            pub const fn index(self) -> usize {
                self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                self.0.fmt(f)
            }
        }

        impl FromStr for $name {
            type Err = InvalidId;

            fn from_str(value: &str) -> Result<Self, Self::Err> {
                if value.is_empty()
                    || (value.len() > 1 && value.starts_with('0'))
                    || !value.bytes().all(|byte| byte.is_ascii_digit())
                {
                    return Err(InvalidId);
                }
                value.parse().map(Self).map_err(|_| InvalidId)
            }
        }
    };
}

identifier!(ComponentId);
identifier!(ElementId);
identifier!(TextId);
identifier!(MountId);

/// A managed sibling region. Its native component root is inserted between
/// these comments, without introducing an HTML host element.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MountMarker {
    Start(MountId),
    End(MountId),
}

impl MountMarker {
    pub fn parse(value: &str) -> Result<Option<Self>, InvalidId> {
        if let Some(id) = value.strip_prefix("rf:mount:") {
            id.parse().map(|id| Some(Self::Start(id)))
        } else if let Some(id) = value.strip_prefix("/rf:mount:") {
            id.parse().map(|id| Some(Self::End(id)))
        } else {
            Ok(None)
        }
    }
}

impl fmt::Display for MountMarker {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Start(id) => write!(f, "rf:mount:{id}"),
            Self::End(id) => write!(f, "/rf:mount:{id}"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextMarker {
    Start(TextId),
    End(TextId),
}

impl TextMarker {
    /// Ordinary HTML comments are not part of this protocol.
    pub fn parse(value: &str) -> Result<Option<Self>, InvalidId> {
        if let Some(id) = value.strip_prefix("rf:") {
            id.parse().map(|id| Some(Self::Start(id)))
        } else if let Some(id) = value.strip_prefix("/rf:") {
            id.parse().map(|id| Some(Self::End(id)))
        } else {
            Ok(None)
        }
    }
}

impl fmt::Display for TextMarker {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Start(id) => write!(f, "rf:{id}"),
            Self::End(id) => write!(f, "/rf:{id}"),
        }
    }
}

pub fn reserved_attribute(name: &str) -> bool {
    name.starts_with("data-rf-")
}

pub fn reserved_comment(value: &str) -> bool {
    value.starts_with("rf:") || value.starts_with("/rf:")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RootKind {
    Existing,
    Template,
}

#[derive(Debug, Clone, Copy)]
pub struct ElementDescriptor {
    pub id: ElementId,
    pub tag: &'static str,
    pub children: ChildPolicy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChildPolicy {
    Static,
    /// A list, component, slot, or attachment owns this element's child region.
    Managed,
}

/// A dynamic Text node occupying the complete contents of one native element.
#[derive(Debug, Clone, Copy)]
pub struct TextElementDescriptor {
    pub id: TextId,
    /// Present only when other bindings also need this element's handle.
    pub host: Option<ElementId>,
    pub tag: &'static str,
}

/// A compile-time description of every DOM handle a component needs.
/// The runtime validates the entire description before installing any behavior.
#[derive(Debug)]
pub struct TemplateDescriptor {
    pub version: u32,
    pub component: ComponentId,
    pub kind: RootKind,
    pub elements: &'static [ElementDescriptor],
    pub texts: &'static [TextId],
    /// Exact sole-child text bindings, associated with a validated native host.
    pub text_elements: &'static [TextElementDescriptor],
}

/// Shared HTML escaping for runtime values and compiler-folded literals.
pub fn escape_into(output: &mut String, value: &str, attribute: bool) {
    for ch in value.chars() {
        match ch {
            '&' => output.push_str("&amp;"),
            '<' => output.push_str("&lt;"),
            '>' => output.push_str("&gt;"),
            '"' if attribute => output.push_str("&quot;"),
            '\'' if attribute => output.push_str("&#39;"),
            ch => output.push(ch),
        }
    }
}

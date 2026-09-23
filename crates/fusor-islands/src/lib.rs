//! Portable descriptors contain wire types and identity, never browser state or
//! component factories. Browser entries live in independently built packages.
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::collections::BTreeMap;

/// Browser registry shipped with this protocol release.
pub const REGISTRY_JAVASCRIPT: &str = include_str!("../runtime/registry.js");
/// Browser composition runtime shipped with this protocol release.
pub const COMPOSITION_JAVASCRIPT: &str = include_str!("../runtime/composition.js");

pub const PROTOCOL_VERSION: u32 = 1;

/// Implement in a small shared wire-types crate. Changing a wire contract needs
/// a new schema identifier; derives do not prove cross-target compatibility.
pub trait Island: 'static {
    type Props: Serialize + DeserializeOwned;
    const NAME: &'static str;
    const UNIT: &'static str;
    const SCHEMA: &'static str;
    const MODE: RenderMode = RenderMode::Attach;
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RenderMode {
    #[default]
    Attach,
    Preview,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Activation {
    #[default]
    Load,
    Visible,
    Idle,
    Interaction,
    Manual,
}
impl Activation {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Load => "load",
            Self::Visible => "visible",
            Self::Idle => "idle",
            Self::Interaction => "interaction",
            Self::Manual => "manual",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Prefetch {
    #[default]
    None,
    Load,
    Visible,
    Idle,
}
impl Prefetch {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Load => "load",
            Self::Visible => "visible",
            Self::Idle => "idle",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Entry {
    pub unit: String,
    pub descriptor: String,
    pub props_schema: String,
    pub template_hash: String,
    pub mode: RenderMode,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Unit {
    pub javascript: String,
    pub wasm: String,
    #[serde(default)]
    pub dependencies: Vec<String>,
    pub entries: Vec<Entry>,
}

/// Public delivery protocol. It is independent of the compiler's private Cargo
/// artifact manifest and the browser's mutable per-instance state.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeliveryManifest {
    pub version: u32,
    pub generation: String,
    pub units: BTreeMap<String, Unit>,
}
impl DeliveryManifest {
    pub fn validate(&self) -> Result<(), String> {
        if self.version != PROTOCOL_VERSION {
            return Err("unsupported island delivery protocol".into());
        }
        if self.generation.is_empty() {
            return Err("delivery generation cannot be empty".into());
        }
        let mut descriptors = std::collections::BTreeSet::new();
        for (name, unit) in &self.units {
            if name.is_empty() || unit.entries.is_empty() {
                return Err("delivery unit needs a name and entries".into());
            }
            for url in std::iter::once(&unit.javascript)
                .chain(std::iter::once(&unit.wasm))
                .chain(&unit.dependencies)
            {
                if !url.starts_with('/')
                    || url.starts_with("//")
                    || url.contains("..")
                    || url.contains(['#', '?', '\\'])
                {
                    return Err(
                        "delivery URLs must be immutable, same-origin absolute paths".into(),
                    );
                }
            }
            for entry in &unit.entries {
                if &entry.unit != name {
                    return Err("island entry belongs to a different delivery unit".into());
                }
                if entry.descriptor.is_empty()
                    || entry.props_schema.is_empty()
                    || entry.template_hash.is_empty()
                    || !descriptors.insert(&entry.descriptor)
                {
                    return Err("island descriptors must be unique and include schema and template identity".into());
                }
            }
        }
        Ok(())
    }
    pub fn entry<D: Island>(&self) -> Result<&Entry, String> {
        let entry = self
            .units
            .get(D::UNIT)
            .and_then(|unit| {
                unit.entries
                    .iter()
                    .find(|entry| entry.descriptor == D::NAME)
            })
            .ok_or_else(|| format!("unregistered island {} in unit {}", D::NAME, D::UNIT))?;
        if entry.props_schema != D::SCHEMA || entry.mode != D::MODE {
            return Err(format!("island descriptor/schema mismatch: {}", D::NAME));
        }
        Ok(entry)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UnitWitness {
    pub version: u32,
    pub entries: Vec<Entry>,
}

/// JSON stays opaque text across JavaScript. This preserves all Rust integer
/// values, including u64 values above JavaScript's Number precision.
pub fn encode<T: Serialize>(value: &T) -> Result<String, serde_json::Error> {
    serde_json::to_string(value)
}
pub fn decode<T: DeserializeOwned>(value: &str) -> Result<T, serde_json::Error> {
    serde_json::from_str(value)
}

#[cfg(feature = "browser")]
pub mod browser;

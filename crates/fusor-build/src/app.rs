//! Application configuration, generation and the versioned Cargo artifact contract.
//! Inline sources become `crate::ui` modules; external sources remain native modules.
mod artifact;
mod config;
mod error;
mod external;
mod generate;
mod includes;
mod validate;

pub use artifact::{
    ARTIFACT_VERSION, ArtifactManifest, JavaScriptArtifact, RegistrationArtifact, SourceArtifact,
};
use config::SourceKind;
pub use config::{AppConfig, DeliveryConfig, DeliveryMode, DeliveryUnit, valid_module_name};
pub use generate::generate;
pub use includes::includes_foreign_file;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

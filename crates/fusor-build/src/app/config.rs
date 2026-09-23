use super::Result;
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Component, Path, PathBuf},
};

/// `[package.metadata.fusor]` in the application's Cargo manifest.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub struct AppConfig {
    pub delivery: Option<DeliveryConfig>,
    #[serde(default = "entry")]
    pub entry: PathBuf,
    pub assets: Option<PathBuf>,
    /// Optional executable and arguments, run in the package before publishing
    /// assets. No shell expansion or implicit dependency installation.
    #[serde(default)]
    pub assets_build: Vec<String>,
    /// Disable compatible refresh when custom build logic reads HTML/assets.
    #[serde(default = "dev_refresh")]
    pub dev_refresh: bool,
    #[serde(default = "output")]
    pub output: PathBuf,
    #[serde(default = "base_path")]
    pub base_path: String,
    /// Application-relative prefixes allowed to receive index.html on an HTML
    /// document request. Empty by default; static asset requests still return 404.
    #[serde(default)]
    pub history_fallback: Vec<String>,
    #[serde(default)]
    pub components: BTreeMap<String, PathBuf>,
    /// Package-relative directories searched recursively for reusable HTML.
    /// An empty list disables discovery; explicit registrations remain supported.
    #[serde(default = "templates")]
    pub templates: Vec<PathBuf>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub struct DeliveryConfig {
    pub mode: DeliveryMode,
    pub binary: Option<String>,
    pub units: BTreeMap<String, DeliveryUnit>,
}
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum DeliveryMode {
    Islands,
}
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub struct DeliveryUnit {
    pub package: String,
    #[serde(default)]
    pub features: Vec<String>,
}

fn entry() -> PathBuf {
    "web/index.html".into()
}
fn templates() -> Vec<PathBuf> {
    vec!["web/components".into()]
}
fn output() -> PathBuf {
    "dist".into()
}
fn base_path() -> String {
    "/".into()
}
fn dev_refresh() -> bool {
    true
}

impl AppConfig {
    pub fn load(manifest: &Path) -> Result<Self> {
        let document: toml::Value = fs::read_to_string(manifest)?.parse()?;
        let metadata = document
            .get("package")
            .and_then(|p| p.get("metadata"))
            .and_then(|m| m.get("fusor"))
            .ok_or("expected [package.metadata.fusor] in Cargo.toml")?;
        let config: Self = metadata.clone().try_into()?;
        config.validate()?;
        Ok(config)
    }

    pub fn validate(&self) -> Result<()> {
        if let Some(delivery) = &self.delivery {
            if delivery.units.is_empty() {
                return Err(
                    "islands delivery requires at least one independently built unit".into(),
                );
            }
            let mut packages = BTreeSet::new();
            for (name, unit) in &delivery.units {
                if name.is_empty()
                    || !name
                        .bytes()
                        .all(|byte| byte.is_ascii_alphanumeric() || b"-_".contains(&byte))
                    || unit.package.is_empty()
                    || !packages.insert(&unit.package)
                {
                    return Err("delivery unit names must be URL-safe and each unit must select a distinct Cargo package".into());
                }
            }
        }
        if self
            .assets_build
            .first()
            .is_some_and(|command| command.trim().is_empty())
        {
            return Err("assets-build must start with a nonempty executable".into());
        }
        if !self.assets_build.is_empty() && self.assets.is_none() {
            return Err("assets-build requires an assets output directory".into());
        }
        for path in std::iter::once(&self.entry)
            .chain(self.components.values())
            .chain(self.templates.iter())
            .chain(self.assets.iter())
            .chain(std::iter::once(&self.output))
        {
            if path.as_os_str().is_empty()
                || path
                    .components()
                    .any(|p| !matches!(p, Component::Normal(_)))
            {
                return Err(format!(
                    "application paths must be nonempty package-relative paths without '..': {}",
                    path.display()
                )
                .into());
            }
        }
        let reserved = [
            "src", "target", ".git", ".cargo", ".codex", ".agents", ".fusor",
        ];
        if reserved.iter().any(|name| self.output.starts_with(name)) {
            return Err(
                "output must be separate from Cargo source, target, and configuration directories"
                    .into(),
            );
        }
        for source in std::iter::once(&self.entry)
            .chain(self.components.values())
            .chain(self.templates.iter())
            .chain(self.assets.iter())
        {
            if source.starts_with(&self.output) || self.output.starts_with(source) {
                return Err("output cannot overlap an application source or assets path".into());
            }
        }
        if !self.base_path.starts_with('/')
            || !self.base_path.ends_with('/')
            || !self
                .base_path
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"/-_".contains(&b))
            || self.base_path.contains("//")
        {
            return Err("base-path must be '/' or a slash-delimited URL path such as '/tools/issues/' (letters, numbers, '-' and '_' only)".into());
        }
        for prefix in &self.history_fallback {
            if !prefix.starts_with('/')
                || prefix.contains("//")
                || !prefix
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"/-_".contains(&b))
                || prefix != "/" && prefix.ends_with('/')
            {
                return Err(
                    "history-fallback entries must be route prefixes such as '/issues' or '/'"
                        .into(),
                );
            }
        }
        for name in self.components.keys() {
            // Restrict module names to portable snake_case; rustc owns all types
            // and imports *inside* these modules.
            if name == "app" || !valid_module_name(name) {
                return Err(format!("invalid component module {name:?}; use a non-keyword snake_case Rust identifier other than 'app'").into());
            }
        }
        Ok(())
    }

    /// Discover a bounded source graph. Explicit registrations take precedence
    /// over discovery; an HTML file is compiled exactly once.
    pub fn discover_sources(&self, root: &Path) -> Result<Vec<(String, PathBuf)>> {
        let root = root.canonicalize()?;
        let mut seen = BTreeSet::new();
        let mut sources = Vec::new();
        for (name, path) in self.sources() {
            let canonical = root
                .join(path)
                .canonicalize()
                .map_err(|e| format!("{}: {e}", root.join(path).display()))?;
            if !canonical.starts_with(&root) || canonical.starts_with(root.join(&self.output)) {
                return Err(format!(
                    "HTML source must stay inside this package and outside its output: {}",
                    path.display()
                )
                .into());
            }
            if !seen.insert(canonical) {
                return Err(
                    format!("HTML source registered more than once: {}", path.display()).into(),
                );
            }
            sources.push((name.to_owned(), path.to_owned()));
        }
        fn visit(directory: &Path, files: &mut BTreeSet<PathBuf>) -> Result<()> {
            for entry in fs::read_dir(directory)? {
                let entry = entry?;
                let kind = entry.file_type()?;
                let path = entry.path();
                if kind.is_symlink() {
                    return Err(format!(
                        "template discovery does not follow symlinks: {}",
                        path.display()
                    )
                    .into());
                }
                if kind.is_dir() {
                    visit(&path, files)?;
                } else if kind.is_file()
                    && path
                        .extension()
                        .is_some_and(|extension| extension == "html")
                {
                    files.insert(path);
                }
            }
            Ok(())
        }
        let mut files = BTreeSet::new();
        for directory in &self.templates {
            let path = root.join(directory);
            match fs::symlink_metadata(&path) {
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                Err(error) => return Err(error.into()),
                Ok(metadata) if !metadata.is_dir() || metadata.file_type().is_symlink() => {
                    return Err(format!(
                        "template discovery requires a real directory: {}",
                        path.display()
                    )
                    .into());
                }
                Ok(_) => {}
            }
            // Check every ancestor, including a symlink above the discovery root.
            for ancestor in path.ancestors().take_while(|ancestor| *ancestor != root) {
                if fs::symlink_metadata(ancestor)?.file_type().is_symlink() {
                    return Err(format!(
                        "template discovery does not follow symlinks: {}",
                        ancestor.display()
                    )
                    .into());
                }
            }
            visit(&path, &mut files)?;
        }
        for path in files {
            if seen.insert(path.canonicalize()?) {
                let relative = path.strip_prefix(&root)?.to_owned();
                sources.push((
                    format!("@{}", relative.to_string_lossy().replace('\\', "/")),
                    relative,
                ));
            }
        }
        Ok(sources)
    }

    /// Entry first, then explicitly registered modules in deterministic order.
    pub fn sources(&self) -> impl Iterator<Item = (&str, &Path)> {
        std::iter::once(("app", self.entry.as_path())).chain(
            self.components
                .iter()
                .map(|(name, path)| (name.as_str(), path.as_path())),
        )
    }
}

/// Whether a name satisfies the shared snake_case module policy (Rust 2024).
/// `app` is valid Rust, but separately reserved by application registration.
pub fn valid_module_name(name: &str) -> bool {
    name != "_"
        && name
            .bytes()
            .next()
            .is_some_and(|b| b.is_ascii_lowercase() || b == b'_')
        && name
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
        && !KEYWORDS.contains(&name)
}

const KEYWORDS: &[&str] = &[
    "as", "async", "await", "break", "const", "continue", "crate", "dyn", "else", "enum", "extern",
    "false", "fn", "for", "if", "impl", "in", "let", "loop", "match", "mod", "move", "mut", "pub",
    "ref", "return", "self", "static", "struct", "super", "trait", "true", "type", "unsafe", "use",
    "where", "while", "abstract", "become", "box", "do", "final", "gen", "macro", "override",
    "priv", "try", "typeof", "unsized", "virtual", "yield",
];

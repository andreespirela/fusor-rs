use super::{
    ARTIFACT_VERSION, AppConfig, ArtifactManifest, RegistrationArtifact, Result, SourceArtifact,
    SourceKind,
    error::SourceError,
    external::{LinkedSource, Linker},
    includes::{BINDINGS_PREFIX, TEMPLATE_DIRECTORY},
    validate,
};
use crate::{Page, RustBlock, SourceMap, extract::extract_from, html};
use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use std::{
    fs,
    path::{Path, PathBuf},
};

/// Compile registered sources to a Cargo output directory. No files in the
/// authored application are modified. Useful to hosts and `cargo fusor expand`.
pub fn generate(manifest: &Path, out: &Path) -> Result<ArtifactManifest> {
    let config = AppConfig::load(manifest)?;
    let root = manifest
        .parent()
        .ok_or("manifest has no parent directory")?;
    let package_root = root.canonicalize()?;
    fs::create_dir_all(out)?;
    let out = out.canonicalize()?;
    let mut generator = Generator {
        config: &config,
        root,
        linker: Linker::new(&package_root, &config.output, &out),
        package_root,
        out,
        next_component: 0,
    };
    let templates = generator.out.join(TEMPLATE_DIRECTORY);
    if templates.exists() {
        fs::remove_dir_all(&templates)?;
    }
    // Compiling a source writes nothing, so a rejected source leaves no new output.
    let sources = config
        .discover_sources(root)?
        .into_iter()
        .map(|(name, path)| generator.compile(name, &path))
        .collect::<Result<Vec<_>>>()?;
    generator.write(sources)
}

struct Generator<'a> {
    config: &'a AppConfig,
    root: &'a Path,
    package_root: PathBuf,
    out: PathBuf,
    next_component: usize,
    linker: Linker,
}

struct CompiledSource {
    name: String,
    kind: SourceKind,
    html_path: PathBuf,
    page: Page,
    map: String,
    rust_path: PathBuf,
    /// Inline Rust becomes a module of `crate::ui`.
    ui_module: Option<TokenStream>,
    external: Option<LinkedSource>,
}

impl Generator<'_> {
    fn compile(&mut self, name: String, path: &Path) -> Result<CompiledSource> {
        let kind = SourceKind::of(&name);
        let authored = self.root.join(path);
        let html_path = authored
            .canonicalize()
            .map_err(|error| SourceError::new(&authored, error))?;
        let source = fs::read_to_string(&html_path)?;
        if kind != SourceKind::Entry {
            validate::component_file(&source)
                .map_err(|error| SourceError::extracted(&html_path, error))?;
        }
        let mut page = extract_from(&source, self.next_component)
            .map_err(|error| SourceError::extracted(&html_path, error))?;
        let native = page.blocks.is_empty();
        if native && page.component_count == 0 {
            return Err(SourceError::new(
                &html_path,
                "native template files require an App boundary or rust:component declarations",
            )
            .into());
        }
        if !native && kind == SourceKind::Discovered {
            return Err(SourceError::new(&html_path, "discovered HTML uses fusor::template! in an ordinary Rust module; register script-based HTML explicitly in Cargo metadata").into());
        }
        if kind != SourceKind::Entry {
            if let Some(offset) = page.app_offset {
                return Err(SourceError::at_offset(
                    &html_path,
                    &source,
                    offset,
                    "App is only allowed in the entry document",
                )
                .into());
            }
        }
        self.next_component += page.component_count;
        let rust_path = if native {
            let path = path.to_str().ok_or("template path must be UTF-8")?;
            self.out.join(TEMPLATE_DIRECTORY).join(format!("{path}.rs"))
        } else {
            self.out.join(format!("{BINDINGS_PREFIX}{name}.rs"))
        };
        let mut ui_module = None;
        let mut external = None;
        if let Some(RustBlock {
            element,
            external: Some(rust),
            ..
        }) = page.blocks.first()
        {
            let (linked, marker) =
                self.linker
                    .link(&name, &html_path, &source, element.start, rust)?;
            page.rust.push('\n');
            page.rust.push_str(&marker);
            external = Some(linked);
        } else if !native {
            let module = format_ident!("{name}");
            let path = rust_path
                .to_str()
                .ok_or("generated module path must be UTF-8")?;
            ui_module = Some(quote! { #[path = #path] pub mod #module; });
        }
        if self.config.delivery.is_some() {
            if let Some(module) = page.javascript.first() {
                return Err(SourceError::at(
                    &html_path,
                    module.line,
                    module.column,
                    "component JavaScript is unsupported in server/island delivery; use a browser application",
                )
                .into());
            }
        }
        let map = SourceMap::new(page.locations.clone())?.to_string();
        Ok(CompiledSource {
            name,
            kind,
            html_path,
            page,
            map,
            rust_path,
            ui_module,
            external,
        })
    }

    fn write(self, sources: Vec<CompiledSource>) -> Result<ArtifactManifest> {
        let mut ui_modules = Vec::new();
        let mut registrations = Vec::new();
        let mut artifacts = Vec::new();
        let mut javascript = Vec::new();
        let mut entry = None;
        let mut templates = String::new();
        for source in sources {
            let page = source.page;
            fs::create_dir_all(source.rust_path.parent().expect("generated parent"))?;
            let fingerprint = source.rust_path.with_extension("fingerprint.rs");
            let map = source.rust_path.with_extension("map");
            if let Some(linked) = &source.external {
                fs::write(&linked.registration, &linked.registration_rust)?;
                registrations.push(linked.registration_mod.clone());
            }
            ui_modules.extend(source.ui_module);
            javascript.extend(crate::javascript::prepare(
                &self.package_root,
                &self.out,
                &source.html_path,
                &page.rust,
                source.external.as_ref().map(|linked| linked.path.as_path()),
                &page.javascript,
            )?);
            fs::write(&source.rust_path, &page.rust)?;
            fs::write(&fingerprint, &page.fingerprint)?;
            fs::write(&map, &source.map)?;
            let (external, registration) = match source.external {
                Some(linked) => (
                    Some(linked.path),
                    Some(RegistrationArtifact {
                        rust: linked.registration,
                        line: linked.line,
                        column: linked.column,
                    }),
                ),
                None => (None, None),
            };
            artifacts.push(SourceArtifact {
                name: source.name,
                source: source.html_path,
                rust: source.rust_path,
                fingerprint,
                map,
                external,
                registration,
            });
            if source.kind == SourceKind::Entry {
                // A native entry has no script to load next to, so load before </body>.
                let loader = page
                    .loader_offset
                    .unwrap_or_else(|| html::body_end(&page.html));
                entry = Some((page.html, loader, page.app_offset.is_some()));
            } else {
                templates.push_str(&page.html);
                templates.push('\n');
            }
        }
        let (mut html, mut loader_offset, managed_entry) =
            entry.expect("the entry is always the first source");
        html::insert_before_body_end(&mut html, &templates, Some(&mut loader_offset));
        let html_path = self.out.join("fusor_app.html");
        let module_path = self.out.join("fusor_module.rs");
        fs::write(&html_path, html)?;
        fs::write(
            &module_path,
            quote! { pub mod ui { #(#ui_modules)* } #(#registrations)* }.to_string(),
        )?;
        let artifact = ArtifactManifest {
            version: ARTIFACT_VERSION,
            html: html_path,
            module: module_path,
            loader_offset,
            managed_entry,
            sources: artifacts,
            javascript,
        };
        fs::write(
            self.out.join("fusor_artifacts.json"),
            serde_json::to_vec_pretty(&artifact)?,
        )?;
        Ok(artifact)
    }
}

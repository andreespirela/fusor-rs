use super::{
    ARTIFACT_VERSION, AppConfig, ArtifactManifest, RegistrationArtifact, Result, SourceArtifact,
};
use crate::{SourceMap, error, extract_from};
use html5gum::{DefaultEmitter, Token, Tokenizer};
use quote::{format_ident, quote};
use std::{collections::BTreeSet, fs, path::Path};

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
    let mut next_component = 0;
    let mut artifacts = Vec::new();
    let mut javascript = Vec::new();
    let mut modules = Vec::new();
    let mut registrations = Vec::new();
    let mut external_sources = BTreeSet::new();
    let mut entry_html = None;
    let mut loader_offset = 0;
    let mut managed_entry = false;
    let mut templates = String::new();
    let native_out = out.join(super::includes::TEMPLATE_DIRECTORY);
    if native_out.exists() {
        fs::remove_dir_all(&native_out)?;
    }
    for (name, path) in config.discover_sources(root)? {
        let path = path.as_path();
        let source_path = root
            .join(path)
            .canonicalize()
            .map_err(|e| format!("{}: {e}", root.join(path).display()))?;
        let source = fs::read_to_string(&source_path)?;
        if name != "app" {
            validate_component_file(&source)
                .map_err(|e| format!("{}:{e}", source_path.display()))?;
        }
        let mut page = extract_from(&source, next_component, true)
            .map_err(|e| format!("{}:{e}", source_path.display()))?;
        let native_template = page.blocks.is_empty();
        if native_template && page.component_count == 0 {
            return Err(format!(
                "{}: native template files require an App boundary or rust:component declarations",
                source_path.display()
            )
            .into());
        }
        if !native_template && name.starts_with('@') {
            return Err(format!("{}: discovered HTML uses fusor::template! in an ordinary Rust module; register script-based HTML explicitly in Cargo metadata", source_path.display()).into());
        }
        if name != "app" {
            if let Some(offset) = page.app_offset {
                return Err(format!(
                    "{}:{}",
                    source_path.display(),
                    error(&source, offset, "App is only allowed in the entry document")
                )
                .into());
            }
        }
        next_component += page.component_count;
        let rust = if native_template {
            native_out.join(format!(
                "{}.rs",
                path.to_str().ok_or("template path must be UTF-8")?
            ))
        } else {
            out.join(format!("{}{name}.rs", super::includes::BINDINGS_PREFIX))
        };
        fs::create_dir_all(rust.parent().expect("generated parent"))?;
        let fingerprint = rust.with_extension("fingerprint.rs");
        let map = rust.with_extension("map");
        let mut external_path = None;
        let mut registration = None;
        if let Some(external) = page
            .blocks
            .first()
            .and_then(|block| block.external.as_ref())
        {
            let path = source_path
                .parent()
                .expect("HTML parent")
                .join(&external.src);
            let canonical = path.canonicalize().map_err(|e| {
                format!(
                    "{}:{}: external Rust source {}: {e}",
                    source_path.display(),
                    error(&source, page.blocks[0].element.start, ""),
                    path.display()
                )
            })?;
            if !canonical.starts_with(&package_root)
                || canonical.starts_with(package_root.join(&config.output))
                || canonical.extension().is_none_or(|ext| ext != "rs")
            {
                return Err(format!("{}: external Rust source must be a .rs file inside this package, outside its output directory", source_path.display()).into());
            }
            if !external_sources.insert(canonical) {
                return Err(format!("{}: Rust source registered more than once; share logic through ordinary Rust modules", path.display()).into());
            }
            let module: syn::Path = syn::parse_str(&external.module)?;
            let marker = format_ident!("__FUSOR_BINDINGS_{}", name.to_uppercase());
            page.rust.push('\n');
            page.rust.push_str(
                &quote! {
                    #[doc(hidden)]
                    pub(crate) const #marker: (&str, &str) = __FUSOR_BINDINGS_ORIGIN;
                }
                .to_string(),
            );
            let expected = path.to_str().ok_or("external source path must be UTF-8")?;
            let link = out.join(format!(
                "{}{name}_registration.rs",
                super::includes::BINDINGS_PREFIX
            ));
            fs::write(&link, quote! {
                const _: () = assert!(
                    ::fusor::authoring::source_matches(#module::#marker, #expected),
                    "external Rust source mismatch: src must identify the module containing fusor::bindings!(name)"
                );
            }.to_string())?;
            let location = error(&source, page.blocks[0].element.start, "");
            let link_path = link.to_str().ok_or("registration path must be UTF-8")?;
            let registration_module = format_ident!("__fusor_registration_{name}");
            registrations.push(quote! { #[path = #link_path] mod #registration_module; });
            registration = Some(RegistrationArtifact {
                rust: link,
                line: location.line,
                column: location.column,
            });
            external_path = Some(path);
        } else if !native_template {
            let identifier = format_ident!("{name}");
            let path = rust.to_str().ok_or("generated module path must be UTF-8")?;
            modules.push(quote! { #[path = #path] pub mod #identifier; });
        }
        if !page.javascript.is_empty() && config.delivery.is_some() {
            let module = &page.javascript[0];
            return Err(format!("{}:{}:{}: component JavaScript is unsupported in server/island delivery; use a browser application", source_path.display(), module.line, module.column).into());
        }
        javascript.extend(crate::javascript::prepare(
            &package_root,
            &out,
            &source_path,
            &page.rust,
            external_path.as_deref(),
            &page.javascript,
        )?);
        fs::write(&rust, &page.rust)?;
        fs::write(&fingerprint, &page.fingerprint)?;
        fs::write(&map, SourceMap::new(page.locations.clone())?.to_string())?;
        artifacts.push(SourceArtifact {
            name: name.clone(),
            source: source_path,
            rust,
            fingerprint,
            map,
            external: external_path,
            registration,
        });
        if name == "app" {
            managed_entry = page.app_offset.is_some();
            loader_offset = page.loader_offset.expect("entry has a loader position");
            entry_html = Some(page.html);
        } else {
            templates.push_str(&page.html);
            templates.push('\n');
        }
    }
    let mut html = entry_html.expect("entry always exists");
    // Insert inert templates before the actual body's closing token. Looking at
    // tokens avoids matching a string in JavaScript, CSS or an HTML comment.
    let mut emitter = DefaultEmitter::<usize>::new_with_span();
    emitter.naively_switch_states(true);
    let end = Tokenizer::new_with_emitter(html.as_str(), emitter)
        .find_map(|token| match token.expect("in-memory HTML") {
            Token::EndTag(tag) if &*tag.name == b"body" => Some(tag.span.start),
            _ => None,
        })
        .unwrap_or(html.len());
    if end <= loader_offset {
        loader_offset += templates.len();
    }
    html.insert_str(end, &templates);
    let html_path = out.join("fusor_app.html");
    let module_path = out.join("fusor_module.rs");
    fs::write(&html_path, html)?;
    fs::write(
        &module_path,
        quote! { pub mod ui { #(#modules)* } #(#registrations)* }.to_string(),
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
        out.join("fusor_artifacts.json"),
        serde_json::to_vec_pretty(&artifact)?,
    )?;
    Ok(artifact)
}

fn validate_component_file(source: &str) -> std::result::Result<(), crate::ExtractError> {
    let mut emitter = DefaultEmitter::<usize>::new_with_span();
    emitter.naively_switch_states(true);
    let mut template_depth = 0;
    let mut rust_script = false;
    for token in Tokenizer::new_with_emitter(source, emitter) {
        match token.expect("in-memory HTML") {
            Token::StartTag(tag) => {
                if &*tag.name == b"template" {
                    if template_depth == 0
                        && !tag.attributes.contains_key(b"rust:component".as_slice())
                    {
                        return Err(error(
                            source,
                            tag.span.start,
                            "component files require named <template rust:component=\"Type\"> declarations",
                        ));
                    }
                    template_depth += 1;
                } else if &*tag.name == b"script" {
                    let is_rust = tag.attributes.get(b"type".as_slice()).is_some_and(|value| {
                        value
                            .as_ref()
                            .trim_ascii()
                            .eq_ignore_ascii_case(b"text/rust")
                    });
                    let is_module = template_depth > 0
                        && tag.attributes.get(b"type".as_slice()).is_some_and(|value| {
                            value.as_ref().trim_ascii().eq_ignore_ascii_case(b"module")
                        });
                    if !is_rust && !is_module {
                        return Err(error(
                            source,
                            tag.span.start,
                            "component files may contain Rust scripts and component-local module scripts inside templates",
                        ));
                    }
                    rust_script = true;
                } else if template_depth == 0 {
                    return Err(error(
                        source,
                        tag.span.start,
                        "component files contain Rust scripts and component templates only; put page markup and styles in the entry page or assets",
                    ));
                }
            }
            Token::EndTag(tag) => {
                if &*tag.name == b"template" && template_depth > 0 {
                    template_depth -= 1;
                } else if &*tag.name == b"script" && rust_script {
                    rust_script = false;
                } else if template_depth == 0 {
                    return Err(error(
                        source,
                        tag.span.start,
                        "unexpected top-level closing tag in component file",
                    ));
                }
            }
            Token::String(text)
                if template_depth == 0
                    && !rust_script
                    && !String::from_utf8_lossy(&text).trim().is_empty() =>
            {
                return Err(error(
                    source,
                    text.span.start,
                    "put component text inside its template",
                ));
            }
            Token::Doctype(tag) => {
                return Err(error(
                    source,
                    tag.span.start,
                    "a component file is a template library, not a document",
                ));
            }
            _ => {}
        }
    }
    Ok(())
}

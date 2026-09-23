//! Compile ordinary Rust embedded in `<script type="text/rust">` HTML elements.
//!
//! Rust bodies are copied verbatim. HTML bindings lower to native Rust token
//! trees; rustc checks every expression and its types. Template descriptions
//! share a typed contract with the DOM runtime.

pub mod app;
mod bindings;
mod cargo;
mod javascript;
mod source_map;

pub use cargo::{compile, compile_app};
pub use source_map::{SourceMap, SourceMapError};

use html5gum::{DefaultEmitter, Token, Tokenizer};
use std::{error::Error, fmt, ops::Range};

/// The original byte ranges of a Rust script's element and its content.
#[derive(Debug, Clone)]
pub struct RustBlock {
    pub element: Range<usize>,
    pub content: Range<usize>,
    pub external: Option<ExternalRust>,
}

/// An explicit relationship to an authored, ordinary Cargo module.
#[derive(Debug, Clone)]
pub struct ExternalRust {
    pub src: String,
    pub module: String,
}

/// A component-local native JavaScript module, before path resolution.
#[derive(Debug, Clone)]
pub struct JavaScriptModule {
    pub id: String,
    pub component: String,
    pub src: Option<String>,
    pub content: String,
    pub line: usize,
    pub column: usize,
}

/// An HTML page and the ordinary Rust module extracted from its script blocks.
#[derive(Debug)]
pub struct Page {
    /// Line-preserved Rust bodies followed by generated component implementations.
    pub rust: String,
    /// Native binding tokens used for compatible refresh, without delivery-only
    /// HTML literals. Island delivery always publishes a complete generation.
    pub fingerprint: String,
    /// HTML with Rust scripts removed and binding declarations replaced by DOM markers.
    pub html: String,
    pub blocks: Vec<RustBlock>,
    pub locations: Vec<BindingLocation>,
    pub javascript: Vec<JavaScriptModule>,
    loader_offset: Option<usize>,
    component_count: usize,
    pub(crate) app_offset: Option<usize>,
}

/// A generated binding's source location in the authored HTML.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BindingLocation {
    pub generated_start: usize,
    /// Exclusive end line.
    pub generated_end: usize,
    pub line: usize,
    pub column: usize,
}

impl Page {
    /// Insert the module loader near the first Rust block, outside inert templates.
    /// Module scripts defer execution until the document has been parsed.
    pub fn with_loader(&self) -> String {
        self.with_loader_url("./boot.js")
    }

    pub(crate) fn with_loader_url(&self, url: &str) -> String {
        let mut html = self.html.clone();
        if let Some(offset) = self.loader_offset {
            let url = url
                .replace('&', "&amp;")
                .replace('"', "&quot;")
                .replace('<', "&lt;");
            html.insert_str(
                offset,
                &format!("<script type=\"module\" src=\"{url}\"></script>"),
            );
        }
        html
    }
}

/// An invalid HTML container or unbalanced Rust binding tokens.
/// Rust expression grammar, types, and borrowing are checked by rustc.
#[derive(Debug, PartialEq, Eq)]
pub struct ExtractError {
    pub line: usize,
    pub column: usize,
    pub message: String,
}

impl fmt::Display for ExtractError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}:{}: {}", self.line, self.column, self.message)
    }
}

impl Error for ExtractError {}

fn error(source: &str, offset: usize, message: impl Into<String>) -> ExtractError {
    let mut offset = offset.min(source.len());
    while !source.is_char_boundary(offset) {
        offset -= 1;
    }
    let prefix = &source[..offset];
    ExtractError {
        line: prefix.bytes().filter(|byte| *byte == b'\n').count() + 1,
        column: prefix.rsplit('\n').next().unwrap_or("").chars().count() + 1,
        message: message.into(),
    }
}

fn whitespace(source: &str, output: &mut String) {
    output.extend(source.chars().map(|character| match character {
        '\n' | '\r' | '\t' => character,
        _ => ' ',
    }));
}

/// Extract every `text/rust` script body, in document order, without interpreting
/// Rust, decoding HTML entities in code, or serializing it through an HTML DOM.
/// Standard HTML script-content rules apply, including the `</script>` delimiter.
pub fn extract(source: &str) -> Result<Page, ExtractError> {
    extract_from(source, 0, false)
}

fn extract_from(
    source: &str,
    first_component: usize,
    native_template: bool,
) -> Result<Page, ExtractError> {
    let mut emitter = DefaultEmitter::<usize>::new_with_span();
    // This recognizes script/style raw text and textarea/title RCDATA. For
    // example, a script-looking string inside JavaScript is not another script.
    emitter.naively_switch_states(true);
    let mut blocks = Vec::new();
    let mut current = None;
    let mut templates = Vec::new();
    let mut loader_source_offset = None;
    for token in Tokenizer::new_with_emitter(source, emitter) {
        let token = token.expect("tokenizing an in-memory string is infallible");
        match token {
            Token::StartTag(tag) if &*tag.name == b"template" => {
                templates.push(tag.span.start);
            }
            Token::EndTag(tag) if &*tag.name == b"template" => {
                templates.pop();
            }
            Token::StartTag(tag) if &*tag.name == b"script" => {
                let is_rust = tag.attributes.get(b"type".as_slice()).is_some_and(|value| {
                    value
                        .as_ref()
                        .trim_ascii()
                        .eq_ignore_ascii_case(b"text/rust")
                });
                if is_rust {
                    if tag.self_closing {
                        return Err(error(
                            source,
                            tag.span.start,
                            "Rust scripts need an explicit closing </script> tag",
                        ));
                    }
                    let src = tag.attributes.get(b"src".as_slice());
                    let module = tag.attributes.get(b"rust:module".as_slice());
                    let external = match (src, module) {
                        (None, None) => None,
                        (Some(src), Some(module)) => {
                            let src = String::from_utf8_lossy(src).trim().to_owned();
                            let module = String::from_utf8_lossy(module).trim().to_owned();
                            if src.is_empty()
                                || src.contains(':')
                                || src.starts_with('/')
                                || src.contains(['?', '#', '\\'])
                            {
                                return Err(error(
                                    source,
                                    tag.span.start,
                                    "src must be a nonempty local Rust file path relative to this HTML file",
                                ));
                            }
                            let path = syn::parse_str::<syn::Path>(&module).map_err(|_| error(source, tag.span.start, "rust:module requires an absolute Rust module path such as crate::app"))?;
                            if path.leading_colon.is_some()
                                || path
                                    .segments
                                    .first()
                                    .is_none_or(|part| part.ident != "crate")
                                || path
                                    .segments
                                    .iter()
                                    .any(|part| !matches!(part.arguments, syn::PathArguments::None))
                            {
                                return Err(error(
                                    source,
                                    tag.span.start,
                                    "rust:module requires an absolute Rust module path such as crate::app",
                                ));
                            }
                            Some(ExternalRust { src, module })
                        }
                        _ => {
                            return Err(error(
                                source,
                                tag.span.start,
                                "external Rust scripts require both src and rust:module; declare the source through an ordinary Rust mod and add fusor::bindings!(name) inside it",
                            ));
                        }
                    };
                    current = Some((tag.span.start, tag.span.end, external));
                    // A script inserted inside template.content is inert. Keep
                    // component-local Rust usable by loading before its template.
                    loader_source_offset
                        .get_or_insert(templates.first().copied().unwrap_or(tag.span.start));
                }
            }
            Token::EndTag(tag) if &*tag.name == b"script" => {
                if let Some((start, body_start, external)) = current.take() {
                    if external.is_some() && !source[body_start..tag.span.start].trim().is_empty() {
                        return Err(error(
                            source,
                            body_start,
                            "a Rust script with src must have an empty body",
                        ));
                    }
                    blocks.push(RustBlock {
                        element: start..tag.span.end,
                        content: body_start..tag.span.start,
                        external,
                    });
                }
            }
            Token::Error(problem) => {
                return Err(error(
                    source,
                    problem.span.start,
                    format!("invalid HTML: {}", problem.value),
                ));
            }
            _ => {}
        }
    }
    if let Some((start, _, _)) = current {
        return Err(error(
            source,
            start,
            "Rust script is missing its closing </script> tag",
        ));
    }
    if blocks.len() > 1 && blocks.iter().any(|block| block.external.is_some()) {
        return Err(error(
            source,
            blocks[1].element.start,
            "use one external Rust source or inline Rust blocks per HTML module; compose external code with ordinary mod and use declarations",
        ));
    }
    let mut rust = String::new();
    let mut rust_cursor = 0;
    for block in &blocks {
        whitespace(&source[rust_cursor..block.content.start], &mut rust);
        rust.push_str(&source[block.content.clone()]);
        rust_cursor = block.content.end;
    }
    whitespace(&source[rust_cursor..], &mut rust);
    let bindings::Compiled {
        mut edits,
        templates: projected_templates,
        locations,
        component_count,
        app_offset,
        fingerprint,
        javascript,
    } = bindings::compile(source, &blocks, &mut rust, first_component)?;
    if component_count > 0 && blocks.is_empty() && !native_template {
        return Err(error(
            source,
            0,
            "declare component state in a <script type=\"text/rust\"> block, or use compile_app() with fusor::template! in an ordinary Rust module",
        ));
    }
    if blocks.is_empty() && native_template {
        let mut emitter = DefaultEmitter::<usize>::new_with_span();
        emitter.naively_switch_states(true);
        loader_source_offset = Some(
            Tokenizer::new_with_emitter(source, emitter)
                .find_map(|token| match token.expect("in-memory HTML") {
                    Token::EndTag(tag) if &*tag.name == b"body" => Some(tag.span.start),
                    _ => None,
                })
                .unwrap_or(source.len()),
        );
    }
    for block in &blocks {
        let mut replacement = String::new();
        whitespace(&source[block.element.clone()], &mut replacement);
        edits.push(bindings::Edit {
            range: block.element.clone(),
            replacement,
        });
    }
    edits.sort_by_key(|edit| edit.range.start);
    let mut html = String::new();
    let mut cursor = 0;
    let mut loader_offset = None;
    for edit in edits {
        if edit.range.start < cursor {
            return Err(error(source, edit.range.start, "overlapping HTML bindings"));
        }
        if let Some(offset) = loader_source_offset {
            if (cursor..=edit.range.start).contains(&offset) {
                loader_offset = Some(html.len() + source[cursor..offset].len());
            }
        }
        html.push_str(&source[cursor..edit.range.start]);
        html.push_str(&edit.replacement);
        cursor = edit.range.end;
    }
    if let Some(offset) = loader_source_offset.filter(|offset| *offset >= cursor) {
        loader_offset = Some(html.len() + source[cursor..offset].len());
    }
    html.push_str(&source[cursor..]);
    if !projected_templates.is_empty() {
        let mut emitter = DefaultEmitter::<usize>::new_with_span();
        emitter.naively_switch_states(true);
        let end = Tokenizer::new_with_emitter(html.as_str(), emitter)
            .find_map(|token| match token.expect("in-memory HTML") {
                Token::EndTag(tag) if &*tag.name == b"body" => Some(tag.span.start),
                _ => None,
            })
            .unwrap_or(html.len());
        if loader_offset.is_some_and(|offset| offset >= end) {
            loader_offset = loader_offset.map(|offset| offset + projected_templates.len());
        }
        html.insert_str(end, &projected_templates);
    }
    Ok(Page {
        rust,
        fingerprint,
        html,
        blocks,
        locations,
        loader_offset,
        component_count,
        app_offset,
        javascript,
    })
}

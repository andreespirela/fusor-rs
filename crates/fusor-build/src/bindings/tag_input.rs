//! Attributes of the built-in tags: which ones a tag accepts, `{{ expression }}`
//! values and the names a tag binds. Errors name the tag and point at the value
//! when there is one, at the tag otherwise.
use super::{interpolation, tokens::Rust};
use crate::{ExtractError, error};
use html5gum::{HtmlString, Spanned, StartTag};

pub(super) struct TagInput<'a> {
    pub source: &'a str,
    pub tag: &'a StartTag<usize>,
    /// The tag as errors name it, such as `ForEach`.
    label: &'a str,
}

impl<'a> TagInput<'a> {
    pub fn new(source: &'a str, tag: &'a StartTag<usize>, label: &'a str) -> Self {
        Self { source, tag, label }
    }

    pub fn error(&self, message: impl Into<String>) -> ExtractError {
        error(self.source, self.tag.span.start, message)
    }

    pub fn error_at(&self, offset: usize, message: impl Into<String>) -> ExtractError {
        error(self.source, offset, message)
    }

    /// Built-ins that own content need an explicit closing tag.
    pub fn closed(&self) -> Result<(), ExtractError> {
        if self.tag.self_closing {
            return Err(self.error(format!("{} requires an explicit closing tag", self.label)));
        }
        Ok(())
    }

    /// Reject any attribute outside `names`; `usage` completes "<Tag> accepts ...".
    pub fn accepts(&self, names: &[&str], usage: &str) -> Result<(), ExtractError> {
        let known = |key: &HtmlString| names.iter().any(|name| name.as_bytes() == key.as_slice());
        if self.tag.attributes.keys().all(known) {
            return Ok(());
        }
        Err(self.error(format!("{} accepts {usage}", self.label)))
    }

    pub fn has(&self, name: &str) -> bool {
        self.tag.attributes.contains_key(name.as_bytes())
    }

    fn get(&self, name: &str) -> Option<&'a Spanned<HtmlString, usize>> {
        self.tag.attributes.get(name.as_bytes())
    }

    /// A raw attribute value and where it starts in the source.
    pub fn text(&self, name: &str) -> Option<(String, usize)> {
        self.get(name).map(|value| {
            (
                String::from_utf8_lossy(value).into_owned(),
                crate::html::value_start(self.source, value.span.start),
            )
        })
    }

    /// A required attribute holding exactly one `{{ Rust expression }}`.
    pub fn expression(&self, name: &str) -> Result<Rust, ExtractError> {
        self.optional_expression(name)?.ok_or_else(|| {
            self.error(format!(
                "{} requires {name}=\"{{{{ Rust expression }}}}\"",
                self.label
            ))
        })
    }

    pub fn optional_expression(&self, name: &str) -> Result<Option<Rust>, ExtractError> {
        let Some((text, offset)) = self.text(name) else {
            return Ok(None);
        };
        let parts = interpolation::interpolations(self.source, &text, offset, false)?;
        interpolation::exact_expression(
            self.source,
            &text,
            parts,
            offset,
            &format!(
                "{} {name} requires exactly one {{{{ Rust expression }}}}",
                self.label
            ),
        )
        .map(Some)
    }

    /// A local the tag introduces, such as `let`.
    pub fn binding(&self, name: &str) -> Result<Option<Rust>, ExtractError> {
        self.text(name)
            .map(|(text, offset)| self.local(name, &text, offset))
            .transpose()
    }

    /// A local the tag always introduces, named `default` unless renamed.
    pub fn binding_or(&self, name: &str, default: &str) -> Result<Rust, ExtractError> {
        let (text, offset) = self
            .text(name)
            .unwrap_or_else(|| (default.to_owned(), self.tag.span.start));
        self.local(name, &text, offset)
    }

    /// Check a local the tag introduces; `role` names it in errors.
    pub fn local(&self, role: &str, name: &str, offset: usize) -> Result<Rust, ExtractError> {
        if super::tags::reserved_scope_name(name) {
            return Err(self.error_at(
                offset,
                format!("{} {role} cannot shadow framework scope names", self.label),
            ));
        }
        self.field(role, name, offset)
    }

    /// Check a Rust field or local name the tag introduces.
    pub fn field(&self, role: &str, name: &str, offset: usize) -> Result<Rust, ExtractError> {
        if !super::tags::snake_case_ident(name) {
            return Err(self.error_at(
                offset,
                format!("{} {role} must be a snake_case Rust identifier", self.label),
            ));
        }
        Rust::parse(self.source, name, offset)
    }
}

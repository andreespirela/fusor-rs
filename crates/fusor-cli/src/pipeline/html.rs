//! The compiler records the loader's byte offset; nothing here parses HTML.
use crate::error::{Error, Result};
use fusor_build::app::ArtifactManifest;
use std::fs;

/// `revision` is set only in development, where the refresh client uses it to
/// tell a patched document from a stale one.
pub(crate) fn render(
    artifact: &ArtifactManifest,
    url_prefix: &str,
    revision: Option<u64>,
    styles: &[String],
) -> Result<String> {
    let mut html = fs::read_to_string(&artifact.html)?;
    let offset = artifact.loader_offset;
    if !html.is_char_boundary(offset) {
        return Err(Error::internal(
            "the compiler recorded a loader offset inside a character",
        ));
    }
    let marker = revision
        .map(|revision| format!(" data-rf-revision=\"{revision}\""))
        .unwrap_or_default();
    let stylesheets = styles
        .iter()
        .map(|style| {
            let style = style.replace('&', "&amp;").replace('"', "&quot;");
            format!("<link rel=\"stylesheet\" href=\"{url_prefix}/pkg/{style}\">")
        })
        .collect::<String>();
    html.insert_str(
        offset,
        &format!(
            "{stylesheets}<script type=\"module\"{marker} src=\"{url_prefix}/boot.js\"></script>"
        ),
    );
    Ok(html)
}

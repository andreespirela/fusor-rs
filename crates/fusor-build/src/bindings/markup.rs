//! HTML the compiler writes into templates: marker comments, component root
//! attributes and escaped attribute values.
use fusor::template::{self, ComponentId, MountId, MountMarker, TextId, TextMarker};

/// An empty mount point; the runtime inserts managed content between the markers.
pub(super) fn mount_point(point: MountId) -> String {
    format!(
        "<!--{}--><!--{}-->",
        MountMarker::Start(point),
        MountMarker::End(point)
    )
}

pub(super) fn text_slot(id: TextId) -> String {
    format!(
        "<!--{}--><!--{}-->",
        TextMarker::Start(id),
        TextMarker::End(id)
    )
}

/// Identify a component root and the template format it was compiled for.
pub(super) fn component_attributes(id: ComponentId) -> String {
    format!(
        " {}=\"{id}\" {}=\"{}\"",
        template::COMPONENT_ATTRIBUTE,
        template::VERSION_ATTRIBUTE,
        template::VERSION
    )
}

pub(super) fn template_open(id: ComponentId) -> String {
    format!("<template{}>", component_attributes(id))
}

/// A button that activates the island instance `id`; the islands runtime
/// listens for clicks on it.
pub(super) fn activation_target(id: &str) -> String {
    format!(" data-fusor-activate-target=\"{}\"", escape_attribute(id))
}

/// Escape a value for a double-quoted attribute written by the compiler, as the
/// server writer escapes the attributes it renders.
pub(super) fn escape_attribute(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    template::escape_into(&mut escaped, value, true);
    escaped
}

#[cfg(test)]
mod tests {
    #[test]
    fn compiler_attributes_escape_like_the_server_writer() {
        assert_eq!(
            super::escape_attribute(r#"a&b"c'd<e>f"#),
            "a&amp;b&quot;c&#39;d&lt;e&gt;f"
        );
    }
}

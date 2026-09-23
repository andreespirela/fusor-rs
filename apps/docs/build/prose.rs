//! Small, deliberately limited authoring format: paragraphs and inline code.
//! Produces typed text nodes, never HTML. Malformed code delimiters fail the build.
pub fn compile(source: &str) -> Result<String, String> {
    let mut output = String::from("&[");
    for (id, paragraph) in source
        .split("\n\n")
        .filter(|p| !p.trim().is_empty())
        .enumerate()
    {
        let paragraph = paragraph.trim();
        if paragraph.matches('`').count() % 2 != 0 {
            return Err(format!(
                "Unclosed inline code in documentation: {paragraph}"
            ));
        }
        output.push_str(&format!("ParagraphData {{ id: {id}, spans: &["));
        for (index, text) in paragraph.split('`').enumerate() {
            if text.is_empty() {
                if index % 2 == 1 {
                    return Err(format!("Empty inline code in documentation: {paragraph}"));
                }
                continue;
            }
            output.push_str(&format!(
                "InlineData {{ id: {index}, code: {}, text: {text:?} }},",
                index % 2 == 1
            ));
        }
        output.push_str("] },");
    }
    output.push(']');
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::compile;

    #[test]
    fn preserves_code_and_paragraph_boundaries_as_data() {
        let result =
            compile("Use `Signal<String>` with `\"Ada\"`.\n\nCall `update(|n| *n += 1)`.").unwrap();
        assert_eq!(result.matches("ParagraphData {").count(), 2);
        assert_eq!(result.matches("code: true").count(), 3);
        assert!(result.contains("text: \"Signal<String>\""));
        assert!(result.contains("text: \"update(|n| *n += 1)\""));
    }

    #[test]
    fn rejects_incomplete_markup_and_ignores_empty_paragraphs() {
        assert!(compile("Use `owner").is_err());
        assert!(compile("Use `` here").is_err());
        assert_eq!(compile(" \n\n").unwrap(), "&[]");
        // HTML-looking input is still a text value, with no HTML injection API.
        assert!(
            compile("`<script>alert(1)</script>`")
                .unwrap()
                .contains("text: \"<script>alert(1)</script>\"")
        );
    }
}

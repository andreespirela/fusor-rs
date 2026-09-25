//! Shared HTML tokenizing and `</body>` placement.
use html5gum::{DefaultEmitter, StartTag, Token, Tokenizer};

/// Tokens with byte spans. Script/style raw text and textarea/title RCDATA are
/// recognized, so a script-looking string inside JavaScript is not another script.
pub(crate) fn tokens(source: &str) -> impl Iterator<Item = Token<usize>> + '_ {
    let mut emitter = DefaultEmitter::<usize>::new_with_span();
    emitter.naively_switch_states(true);
    Tokenizer::new_with_emitter(source, emitter)
        .map(|token| token.expect("tokenizing an in-memory string is infallible"))
}

pub(crate) fn attribute(tag: &StartTag<usize>, name: &[u8]) -> Option<String> {
    tag.attributes
        .get(name)
        .map(|value| String::from_utf8_lossy(value).trim().to_owned())
}

pub(crate) fn has_type(tag: &StartTag<usize>, kind: &[u8]) -> bool {
    tag.attributes
        .get(b"type".as_slice())
        .is_some_and(|value| value.as_ref().trim_ascii().eq_ignore_ascii_case(kind))
}

pub(crate) fn is_rust_script(tag: &StartTag<usize>) -> bool {
    has_type(tag, b"text/rust")
}

/// Looking at tokens avoids matching `</body>` inside JavaScript, CSS or a comment.
pub(crate) fn body_end(html: &str) -> usize {
    tokens(html)
        .find_map(|token| match token {
            Token::EndTag(tag) if &*tag.name == b"body" => Some(tag.span.start),
            _ => None,
        })
        .unwrap_or(html.len())
}

/// A loader at the insertion point stays after the inserted text, next to `</body>`.
pub(crate) fn insert_before_body_end(html: &mut String, text: &str, loader: Option<&mut usize>) {
    let end = body_end(html);
    if let Some(loader) = loader.filter(|loader| **loader >= end) {
        *loader += text.len();
    }
    html.insert_str(end, text);
}

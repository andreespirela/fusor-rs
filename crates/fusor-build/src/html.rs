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

/// Whether markup holds only whitespace and comments, which render nothing.
pub(crate) fn is_blank(html: &str) -> bool {
    tokens(html).all(|token| match token {
        Token::String(text) => text.iter().all(u8::is_ascii_whitespace),
        Token::Comment(_) | Token::Error(_) => true,
        _ => false,
    })
}

/// html5gum spans an attribute from its name. Skip the name, `=` and an opening
/// quote to find where the value starts in the source.
pub(crate) fn value_start(source: &str, attribute: usize) -> usize {
    let rest = &source[attribute..];
    let name = rest
        .find(|ch: char| ch.is_ascii_whitespace() || matches!(ch, '=' | '/' | '>'))
        .unwrap_or(rest.len());
    let after_name = rest[name..].trim_start();
    let Some(value) = after_name.strip_prefix('=') else {
        return attribute + name;
    };
    let value = value.trim_start();
    let quoted = value.strip_prefix(['"', '\'']).unwrap_or(value);
    source.len() - quoted.len()
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

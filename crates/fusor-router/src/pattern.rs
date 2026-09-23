//! URL matching independent of browser history and HTML rendering.
use crate::{AppUrl, UrlError};
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Eq)]
enum Segment {
    Literal(String),
    Parameter(String),
}

/// An exact route pattern, optionally ending in `/*` to delegate a remainder.
#[derive(Clone, Debug)]
pub struct Pattern {
    segments: Vec<Segment>,
    delegated: bool,
}

/// Decoded captures and the number of path segments consumed by a match.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Match {
    pub params: BTreeMap<String, String>,
    pub consumed: usize,
}

impl Pattern {
    pub fn new(path: &str) -> Result<Self, UrlError> {
        if path.starts_with("//") {
            return Err(UrlError("route patterns have at most one leading slash"));
        }
        let path = path.strip_prefix('/').unwrap_or(path);
        if path.contains(['?', '#', '\\']) || path.contains("//") {
            return Err(UrlError("route patterns contain path segments only"));
        }
        let mut parts: Vec<_> = if path.is_empty() {
            vec![]
        } else {
            path.split('/').collect()
        };
        let delegated = parts.last() == Some(&"*");
        if delegated {
            parts.pop();
        }
        if parts.last() == Some(&"") {
            parts.pop();
        }
        let mut names = std::collections::BTreeSet::new();
        let segments = parts
            .into_iter()
            .map(|part| {
                if let Some(name) = part.strip_prefix(':') {
                    if name.is_empty()
                        || !name.bytes().enumerate().all(|(i, c)| {
                            c == b'_' || c.is_ascii_alphabetic() || (i > 0 && c.is_ascii_digit())
                        })
                        || !names.insert(name.to_owned())
                    {
                        return Err(UrlError(
                            "route parameter names must be distinct identifiers",
                        ));
                    }
                    Ok(Segment::Parameter(name.into()))
                } else {
                    if part.contains(['*', ':', '%'])
                        || part == "."
                        || part == ".."
                        || part.chars().any(char::is_control)
                    {
                        return Err(UrlError(
                            "use literal segments, :name captures, and an optional trailing /*",
                        ));
                    }
                    Ok(Segment::Literal(part.into()))
                }
            })
            .collect::<Result<_, _>>()?;
        Ok(Self {
            segments,
            delegated,
        })
    }
    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.segments.iter().filter_map(|s| match s {
            Segment::Parameter(n) => Some(n.as_str()),
            _ => None,
        })
    }
    pub fn delegated(&self) -> bool {
        self.delegated
    }
    pub fn matches(&self, segments: &[String], offset: usize) -> Option<Match> {
        let rest = segments.get(offset..)?;
        if rest.len() < self.segments.len()
            || (!self.delegated && rest.len() != self.segments.len())
        {
            return None;
        }
        let mut params = BTreeMap::new();
        for (segment, value) in self.segments.iter().zip(rest) {
            match segment {
                Segment::Literal(literal) if literal != value => return None,
                Segment::Parameter(name) if !value.is_empty() => {
                    params.insert(name.clone(), value.clone());
                }
                Segment::Parameter(_) => return None,
                _ => {}
            }
        }
        Some(Match {
            params,
            consumed: offset + self.segments.len(),
        })
    }
    /// Higher values win over less-specific patterns, independent of declaration order.
    pub fn priority(&self) -> Vec<u8> {
        self.segments
            .iter()
            .map(|s| {
                if matches!(s, Segment::Literal(_)) {
                    2
                } else {
                    1
                }
            })
            .chain([if self.delegated { 0 } else { 3 }])
            .collect()
    }
    /// Patterns with identical specificity that can match the same URL are ambiguous.
    pub fn conflicts(&self, other: &Self) -> bool {
        self.priority() == other.priority()
            && self
                .segments
                .iter()
                .zip(&other.segments)
                .all(|(a, b)| match (a, b) {
                    (Segment::Literal(a), Segment::Literal(b)) => a == b,
                    _ => true,
                })
    }
}

/// Path segments for routing. Root and a trailing slash have the same identity.
pub fn path_segments(url: &AppUrl) -> Result<Vec<String>, UrlError> {
    let mut segments = url.segments()?;
    if segments.last().is_some_and(String::is_empty) {
        segments.pop();
    }
    Ok(segments)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn nested_prefix_and_decoded_captures() {
        let segments =
            path_segments(&AppUrl::parse("/teams/a%2Fb/settings?tab=x").unwrap()).unwrap();
        let parent = Pattern::new("/teams/:team/*")
            .unwrap()
            .matches(&segments, 0)
            .unwrap();
        assert_eq!(parent.params["team"], "a/b");
        assert!(
            Pattern::new("settings")
                .unwrap()
                .matches(&segments, parent.consumed)
                .is_some()
        );
        assert!(
            Pattern::new("/teams/:team")
                .unwrap()
                .matches(&segments, 0)
                .is_none()
        );
        let base = path_segments(&AppUrl::parse("/dashboard").unwrap()).unwrap();
        let matched = Pattern::new("/dashboard/*")
            .unwrap()
            .matches(&base, 0)
            .unwrap();
        assert!(
            Pattern::new("")
                .unwrap()
                .matches(&base, matched.consumed)
                .is_some()
        );
    }
    #[test]
    fn priority_and_ambiguity() {
        assert!(
            Pattern::new("/articles/new").unwrap().priority()
                > Pattern::new("/articles/:id").unwrap().priority()
        );
        assert!(
            Pattern::new("/articles/:id")
                .unwrap()
                .conflicts(&Pattern::new("/articles/:slug").unwrap())
        );
        assert!(
            !Pattern::new("/a/:id")
                .unwrap()
                .conflicts(&Pattern::new("/b/:id").unwrap())
        );
        for pattern in [
            "/:id/:id", "/a/*/b", "/:123", "/a?query", "/../a", "//", "//a",
        ] {
            assert!(Pattern::new(pattern).is_err(), "{pattern}");
        }
    }
}

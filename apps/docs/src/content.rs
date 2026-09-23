use crate::code::{CodeData, CodeToken};

#[derive(Clone, Copy, PartialEq)]
pub struct InlineData {
    pub id: usize,
    pub code: bool,
    pub text: &'static str,
}
#[derive(Clone, Copy, PartialEq)]
pub struct ParagraphData {
    pub id: usize,
    pub spans: &'static [InlineData],
}
#[derive(Clone, Copy, PartialEq)]
pub struct SectionData {
    pub id: &'static str,
    pub title: &'static str,
    pub body_prose: &'static [ParagraphData],
    pub body: &'static str,
    pub code: &'static str,
    pub tokens: &'static [CodeToken],
    pub language: &'static str,
    pub note_prose: &'static [ParagraphData],
    pub note: &'static str,
    pub references: &'static [LinkData],
    pub links: &'static [LinkData],
}
pub struct PageData {
    pub parent: Option<&'static str>,
    pub reference: bool,
    pub slug: &'static str,
    pub title: &'static str,
    pub group: &'static str,
    pub lead_prose: &'static [ParagraphData],
    pub lead: &'static str,
    pub sections: &'static [SectionData],
}
include!(concat!(env!("OUT_DIR"), "/content.rs"));
#[derive(Clone, Copy, PartialEq)]
pub struct LinkData {
    pub href: &'static str,
    pub label: &'static str,
}

#[derive(Clone, Copy, PartialEq)]
pub struct DemoData {
    pub slug: &'static str,
    pub title: &'static str,
    pub category: &'static str,
    pub description: &'static str,
    pub try_it: &'static str,
    pub explanation: &'static str,
    pub guide: &'static str,
    pub guide_label: &'static str,
    pub preview_title: &'static str,
    pub preview_value: &'static str,
    pub preview_detail: &'static str,
    pub rust: CodeData,
    pub html: CodeData,
    pub javascript: Option<CodeData>,
}

/// Ancestors are ordered from the topic root to the immediate parent.
pub fn ancestors(index: usize) -> Vec<usize> {
    let mut result = Vec::new();
    let mut current = PAGES.get(index).and_then(|page| page.parent);
    while let Some(slug) = current {
        let parent = PAGES
            .iter()
            .position(|page| page.slug == slug)
            .expect("validated parent");
        result.push(parent);
        current = PAGES[parent].parent;
    }
    result.reverse();
    result
}
pub fn children(parent: Option<&str>, group: &str) -> Vec<usize> {
    PAGES
        .iter()
        .enumerate()
        .filter(|(_, page)| page.parent == parent && page.group == group)
        .map(|(index, _)| index)
        .collect()
}
pub fn matches(index: usize, query: &str) -> bool {
    let needle = query.trim().to_lowercase();
    let page = &PAGES[index];
    [page.title, page.group, page.lead]
        .iter()
        .any(|text| text.replace('`', "").to_lowercase().contains(&needle))
        || page.sections.iter().any(|section| {
            [section.title, section.body, section.code, section.note]
                .iter()
                .any(|text| text.replace('`', "").to_lowercase().contains(&needle))
        })
}

use fusor::prelude::*;

#[derive(Clone, Copy, PartialEq)]
pub struct CodeToken {
    pub id: usize,
    pub text: &'static str,
    pub style: &'static str,
}

#[derive(Clone, Copy, PartialEq)]
pub struct CodeFile {
    pub name: &'static str,
    pub href: &'static str,
    pub tokens: &'static [CodeToken],
}

include!(concat!(env!("OUT_DIR"), "/highlighted.rs"));

#[derive(FromInputs)]
pub struct CodeBlock {
    #[input]
    file: Memo<&'static CodeFile>,
}

fusor::template!("web/components/code.html");

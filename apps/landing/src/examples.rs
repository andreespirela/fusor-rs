use crate::code::{self, CodeFile};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Example {
    Search,
    KeyedList,
    AsyncData,
    Counter,
}

impl Example {
    pub fn source(self, rust: bool) -> &'static CodeFile {
        match (self, rust) {
            (Self::Counter, false) => &code::COUNTER_HTML,
            (Self::Counter, true) => &code::COUNTER_RS,
            (Self::Search, false) => &code::SEARCH_HTML,
            (Self::Search, true) => &code::SEARCH_RS,
            (Self::KeyedList, false) => &code::KEYED_LIST_HTML,
            (Self::KeyedList, true) => &code::KEYED_LIST_RS,
            (Self::AsyncData, false) => &code::ASYNC_DATA_HTML,
            (Self::AsyncData, true) => &code::ASYNC_DATA_RS,
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            Self::Counter => "Update a signal. Only the text node that reads it changes.",
            Self::Search => {
                "Type to filter the guides. The input and results share reactive state."
            }
            Self::KeyedList => {
                "Reorder editable rows. Stable keys keep each row’s DOM and input value together."
            }
            Self::AsyncData => {
                "Switch issues while loading. The title and status publish together when both reads are ready."
            }
        }
    }

    pub fn guide(self) -> &'static str {
        match self {
            Self::Counter => "/docs/components",
            Self::Search => "/docs/reactivity",
            Self::KeyedList => "/docs/components/for-each",
            Self::AsyncData => "/docs/coherent-async",
        }
    }
}

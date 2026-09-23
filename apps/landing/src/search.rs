use fusor::prelude::*;

#[derive(FromInputs)]
pub struct LiveSearch {
    #[local(init = signal(String::new()))]
    query: Signal<String>,
}

#[derive(Clone, PartialEq)]
struct Guide {
    title: &'static str,
    topic: &'static str,
    path: &'static str,
}

impl LiveSearch {
    fn results(&self) -> Vec<Guide> {
        let query = self.query.get().trim().to_lowercase();
        GUIDES
            .iter()
            .filter(|guide| {
                format!("{} {}", guide.title, guide.topic)
                    .to_lowercase()
                    .contains(&query)
            })
            .cloned()
            .collect()
    }
}

const GUIDES: &[Guide] = &[
    Guide {
        title: "Signals",
        topic: "Reactive state",
        path: "/docs/reactivity",
    },
    Guide {
        title: "Components",
        topic: "Typed inputs",
        path: "/docs/components",
    },
    Guide {
        title: "Async boundaries",
        topic: "Consistent views",
        path: "/docs/coherent-async",
    },
    Guide {
        title: "Routing",
        topic: "Typed navigation",
        path: "/docs/routing",
    },
];

fusor::template!("web/components/search.html");

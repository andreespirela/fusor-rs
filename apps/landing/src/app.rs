use crate::{
    async_data::AsyncData,
    code::{CodeBlock, CodeFile},
    counter::Counter,
    examples::Example,
    keyed_list::KeyedList,
    search::LiveSearch,
};
use fusor::prelude::*;

const INSTALL: &str = "cargo install --path crates/fusor-cli --locked";

struct App {
    example: Signal<Example>,
    show_rust: Signal<bool>,
    copy_label: Signal<&'static str>,
}

impl App {
    fn new() -> Self {
        Self {
            example: signal(Example::Search),
            show_rust: signal(false),
            copy_label: signal("Copy command"),
        }
    }

    fn select(&self, example: Example) {
        self.example.set(example);
        self.show_rust.set(false);
    }

    fn source(&self) -> &'static CodeFile {
        self.example.get().source(self.show_rust.get())
    }

    fn copy_install(&self) {
        let label = self.copy_label.clone();
        let Some(window) = web_sys::window() else {
            return;
        };
        let promise = window.navigator().clipboard().write_text(INSTALL);
        wasm_bindgen_futures::spawn_local(async move {
            label.set(
                if wasm_bindgen_futures::JsFuture::from(promise).await.is_ok() {
                    "Copied!"
                } else {
                    "Select the command to copy"
                },
            );
        });
    }
}

fusor::template!("web/index.html");

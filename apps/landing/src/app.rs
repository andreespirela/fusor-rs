use crate::{
    async_data::AsyncData,
    code::{self, CodeBlock, CodeFile},
    counter::Counter,
    examples::Example,
    keyed_list::KeyedList,
    search::LiveSearch,
};
use fusor::prelude::*;

const INSTALL_UNIX: &str = "curl -fsSL https://fusor.build/install.sh | sh";
const INSTALL_WINDOWS: &str = "irm https://fusor.build/install.ps1 | iex";
// Shown literally in the source key; `{{` in template text starts a binding.
const INTERPOLATION: &str = "{{ … }}";

struct App {
    example: Signal<Example>,
    show_page: Signal<bool>,
    show_rust: Signal<bool>,
    install_windows: Signal<bool>,
    copy_label: Signal<&'static str>,
}

impl App {
    fn new() -> Self {
        Self {
            example: signal(Example::Counter),
            show_page: signal(false),
            show_rust: signal(false),
            install_windows: signal(
                web_sys::window()
                    .and_then(|window| window.navigator().user_agent().ok())
                    .is_some_and(|agent| agent.contains("Windows")),
            ),
            copy_label: signal("Copy command"),
        }
    }

    fn select(&self, example: Example) {
        self.example.set(example);
        self.show_file(false, false);
    }

    // The editor shows the selected component's files or the page that hosts it.
    fn file(&self, page: bool, rust: bool) -> &'static CodeFile {
        match (page, rust) {
            (false, _) => self.example.get().source(rust),
            (true, false) => &code::HOST_HTML,
            (true, true) => &code::HOST_RS,
        }
    }

    fn shows(&self, page: bool, rust: bool) -> bool {
        self.show_page.get() == page && self.show_rust.get() == rust
    }

    fn show_file(&self, page: bool, rust: bool) {
        fusor::batch(|| {
            self.show_page.set(page);
            self.show_rust.set(rust);
        });
    }

    fn source(&self) -> &'static CodeFile {
        self.file(self.show_page.get(), self.show_rust.get())
    }

    fn select_install(&self, windows: bool) {
        self.install_windows.set(windows);
        self.copy_label.set("Copy command");
    }

    fn install_command(&self) -> &'static str {
        if self.install_windows.get() {
            INSTALL_WINDOWS
        } else {
            INSTALL_UNIX
        }
    }

    fn install_note(&self) -> &'static str {
        if self.install_windows.get() {
            "Run in PowerShell, then open a new terminal to use fusor."
        } else {
            "Follow the installer’s PATH instructions to make fusor available in your shell."
        }
    }

    fn copy_install(&self) {
        let label = self.copy_label.clone();
        let platform = self.install_windows.clone();
        let copied_platform = platform.get();
        let Some(window) = web_sys::window() else {
            return;
        };
        let promise = window
            .navigator()
            .clipboard()
            .write_text(self.install_command());
        wasm_bindgen_futures::spawn_local(async move {
            let copied = wasm_bindgen_futures::JsFuture::from(promise).await.is_ok();
            if platform.get() == copied_platform {
                label.set(if copied { "Copied!" } else { "Select to copy" });
            }
        });
    }
}

fusor::template!("web/index.html");

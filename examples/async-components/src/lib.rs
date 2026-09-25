use fusor::prelude::*;
use fusor_async::{
    AsyncValue, browser,
    fetch::{self, FetchError},
};
use wasm_bindgen::prelude::*;

struct App {
    selected: Signal<String>,
    visible: Signal<bool>,
    clicked: Signal<String>,
    retained: AsyncValue<String, String, FetchError>,
    show_retained: Signal<bool>,
}
impl App {
    fn new(owner: OwnerHandle) -> Self {
        Self {
            selected: signal("A".into()),
            visible: signal(true),
            clicked: signal(String::new()),
            retained: browser::read(
                &owner,
                || "retained".to_owned(),
                |_, cancel| async move { fetch::get_text("/api/retained/value", &cancel).await },
            ),
            show_retained: signal(false),
        }
    }
}
struct ReadPanel {
    read: AsyncValue<String, String, FetchError>,
    clicked: Signal<String>,
}
struct ReadPanelInputs {
    selected: Signal<String>,
    kind: &'static str,
    clicked: Signal<String>,
}
impl FromInputs for ReadPanel {
    type Inputs = ReadPanelInputs;
    fn from_inputs(inputs: Self::Inputs, owner: OwnerHandle) -> Result<Self, JsValue> {
        Ok(Self {
            read: browser::read(
                &owner,
                move || inputs.selected.get(),
                move |key, cancel| async move {
                    fetch::get_text(&format!("/api/{}/{key}", inputs.kind), &cancel).await
                },
            ),
            clicked: inputs.clicked,
        })
    }
}
#[derive(FromInputs)]
struct Retained {
    #[input]
    value: AsyncValue<String, String, FetchError>,
}
#[derive(FromInputs)]
struct Panel;
#[wasm_bindgen]
pub fn stop() -> Result<(), JsValue> {
    fusor::dom::application::unmount()
}
fusor::template!("web/index.html");

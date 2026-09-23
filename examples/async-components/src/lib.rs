use fusor::prelude::*;
use fusor_async::{AsyncValue, browser};
use wasm_bindgen::prelude::*;

struct App {
    selected: Signal<String>,
    visible: Signal<bool>,
    clicked: Signal<String>,
    retained: AsyncValue<String, String, String>,
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
                |_, request| async move {
                    request
                        .get_text("/api/retained/value")
                        .await
                        .map_err(|e| format!("{e:?}"))
                },
            ),
            show_retained: signal(false),
        }
    }
}
struct ReadPanel {
    read: AsyncValue<String, String, String>,
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
                move |key, request| async move {
                    request
                        .get_text(&format!("/api/{}/{key}", inputs.kind))
                        .await
                        .map_err(|e| format!("{e:?}"))
                },
            ),
            clicked: inputs.clicked,
        })
    }
}
#[derive(FromInputs)]
struct Retained {
    #[input]
    value: AsyncValue<String, String, String>,
}
#[derive(FromInputs)]
struct Panel;
#[wasm_bindgen]
pub fn stop() -> Result<(), JsValue> {
    fusor::dom::application::unmount()
}
fusor::template!("web/index.html");

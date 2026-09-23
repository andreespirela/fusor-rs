use crate::chart::ChartPanel;
use fusor::{Signal, signal};
use wasm_bindgen::JsValue;

struct App {
    count: Signal<u32>,
    result: Signal<String>,
    frames: Signal<JsValue>,
    play: Signal<bool>,
    finished: Signal<u32>,
    chart_visible: Signal<bool>,
}

impl App {
    fn new() -> Self {
        let frames = js_sys::Array::new();
        for transform in ["translateX(0)", "translateX(160px)"] {
            let frame = js_sys::Object::new();
            js_sys::Reflect::set(&frame, &"transform".into(), &transform.into()).unwrap();
            frames.push(&frame);
        }
        Self {
            count: signal(1),
            result: signal("Loading native JavaScript module…".into()),
            frames: signal(frames.into()),
            play: signal(false),
            finished: signal(0),
            chart_visible: signal(true),
        }
    }

    fn utility_result(&self, event: web_sys::Event) {
        if let Ok(value) = fusor::js::event_detail::<String>(&event) {
            self.result.set(value);
        }
    }
}

fusor::template!("web/index.html");

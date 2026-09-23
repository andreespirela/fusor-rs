//! The Leptos benchmark application: the same section, aggregate output and
//! keyed rows as every other workload, written with Leptos signals, memos and
//! `<For>`. `hydrate` builds the browser bundle; `ssr` builds the native renderer.
use leptos::prelude::*;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Rows,
    Computed,
    Fanout,
    Fanin,
}

impl Mode {
    pub fn parse(mode: &str) -> Self {
        match mode {
            "computed" => Self::Computed,
            "fanout" => Self::Fanout,
            "fanin" => Self::Fanin,
            _ => Self::Rows,
        }
    }
}

/// Reference-counted row signals: a row inserted later needs no reactive owner,
/// and is released with the last list or view that holds it.
#[derive(Clone)]
pub struct Item {
    id: u32,
    value: ArcRwSignal<u32>,
}

impl Item {
    fn new(id: u32) -> Self {
        Self {
            id,
            value: ArcRwSignal::new(id),
        }
    }
}

/// Created inside the application's root owner, which disposes it on unmount.
#[derive(Clone, Copy)]
pub struct Model {
    rows: RwSignal<Vec<Item>>,
    shared: RwSignal<u32>,
    total: Memo<u64>,
    mode: Mode,
}

impl Model {
    pub fn new(n: u32, mode: Mode) -> Self {
        let rows: Vec<Item> = (0..n).map(Item::new).collect();
        let inputs = if mode == Mode::Fanin {
            rows.clone()
        } else {
            Vec::new()
        };
        Self {
            rows: RwSignal::new(rows),
            shared: RwSignal::new(0),
            total: Memo::new(move |_| inputs.iter().map(|item| u64::from(item.value.get())).sum()),
            mode,
        }
    }
}

#[component]
fn Row(item: Item, model: Model) -> impl IntoView {
    let Item { id, value } = item;
    let computed = Memo::new({
        let value = value.clone();
        move |_| value.get() * 2
    });
    let (mode, shared) = (model.mode, model.shared);
    let text = {
        let value = value.clone();
        move || match mode {
            Mode::Computed => computed.get(),
            Mode::Fanout => value.get() + shared.get(),
            _ => value.get(),
        }
    };
    view! {
        <li data-id=id>
            <span class="value">{text}</span>
            <button on:click=move |_| value.update(|value| *value += 1)>"+"</button>
        </li>
    }
}

#[component]
fn App(model: Model) -> impl IntoView {
    let Model {
        rows, total, mode, ..
    } = model;
    view! {
        <section>
            <output id="total">{move || total.get()}</output>
            <ul>
                <For
                    each=move || if mode == Mode::Fanin { Vec::new() } else { rows.get() }
                    key=|item| item.id
                    let:item
                >
                    <Row item model/>
                </For>
            </ul>
        </section>
    }
}

/// Native server rendering as Leptos' server integrations do it: a root owner
/// with the SSR shared context, and the in-order HTML stream, here collected
/// into one completed string. (tachys 0.2.18's synchronous `to_html` writes an
/// extra text node before every `<For>` row, which does not hydrate.)
#[cfg(feature = "ssr")]
pub fn server_render(n: u32) -> String {
    use futures::StreamExt;
    let owner = Owner::new_root(Some(std::sync::Arc::new(
        hydration_context::SsrSharedContext::new(),
    )));
    owner.with(|| {
        let model = Model::new(n, Mode::Rows);
        let stream = view! { <App model/> }.to_html_stream_in_order();
        futures::executor::block_on(stream.collect::<String>())
    })
}

#[cfg(feature = "hydrate")]
mod browser {
    use super::*;
    use leptos::mount::{hydrate_from, mount_to};
    use std::{any::Any, cell::Cell, cell::RefCell, rc::Rc};
    use wasm_bindgen::{JsCast, prelude::*};

    // The model's arena signals belong to the mounted root owner. Dropping the
    // `UnmountHandle` removes the view and disposes that owner.
    thread_local! {
        static APP: RefCell<Option<(Model, Box<dyn Any>)>> = const { RefCell::new(None) };
    }

    fn model() -> Model {
        APP.with(|app| app.borrow().as_ref().expect("mounted benchmark").0)
    }

    fn root() -> web_sys::HtmlElement {
        document()
            .get_element_by_id("app")
            .expect("#app")
            .unchecked_into()
    }

    #[wasm_bindgen]
    pub fn bench_mount(n: u32, mode: String) {
        let mode = Mode::parse(&mode);
        let created = Rc::new(Cell::new(None));
        let slot = created.clone();
        let handle = mount_to(root(), move || {
            let model = Model::new(n, mode);
            slot.set(Some(model));
            view! { <App model/> }
        });
        let model = created.get().expect("model created while mounting");
        APP.with(|app| app.replace(Some((model, Box::new(handle)))));
    }

    #[wasm_bindgen]
    pub fn bench_hydrate(n: u32) {
        let created = Rc::new(Cell::new(None));
        let slot = created.clone();
        let handle = hydrate_from(root(), move || {
            let model = Model::new(n, Mode::Rows);
            slot.set(Some(model));
            view! { <App model/> }
        });
        let model = created.get().expect("model created while hydrating");
        APP.with(|app| app.replace(Some((model, Box::new(handle)))));
    }

    #[wasm_bindgen]
    pub fn bench_unmount() {
        let mounted = APP.with(|app| app.take());
        drop(mounted);
    }

    /// Render effects run as tasks on the wasm-bindgen-futures executor that
    /// `mount_to`/`hydrate_from` install. Awaiting the executor's next tick
    /// runs every task queued before this barrier, including the view updates
    /// and the completion of tasks owned by an unmounted view.
    #[wasm_bindgen]
    pub async fn bench_flush() {
        leptos::task::tick().await;
    }

    #[wasm_bindgen]
    pub fn bench_update(index: usize, value: u32) {
        model()
            .rows
            .with_untracked(|rows| rows[index].value.set(value));
    }

    #[wasm_bindgen]
    pub fn bench_bulk(count: usize) {
        model().rows.with_untracked(|rows| {
            for row in &rows[..count] {
                row.value.update(|value| *value += 1);
            }
        });
    }

    #[wasm_bindgen]
    pub fn bench_insert(index: usize, id: u32) {
        model()
            .rows
            .update(|rows| rows.insert(index, Item::new(id)));
    }

    #[wasm_bindgen]
    pub fn bench_remove(index: usize) {
        model().rows.update(|rows| {
            rows.remove(index);
        });
    }

    #[wasm_bindgen]
    pub fn bench_swap(a: usize, b: usize) {
        model().rows.update(|rows| rows.swap(a, b));
    }

    #[wasm_bindgen]
    pub fn bench_fanout(value: u32) {
        model().shared.set(value);
    }

    #[wasm_bindgen]
    pub fn bench_fanin() {
        model().rows.with_untracked(|rows| {
            for row in rows {
                row.value.update(|value| *value += 1);
            }
        });
    }
}

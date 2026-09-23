use fusor::{OwnerHandle, Signal, signal};
use fusor_async::{Resource, ResourceState, browser::resource};
use gloo_timers::future::TimeoutFuture;
use std::{cell::Cell, rc::Rc};

pub struct Loading {
    selected: Signal<String>,
    data: Resource<String, String, String>,
    fail_next: Rc<Cell<bool>>,
}

impl Loading {
    pub fn new(owner: OwnerHandle) -> Self {
        let selected = signal("Kyoto".to_owned());
        let fail_next = Rc::new(Cell::new(false));
        let key = selected.clone();
        let failure = fail_next.clone();
        let data = resource(
            &owner,
            move || Some(key.get()),
            move |city, request| {
                let fail = failure.replace(false);
                async move {
                    // Demo-only latency. Real data still comes from an HTTP request.
                    TimeoutFuture::new(700).await;
                    if fail {
                        return Err("A simulated connection error. Try again.".into());
                    }
                    request
                        .get_text(&format!("/docs/demo-data/{city}.txt"))
                        .await
                        .map_err(|error| {
                            error.as_string().unwrap_or_else(|| "Request failed".into())
                        })
                }
            },
        );
        Self {
            selected,
            data,
            fail_next,
        }
    }

    fn simulate_error(&self) {
        self.fail_next.set(true);
        self.data.refresh();
    }

    fn status(&self) -> String {
        self.data.with(|state| match state {
            ResourceState::Loading { key, .. } => format!("Loading {key}…"),
            ResourceState::Ready(data) => format!("{0} is ready", data.key),
            ResourceState::Error { error, .. } => error.to_string(),
            _ => "Choose a destination".into(),
        })
    }

    fn title(&self) -> String {
        self.data.with(|state| {
            state
                .data()
                .map_or_else(|| "Your next destination".into(), |data| data.key.clone())
        })
    }

    fn text(&self) -> String {
        self.data.with(|state| {
            state.data().map_or_else(
                || "Field notes will appear here.".into(),
                |data| data.value.to_string(),
            )
        })
    }
}

fusor::bindings!(loading);

use fusor::prelude::*;
use wasm_bindgen::prelude::*;

#[derive(Clone, PartialEq)]
pub struct User {
    pub name: String,
}
#[derive(Clone, PartialEq)]
pub enum Session {
    Checking,
    Guest,
    Authenticated { user: User },
}

#[cfg(target_arch = "wasm32")]
struct App {
    visible: Signal<bool>,
    session: Signal<Session>,
    clicked: Signal<String>,
    items: Signal<Vec<u32>>,
    boundary: fusor::coherence::AsyncBoundary,
    read: fusor_async::AsyncValue<String, String, fusor_async::fetch::FetchError>,
    outer_read: fusor_async::AsyncValue<String, String, fusor_async::fetch::FetchError>,
}
#[cfg(target_arch = "wasm32")]
impl App {
    fn new(owner: OwnerHandle) -> Self {
        let session = signal(Session::Guest);
        let make_read = |kind: &'static str| {
            let selected = session.clone();
            fusor_async::browser::read(
                &owner,
                move || match selected.get() {
                    Session::Authenticated { user } => user.name,
                    _ => "guest".into(),
                },
                move |name, cancel| async move {
                    fusor_async::fetch::get_text(&format!("/api/{name}?kind={kind}"), &cancel).await
                },
            )
        };
        Self {
            read: make_read("coherent"),
            outer_read: make_read("independent"),
            visible: signal(true),
            session,
            clicked: signal(String::new()),
            items: signal(vec![1, 2]),
            boundary: fusor::coherence::AsyncBoundary::coherent(),
        }
    }
    fn login(&self, name: &str) {
        self.session.set(Session::Authenticated {
            user: User { name: name.into() },
        });
    }
}

mod dashboard;
mod shared;
#[cfg(target_arch = "wasm32")]
mod wrapper;
use dashboard::Dashboard;
pub use shared::Shared;
#[cfg(target_arch = "wasm32")]
use wrapper::Wrapper;
#[cfg(target_arch = "wasm32")]
fusor::template!("web/index.html");

#[wasm_bindgen]
pub fn stop() -> Result<(), JsValue> {
    fusor::dom::application::unmount()
}

thread_local! {
    static SHARED_SCOPE: std::cell::RefCell<Option<Scope>> = const { std::cell::RefCell::new(None) };
}
#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
pub fn hydrate_shared(guest: bool) -> Result<(), JsValue> {
    let root = document()?
        .get_element_by_id("shared")
        .ok_or_else(|| JsValue::from_str("missing shared root"))?;
    let state = Shared::new();
    if guest {
        state.session.set(Session::Guest);
    }
    let scope = fusor::dom::delivery::with_root(&root, || state.mount())?;
    SHARED_SCOPE.with(|current| *current.borrow_mut() = Some(scope));
    Ok(())
}

#[cfg(not(target_arch = "wasm32"))]
#[cfg(test)]
mod tests {
    use super::*;
    use fusor_server::{Context, Render};
    #[test]
    fn renders_only_selected_branches_and_escapes_captures() {
        let state = Shared::new();
        let first = state.render(&mut Context::new()).unwrap().into_string();
        assert!(first.contains("Ada"));
        assert!(first.contains("Visible"));
        assert!(!first.contains("Sign in"));
        assert!(first.contains("fusor:branch:2"));
        state.session.set(Session::Authenticated {
            user: User {
                name: "<unsafe>".into(),
            },
        });
        let escaped = state.render(&mut Context::new()).unwrap().into_string();
        assert!(escaped.contains("&lt;unsafe&gt;"));
        state.session.set(Session::Guest);
        state.visible.set(false);
        let second = state.render(&mut Context::new()).unwrap().into_string();
        assert!(second.contains("Sign in"));
        assert!(!second.contains("Ada"));
        assert!(!second.contains("Visible"));
    }
}

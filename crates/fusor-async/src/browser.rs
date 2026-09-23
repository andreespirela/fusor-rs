//! Browser executor and an abortable GET adapter. HTTP caching follows Fetch defaults.
use crate::{RequestContext, Resource};
use fusor::OwnerHandle;
use std::future::Future;
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;
use web_sys::{AbortController, Request, RequestInit, Response};

/// Declare a read participating in the nearest generated coherent region.
pub fn read<K, T, E, F>(
    owner: &OwnerHandle,
    key: impl Fn() -> K + 'static,
    load: impl Fn(K, RequestContext) -> F + 'static,
) -> crate::AsyncValue<K, T, E>
where
    K: Clone + PartialEq + 'static,
    T: 'static,
    E: std::fmt::Display + 'static,
    F: Future<Output = Result<T, E>> + 'static,
{
    crate::AsyncValue::new(owner, key, load, wasm_bindgen_futures::spawn_local)
}

pub fn resource<K, T, E, F>(
    owner: &OwnerHandle,
    key: impl Fn() -> Option<K> + 'static,
    load: impl Fn(K, RequestContext) -> F + 'static,
) -> Resource<K, T, E>
where
    K: Clone + PartialEq + 'static,
    T: 'static,
    E: 'static,
    F: Future<Output = Result<T, E>> + 'static,
{
    Resource::new(owner, key, load, wasm_bindgen_futures::spawn_local)
}

impl RequestContext {
    /// GET and fully read a UTF-8 response body. Rejects non-2xx responses.
    /// The AbortController stays registered through body consumption.
    pub async fn get_text(&self, url: &str) -> Result<String, JsValue> {
        if self.is_cancelled() {
            return Err(JsValue::from_str("request cancelled"));
        }
        let controller = AbortController::new()?;
        let options = RequestInit::new();
        options.set_method("GET");
        options.set_signal(Some(&controller.signal()));
        let request = Request::new_with_str_and_init(url, &options)?;
        let _cancel = self.on_cancel(move || controller.abort());
        let window =
            web_sys::window().ok_or_else(|| JsValue::from_str("Fetch requires a browser"))?;
        let response: Response = JsFuture::from(window.fetch_with_request(&request))
            .await?
            .dyn_into()?;
        if !response.ok() {
            return Err(JsValue::from_str(&format!(
                "GET {url}: HTTP {}",
                response.status()
            )));
        }
        JsFuture::from(response.text()?)
            .await?
            .as_string()
            .ok_or_else(|| JsValue::from_str("Fetch returned a non-text body"))
    }
}

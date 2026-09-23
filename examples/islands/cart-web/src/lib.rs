#[cfg(target_arch = "wasm32")]
fusor_islands::export!(
    fusor_islands::browser::Unit::new().entry::<catalog_types::Cart, catalog_views::CartView>(
        |_, props| catalog_views::CartView::new(props)
    )
);
fusor::bindings!(app);
include!(env!("FUSOR_MODULE"));

// Consumer fixture for the public Rust control API. Polling then dropping is
// exactly what an abandoned application future does; no JS cancellation shim.
#[cfg(target_arch = "wasm32")]
#[wasm_bindgen::prelude::wasm_bindgen]
pub fn exercise_waiter(dispose_owner: bool) -> Result<(), wasm_bindgen::JsValue> {
    use std::{
        future::Future,
        task::{Context, Poll, Waker},
    };
    let owner = fusor::Owner::new();
    let handle =
        fusor_islands::browser::get::<catalog_types::Designer>(&owner.handle(), "designer")
            .map_err(|error| wasm_bindgen::JsValue::from_str(&error.to_string()))?;
    owner.commit();
    let mut future = std::pin::pin!(handle.activate());
    if !matches!(
        future
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop())),
        Poll::Pending
    ) {
        return Err(wasm_bindgen::JsValue::from_str(
            "expected deferred activation",
        ));
    }
    if dispose_owner {
        owner.dispose();
    }
    Ok(())
}

/// Exercise a completed request through the public typed Rust facade.
#[cfg(target_arch = "wasm32")]
#[wasm_bindgen::prelude::wasm_bindgen]
pub async fn exercise_control(prefetch: bool) -> Result<(), wasm_bindgen::JsValue> {
    let owner = fusor::Owner::new();
    let target =
        fusor_islands::browser::get::<catalog_types::Designer>(&owner.handle(), "designer")
            .map_err(|error| wasm_bindgen::JsValue::from_str(&error.to_string()))?;
    owner.commit();
    let result = if prefetch {
        target.prefetch().await
    } else {
        target.activate().await
    };
    result.map_err(|error| wasm_bindgen::JsValue::from_str(&error.to_string()))
    // Dropping this caller leaves an activated target owned by its host.
}

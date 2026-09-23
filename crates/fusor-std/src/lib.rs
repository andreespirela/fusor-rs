//! Optional first-party conventions. Core, resources, queries and routing stay
//! independently usable. No features are enabled by default.
#[cfg(feature = "actions")]
pub mod actions;
#[cfg(feature = "forms")]
pub mod forms;
#[cfg(feature = "resources")]
pub use fusor_async as resources;
#[cfg(feature = "query")]
pub use fusor_query as query;
#[cfg(feature = "routing")]
pub use fusor_router as routing;

#[cfg(any(feature = "forms", feature = "actions"))]
mod identity {
    use std::cell::Cell;
    thread_local! { static NEXT: Cell<u64> = const { Cell::new(0) }; }
    pub(crate) fn next() -> u64 {
        NEXT.with(|next| {
            let id = next
                .get()
                .checked_add(1)
                .expect("editing identity overflow");
            next.set(id);
            id
        })
    }
}

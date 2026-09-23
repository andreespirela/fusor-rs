//! A custom memory-backed router using only the public Rust APIs.
use fusor::{
    FromInputs, OwnerHandle,
    dom::{Component, ComponentFactory, Scope, TemplateComponent, document},
};
use fusor_router::{
    AppUrl,
    browser::{
        NavigationDriver, PreparedNavigation,
        declarative::{RouteView, ViewRouter},
    },
};
use std::cell::RefCell;
use wasm_bindgen::prelude::*;

thread_local! {
    static ROUTER: RefCell<Option<ViewRouter>> = const { RefCell::new(None) };
    static STAGE: RefCell<Option<Box<dyn PreparedNavigation>>> = const { RefCell::new(None) };
}
#[derive(FromInputs)]
pub struct Memory;
impl TemplateComponent for Memory {}
impl Component for Memory {
    fn mount(self) -> Result<Scope, JsValue> {
        Self::try_mount_with(|_| Ok(self))
    }
    fn prepare_component(
        parent: Option<&OwnerHandle>,
        make: ComponentFactory<'_, Self>,
    ) -> Result<Scope, JsValue> {
        let root = document()?.create_element("section")?;
        root.set_id("memory");
        let mut scope = Scope::new(root.clone());
        scope.prepare_owner(parent);
        let _ = make(scope.owner())?;
        let target = scope.mount_point(&root)?;
        let router = ViewRouter::mount(
            &mut scope,
            &target,
            vec![RouteView::new("/:page", |owner, matched| {
                let root = document()?.create_element("p")?;
                let page = &matched.params["page"];
                root.set_text_content(Some(page));
                let mut scope = Scope::new(root);
                scope.prepare_owner(Some(owner));
                if page == "fail" {
                    scope.before_commit(|| Err(JsValue::from_str("memory preparation failed")))?;
                }
                Ok(scope)
            })?],
            AppUrl::parse("/first").map_err(|e| JsValue::from_str(&e.to_string()))?,
        )?;
        ROUTER.with(|slot| *slot.borrow_mut() = Some(router));
        scope.retain(scope.owner().on_cleanup(|| {
            STAGE.with(|slot| slot.borrow_mut().take());
            ROUTER.with(|slot| slot.borrow_mut().take());
        }));
        Ok(scope)
    }
}
#[wasm_bindgen]
pub fn memory_stage(path: &str) -> Result<(), JsValue> {
    let router = ROUTER
        .with(|slot| slot.borrow().clone())
        .ok_or_else(|| JsValue::from_str("memory router unmounted"))?;
    let url = AppUrl::parse(path).map_err(|e| JsValue::from_str(&e.to_string()))?;
    let stage = router.prepare_navigation(&url)?;
    STAGE.with(|slot| *slot.borrow_mut() = Some(stage));
    Ok(())
}
#[wasm_bindgen]
pub fn memory_finish(commit: bool) {
    let stage = STAGE.with(|slot| slot.borrow_mut().take());
    if let Some(stage) = stage {
        if commit {
            stage.commit();
        }
    }
}

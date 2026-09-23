#[cfg(not(feature = "grouped"))]
use catalog_types::{Cart, Designer};
#[cfg(feature = "grouped")]
use catalog_types::{GroupedCart as Cart, GroupedDesigner as Designer};
use fusor_islands::DeliveryManifest;
use fusor_server::{Context, Registry, Render};
use std::{env, fs};

struct Page {
    title: String,
}
struct DesignerPreview {
    title: String,
}
fn registry() -> fusor_server::Result<Registry> {
    let mut registry = Registry::new();
    registry.register::<Cart, catalog_views::CartView>(catalog_views::CartView::new)?;
    registry
        .register::<Designer, DesignerPreview>(|props| DesignerPreview { title: props.title })?;
    Ok(registry)
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = env::args().collect();
    let registry = registry()?;
    if args.get(1).is_some_and(|arg| arg == "--fusor-manifest") {
        println!("{}", serde_json::to_string(&registry.witness())?);
    } else if args.get(1).is_some_and(|arg| arg == "--fusor-render") && args.len() == 4 {
        let manifest: DeliveryManifest = serde_json::from_slice(&fs::read(&args[2])?)?;
        let mut context = Context::with_islands(&manifest, &registry)?;
        let html = Page {
            title: "A native HTML catalog".into(),
        }
        .render(&mut context)?;
        fs::write(&args[3], format!("<!doctype html>\n{html}"))?;
    } else {
        return Err(
            "use fusor build -p catalog-site, or --fusor-manifest / --fusor-render MANIFEST OUTPUT"
                .into(),
        );
    }
    Ok(())
}
fusor::bindings!(app);
include!(env!("FUSOR_MODULE"));

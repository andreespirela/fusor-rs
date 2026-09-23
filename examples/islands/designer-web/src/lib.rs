fusor_islands::export!(
    fusor_islands::browser::Unit::new()
        .entry::<catalog_types::Designer, catalog_designer_views::DesignerView>(|_, props| {
            catalog_designer_views::DesignerView::new(props)
        })
);
fusor::bindings!(app);
include!(env!("FUSOR_MODULE"));

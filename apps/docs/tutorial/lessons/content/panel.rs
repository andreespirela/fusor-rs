use fusor::prelude::*;

#[derive(FromInputs)]
pub struct Panel;

fusor::template!("web/components/panel.html");

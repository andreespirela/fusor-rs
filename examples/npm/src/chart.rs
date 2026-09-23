use fusor::{FromInputs, JsInputs, Signal};

#[derive(FromInputs, JsInputs)]
pub struct ChartPanel {
    #[input]
    #[js]
    count: Signal<u32>,
}

fusor::template!("web/components/chart.html");

mod report;
use fusor::{OwnerHandle, Signal, signal};
use fusor_async::{AsyncBoundary, AsyncValue, browser};
use report::{MemoryRow, MetricRow, Report};
struct App {
    view: AsyncBoundary,
    report: AsyncValue<(), Report, String>,
    category: Signal<String>,
    percentile: Signal<bool>,
}
impl App {
    fn new(owner: OwnerHandle) -> Self {
        Self {
            view: AsyncBoundary::coherent(),
            report: browser::read(
                &owner,
                || (),
                |(), context| async move {
                    let text = context
                        .get_text("/benchmarks/results.json")
                        .await
                        .map_err(|error| format!("{error:?}"))?;
                    let report: Report =
                        serde_json::from_str(&text).map_err(|error| error.to_string())?;
                    if report.schema != 1 {
                        return Err("Unsupported report version".into());
                    }
                    Ok(report)
                },
            ),
            category: signal("all".into()),
            percentile: signal(false),
        }
    }
    fn status(&self) -> String {
        use fusor_async::BoundaryStatus;
        match self.view.status() {
            BoundaryStatus::Error(error) | BoundaryStatus::Faulted(error) => {
                format!("The report could not load: {error}")
            }
            BoundaryStatus::Ready => String::new(),
            _ => "Loading the recorded measurements…".into(),
        }
    }
}
struct Row {
    item: fusor::Memo<MetricRow>,
    percentile: Signal<bool>,
}
impl Row {
    fn value(&self, index: usize) -> String {
        self.item.get().display(index, self.percentile.get())
    }
    fn operation(&self, index: usize) -> String {
        self.item
            .with(|row| row.operations[index].clone().unwrap_or_default())
    }
    fn best(&self, index: usize) -> bool {
        self.item.get().best(index, self.percentile.get())
    }
}
struct Memory {
    item: fusor::Memo<MemoryRow>,
}
fusor::bindings!(app);
include!(env!("FUSOR_MODULE"));

impl fusor::dom::FromInputs for Row {
    type Inputs = Self;
    fn from_inputs(inputs: Self, _owner: fusor::OwnerHandle) -> Result<Self, fusor::dom::JsValue> {
        Ok(inputs)
    }
}

impl fusor::dom::FromInputs for Memory {
    type Inputs = Self;
    fn from_inputs(inputs: Self, _owner: fusor::OwnerHandle) -> Result<Self, fusor::dom::JsValue> {
        Ok(inputs)
    }
}

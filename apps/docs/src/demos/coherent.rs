use fusor::{OwnerHandle, Signal, signal};
use fusor_async::{AsyncBoundary, AsyncValue, BoundaryStatus, browser};
use gloo_timers::future::TimeoutFuture;

pub struct Coherent {
    product: Signal<String>,
    view: AsyncBoundary,
}

impl Coherent {
    pub fn new() -> Self {
        Self {
            product: signal("Notebook".into()),
            view: AsyncBoundary::coherent(),
        }
    }

    fn status(&self) -> String {
        match self.view.status() {
            BoundaryStatus::Pending => {
                format!("Preparing {} — waiting for both reads…", self.product.get())
            }
            BoundaryStatus::Ready => "Complete view published".into(),
            BoundaryStatus::Error(error) | BoundaryStatus::Faulted(error) => {
                format!("Could not load: {error}")
            }
            _ => "Preparing the first view…".into(),
        }
    }
}

struct ProductRead {
    value: AsyncValue<String, String, String>,
    label: &'static str,
}

impl ProductRead {
    fn new(
        owner: OwnerHandle,
        product: Signal<String>,
        field: &'static str,
        delay: u32,
        label: &'static str,
    ) -> Self {
        let value = browser::read(
            &owner,
            move || product.get(),
            move |key, request| async move {
                TimeoutFuture::new(delay).await;
                request
                    .get_text(&format!("/docs/demo-data/{key}-{field}.txt"))
                    .await
                    .map_err(|error| error.as_string().unwrap_or_else(|| "Request failed".into()))
            },
        );
        Self { value, label }
    }
}

fusor::bindings!(coherent);

struct ProductReadInputs {
    pub product: Signal<String>,
    pub field: &'static str,
    pub delay: u32,
    pub label: &'static str,
}
impl fusor::dom::FromInputs for ProductRead {
    type Inputs = ProductReadInputs;
    fn from_inputs(
        inputs: Self::Inputs,
        owner: fusor::OwnerHandle,
    ) -> Result<Self, fusor::dom::JsValue> {
        Ok(Self::new(
            owner,
            inputs.product,
            inputs.field,
            inputs.delay,
            inputs.label,
        ))
    }
}

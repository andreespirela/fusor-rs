use fusor::prelude::*;
use fusor_async::{AsyncValue, browser};
use gloo_timers::future::TimeoutFuture;

#[derive(FromInputs)]
pub struct AsyncData {
    #[local(init = signal(12_u32))]
    issue: Signal<u32>,
}

struct IssueField {
    value: AsyncValue<u32, String, String>,
    field: &'static str,
}

pub struct IssueFieldInputs {
    pub issue: Signal<u32>,
    pub field: &'static str,
    pub delay: u32,
}

impl FromInputs for IssueField {
    type Inputs = IssueFieldInputs;

    fn from_inputs(inputs: Self::Inputs, owner: OwnerHandle) -> Result<Self, fusor::dom::JsValue> {
        let field = inputs.field;
        let value = browser::read(
            &owner,
            move || inputs.issue.get(),
            move |issue, request| async move {
                TimeoutFuture::new(inputs.delay).await;
                request
                    .get_text(&format!("./data/issues/{issue}-{field}.txt"))
                    .await
                    .map(|value| value.trim().to_owned())
                    .map_err(|error| format!("{error:?}"))
            },
        );
        Ok(Self { value, field })
    }
}

fusor::template!("web/components/async_data.html");

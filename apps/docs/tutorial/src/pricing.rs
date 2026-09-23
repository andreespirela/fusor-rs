use fusor::{OwnerHandle, Signal};
use fusor_async::{AsyncValue, browser};

pub struct Price {
    quote: AsyncValue<String, String, String>,
}

impl Price {
    pub fn new(owner: OwnerHandle, product: Signal<String>) -> Self {
        Self {
            quote: browser::read(
                &owner,
                move || product.get(),
                |key, request| async move {
                    request
                        .get_text(&format!("/data/price/{key}.txt"))
                        .await
                        .map_err(|error| format!("{error:?}"))
                },
            ),
        }
    }
}

pub struct Stock {
    stock: AsyncValue<String, String, String>,
}

impl Stock {
    pub fn new(owner: OwnerHandle, product: Signal<String>) -> Self {
        Self {
            stock: browser::read(
                &owner,
                move || product.get(),
                |key, request| async move {
                    request
                        .get_text(&format!("/data/stock/{key}.txt"))
                        .await
                        .map_err(|error| format!("{error:?}"))
                },
            ),
        }
    }
}

fusor::bindings!(pricing);

pub struct PriceInputs {
    pub product: Signal<String>,
}
impl fusor::dom::FromInputs for Price {
    type Inputs = PriceInputs;
    fn from_inputs(
        inputs: Self::Inputs,
        owner: fusor::OwnerHandle,
    ) -> Result<Self, fusor::dom::JsValue> {
        Ok(Self::new(owner, inputs.product))
    }
}

pub struct StockInputs {
    pub product: Signal<String>,
}
impl fusor::dom::FromInputs for Stock {
    type Inputs = StockInputs;
    fn from_inputs(
        inputs: Self::Inputs,
        owner: fusor::OwnerHandle,
    ) -> Result<Self, fusor::dom::JsValue> {
        Ok(Self::new(owner, inputs.product))
    }
}

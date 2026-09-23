use fusor_islands::{Island, RenderMode};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct CartProps {
    pub product_id: u64,
    pub title: String,
    pub quantity: String,
}
pub struct Cart;
impl Island for Cart {
    type Props = CartProps;
    const NAME: &'static str = "catalog.cart";
    const UNIT: &'static str = "cart";
    const SCHEMA: &'static str = "catalog.cart.props.v1";
}

#[derive(Serialize, Deserialize)]
pub struct DesignerProps {
    pub product_id: u64,
    pub title: String,
}
pub struct Designer;
impl Island for Designer {
    type Props = DesignerProps;
    const NAME: &'static str = "catalog.designer";
    const UNIT: &'static str = "designer";
    const SCHEMA: &'static str = "catalog.designer.props.v1";
    const MODE: RenderMode = RenderMode::Preview;
}

/// The same contracts can be grouped into one delivery unit. This fixture makes
/// the cost of duplicated runtime code measurable without changing view logic.
pub struct GroupedCart;
impl Island for GroupedCart {
    type Props = CartProps;
    const NAME: &'static str = Cart::NAME;
    const UNIT: &'static str = "grouped";
    const SCHEMA: &'static str = Cart::SCHEMA;
}
pub struct GroupedDesigner;
impl Island for GroupedDesigner {
    type Props = DesignerProps;
    const NAME: &'static str = Designer::NAME;
    const UNIT: &'static str = "grouped";
    const SCHEMA: &'static str = Designer::SCHEMA;
    const MODE: RenderMode = RenderMode::Preview;
}

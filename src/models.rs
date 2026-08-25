use rust_decimal::Decimal;

#[derive(Debug, Clone)]
pub struct Contract {
    pub conid: i64,
    pub symbol: String,
    pub sec_type: String,
    pub exchange: String,
    pub currency: String,
}

#[derive(Debug)]
pub struct Account {
    pub account_id: String,
    pub account_type: String,
    pub currency: String,
    pub status: String,
}

#[derive(Debug)]
pub struct NewOrder {
    pub order_id: i64,
    pub account_id: String,
    pub conid: i64,
    pub side: String,
    pub order_type: String,
    pub quantity: Decimal,
    pub lmt_price: Option<Decimal>,
    pub aux_price: Option<Decimal>,
}

#[derive(Debug)]
pub struct Order {
    pub order_id: i64,
    pub perm_id: Option<i64>,
    pub account_id: String,
    pub conid: i64,
    pub side: String,
    pub order_type: String,
    pub total_quantity: Decimal,
    pub filled_quantity: Decimal,
    pub lmt_price: Option<Decimal>,
    pub aux_price: Option<Decimal>,
    pub status: String,
}

#[derive(Debug)]
pub struct NewFill {
    pub exec_id: String,
    pub order_id: i64,
    pub account_id: String,
    pub price: Decimal,
}

#[derive(Debug)]
pub struct Position {
    pub account_id: String,
    pub conid: i64,
    pub position: Decimal,
    pub avg_cost: Option<Decimal>,
}

#[derive(Debug)]
pub struct CashBalance {
    pub account_id: String,
    pub currency: String,
    pub cash: Decimal,
}

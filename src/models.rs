//! Domain structs shared by the CLI, the database layer and the HTTP API.
//!
//! Money and quantity fields are `Decimal`, never `f64`: the database stores
//! six-decimal fixed-point micro-units, so a binary float would not
//! round-trip. The API serializes these types directly (see `trading::wire`)
//! rather than keeping parallel response structs.

use rust_decimal::Decimal;
use serde::{Serialize, Serializer};

fn serialize_decimal<S: Serializer>(value: &Decimal, serializer: S) -> Result<S::Ok, S::Error> {
    // `normalize()` drops trailing zeros so the UI renders `185.52` and not
    // `185.520000`, without ever going through binary floating point.
    serializer.serialize_str(&value.normalize().to_string())
}

fn serialize_optional_decimal<S: Serializer>(
    value: &Option<Decimal>,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    match value {
        Some(price) => serialize_decimal(price, serializer),
        None => serializer.serialize_none(),
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Contract {
    pub conid: i64,
    pub symbol: String,
    pub sec_type: String,
    pub exchange: String,
    pub currency: String,
}

#[derive(Debug, Serialize)]
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

#[derive(Debug, Serialize)]
pub struct Order {
    pub order_id: i64,
    pub perm_id: Option<i64>,
    pub account_id: String,
    pub conid: i64,
    pub side: String,
    pub order_type: String,
    #[serde(serialize_with = "serialize_decimal")]
    pub total_quantity: Decimal,
    #[serde(serialize_with = "serialize_decimal")]
    pub filled_quantity: Decimal,
    #[serde(serialize_with = "serialize_optional_decimal")]
    pub lmt_price: Option<Decimal>,
    #[serde(serialize_with = "serialize_optional_decimal")]
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

#[derive(Debug, Serialize)]
pub struct Position {
    pub account_id: String,
    pub conid: i64,
    #[serde(serialize_with = "serialize_decimal")]
    pub position: Decimal,
    #[serde(serialize_with = "serialize_optional_decimal")]
    pub avg_cost: Option<Decimal>,
}

#[derive(Debug, Serialize)]
pub struct CashBalance {
    pub account_id: String,
    pub currency: String,
    #[serde(serialize_with = "serialize_decimal")]
    pub cash: Decimal,
}

#[derive(Debug, Serialize)]
pub struct Fill {
    pub exec_id: String,
    pub order_id: i64,
    pub account_id: String,
    pub conid: i64,
    pub side: String,
    #[serde(serialize_with = "serialize_decimal")]
    pub quantity: Decimal,
    #[serde(serialize_with = "serialize_decimal")]
    pub price: Decimal,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    #[test]
    fn decimals_serialize_as_normalized_strings() {
        let order = Order {
            order_id: 7,
            perm_id: None,
            account_id: "SIM1".into(),
            conid: 265598,
            side: "BUY".into(),
            order_type: "LMT".into(),
            total_quantity: Decimal::from_str("100.000000").unwrap(),
            filled_quantity: Decimal::from_str("0").unwrap(),
            lmt_price: Some(Decimal::from_str("185.520000").unwrap()),
            aux_price: None,
            status: "Submitted".into(),
        };
        let json = serde_json::to_string(&order).expect("serialize order");
        assert!(json.contains("\"total_quantity\":\"100\""), "{json}");
        assert!(json.contains("\"filled_quantity\":\"0\""), "{json}");
        assert!(json.contains("\"lmt_price\":\"185.52\""), "{json}");
        assert!(json.contains("\"aux_price\":null"), "{json}");
        assert!(json.contains("\"perm_id\":null"), "{json}");
    }

    #[test]
    fn negative_positions_keep_their_sign() {
        let position = Position {
            account_id: "SIM1".into(),
            conid: 1,
            position: Decimal::from_str("-12.5").unwrap(),
            avg_cost: Some(Decimal::from_str("3.000000").unwrap()),
        };
        let json = serde_json::to_string(&position).expect("serialize position");
        assert!(json.contains("\"position\":\"-12.5\""), "{json}");
        assert!(json.contains("\"avg_cost\":\"3\""), "{json}");
    }
}

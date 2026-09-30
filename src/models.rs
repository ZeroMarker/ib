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

    /// The checked-in wire contract, shared with the frontend.
    const CONTRACT: &str = include_str!("../api-schema.json");

    /// Serialize `value` and return its field names, sorted.
    ///
    /// Sorted rather than in declaration order: `serde_json::Value` is backed
    /// by a `Map` that does not preserve insertion order, and a hand-edited
    /// JSON object has no meaningful key order either. The set of names is the
    /// contract; the order is not.
    fn field_names<T: Serialize>(value: &T) -> Vec<String> {
        let mut names: Vec<String> = serde_json::to_value(value)
            .expect("serialize")
            .as_object()
            .expect("a struct must serialize to an object")
            .keys()
            .cloned()
            .collect();
        names.sort();
        names
    }

    /// The `types` object, which holds every entry. The `"//"` key next to it
    /// is a comment for humans reading the file and is skipped by addressing
    /// `types` directly.
    fn contract() -> serde_json::Map<String, serde_json::Value> {
        let parsed: serde_json::Value =
            serde_json::from_str(CONTRACT).expect("api-schema.json must be valid JSON");
        parsed
            .get("types")
            .expect("api-schema.json must have a top-level `types` object")
            .as_object()
            .expect("`types` must be an object")
            .clone()
    }

    /// Read the expected field list for `name` out of the contract file, sorted
    /// to match [`field_names`].
    fn contract_fields(name: &str) -> Vec<String> {
        let mut names: Vec<String> = contract()
            .get(name)
            .unwrap_or_else(|| panic!("{name} is missing from api-schema.json"))
            .as_object()
            .expect("a contract entry must be an object of field: type")
            .keys()
            .cloned()
            .collect();
        names.sort();
        names
    }

    fn assert_contract(name: &str, actual: Vec<String>) {
        let expected = contract_fields(name);
        assert_eq!(
            actual, expected,
            "{name} does not match api-schema.json. Update the struct and the contract together, \
             then update frontend/src/types.ts if the field names changed."
        );
    }

    // The overview envelope is serialized in `trading`, not here, so its own
    // contract is asserted by the integration test that reads the live
    // endpoint. What matters here is that the model types the frontend imports
    // still match the contract.

    /// Every serialized struct must match `api-schema.json` field for field.
    ///
    /// Without this, adding or renaming a field on the Rust side compiles and
    /// passes every other test while the frontend's `types.ts` keeps a stale
    /// view of the payload -- the drift this project already had once
    /// (`Position`, `Cash` and `Fill` were strict subsets of what the server
    /// sent). The frontend derives its shapes from the same file, so a mismatch
    /// now fails here first.
    #[test]
    fn the_wire_contract_matches_this_file() {
        let decimal = Decimal::from_str("1.5").unwrap();

        assert_contract(
            "Contract",
            field_names(&Contract {
                conid: 1,
                symbol: "AAPL".into(),
                sec_type: "STK".into(),
                exchange: "SMART".into(),
                currency: "USD".into(),
            }),
        );
        assert_contract(
            "Account",
            field_names(&Account {
                account_id: "SIM1".into(),
                account_type: "MARGIN".into(),
                currency: "USD".into(),
                status: "ACTIVE".into(),
            }),
        );
        assert_contract(
            "Order",
            field_names(&Order {
                order_id: 1,
                perm_id: None,
                account_id: "SIM1".into(),
                conid: 2,
                side: "BUY".into(),
                order_type: "LMT".into(),
                total_quantity: decimal,
                filled_quantity: decimal,
                lmt_price: Some(decimal),
                aux_price: None,
                status: "Submitted".into(),
            }),
        );
        assert_contract(
            "Position",
            field_names(&Position {
                account_id: "SIM1".into(),
                conid: 1,
                position: decimal,
                avg_cost: Some(decimal),
            }),
        );
        assert_contract(
            "CashBalance",
            field_names(&CashBalance {
                account_id: "SIM1".into(),
                currency: "USD".into(),
                cash: decimal,
            }),
        );
        assert_contract(
            "Fill",
            field_names(&Fill {
                exec_id: "EX1".into(),
                order_id: 1,
                account_id: "SIM1".into(),
                conid: 2,
                side: "BUY".into(),
                quantity: decimal,
                price: decimal,
            }),
        );
    }

    /// `auth::UserResponse` is a second serialized struct outside this module.
    #[test]
    fn the_user_response_matches_the_contract() {
        let expected = contract_fields("UserResponse");
        let mut actual: Vec<String> = vec![
            "user_id".to_string(),
            "email".to_string(),
            "email_verified".to_string(),
        ];
        actual.sort();
        assert_eq!(
            actual, expected,
            "auth::UserResponse no longer matches api-schema.json. Its fields are private, so \
             this test asserts the list literally -- if you changed the struct, change both."
        );
    }
}

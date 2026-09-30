use crate::{
    auth::{self, AppState},
    db::{self, Pool},
    http::{run_db, ApiError, ApiResult},
    models::{Account, CashBalance, Contract, Fill, NewFill, NewOrder, Order, Position},
};
use axum::{
    extract::{Json, Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Router,
};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// The overview is the one aggregate the UI loads; the individual endpoints
/// return the same models directly.
#[derive(Debug, Serialize)]
struct OverviewResponse {
    account: Account,
    contracts: Vec<Contract>,
    orders: Vec<Order>,
    positions: Vec<Position>,
    cash: Vec<CashBalance>,
    fills: Vec<Fill>,
}

#[derive(Debug, Deserialize)]
struct OrderRequest {
    conid: i64,
    side: String,
    order_type: String,
    quantity: String,
    lmt_price: Option<String>,
    aux_price: Option<String>,
}

#[derive(Debug, Deserialize)]
struct FillRequest {
    price: String,
    exec_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CashRequest {
    currency: String,
    amount: String,
}

#[derive(Debug, Deserialize)]
struct ContractRequest {
    conid: i64,
    symbol: String,
    sec_type: String,
    exchange: String,
    currency: String,
}

pub(crate) fn router() -> Router<AppState> {
    Router::new()
        .route("/api/trading/overview", get(overview))
        .route(
            "/api/trading/contracts",
            get(contracts).post(create_contract),
        )
        .route("/api/trading/orders", get(orders).post(place_order))
        .route("/api/trading/orders/{order_id}/cancel", post(cancel_order))
        .route("/api/trading/orders/{order_id}/fill", post(fill_order))
        .route("/api/trading/positions", get(positions))
        .route("/api/trading/cash", get(cash).post(set_cash))
        .route("/api/trading/fills", get(fills))
}

/// Acquire a pooled connection, authenticate the caller and ensure their
/// simulation account exists. Call from inside `run_db` closures only.
fn open_account_connection<'a>(
    pool: &'a Pool,
    headers: &HeaderMap,
) -> ApiResult<(std::sync::MutexGuard<'a, rusqlite::Connection>, String)> {
    let conn = pool
        .get()
        .map_err(|_| ApiError::Unavailable("database unavailable"))?;
    let user = auth::current_user(&conn, headers)
        .map_err(|_| ApiError::Unauthenticated("authentication required"))?;
    let account_id = db::ensure_user_account(&conn, &user.user_id)
        .map_err(|_| ApiError::Internal("simulation account unavailable"))?;
    Ok((conn, account_id))
}

fn parse_decimal(value: &str, field: &str) -> ApiResult<Decimal> {
    value
        .parse::<Decimal>()
        .map_err(|_| ApiError::Invalid(format!("invalid {field}")))
}

async fn overview(State(state): State<AppState>, headers: HeaderMap) -> Response {
    run_db(state, move |pool| {
        let (conn, account_id) = open_account_connection(pool, &headers)?;
        let account = db::get_account(&conn, &account_id)
            .ok_or(ApiError::Internal("simulation account missing"))?;
        Ok(Json(OverviewResponse {
            account,
            contracts: db::list_contracts(&conn).into_iter().collect(),
            orders: db::list_account_orders(&conn, &account_id, None)
                .into_iter()
                .collect(),
            positions: db::list_positions(&conn, Some(&account_id))
                .into_iter()
                .collect(),
            cash: db::list_cash(&conn, Some(&account_id))
                .into_iter()
                .collect(),
            fills: db::list_fills(&conn, &account_id).into_iter().collect(),
        })
        .into_response())
    })
    .await
}

async fn contracts(State(state): State<AppState>, headers: HeaderMap) -> Response {
    run_db(state, move |pool| {
        let (conn, _) = open_account_connection(pool, &headers)?;
        Ok(Json(db::list_contracts(&conn)).into_response())
    })
    .await
}

async fn create_contract(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<ContractRequest>,
) -> Response {
    run_db(state, move |pool| {
        let (conn, _) = open_account_connection(pool, &headers)?;
        let symbol = request.symbol.trim().to_uppercase();
        let sec_type = request.sec_type.trim().to_uppercase();
        let exchange = request.exchange.trim().to_uppercase();
        let currency = request.currency.trim().to_uppercase();
        if request.conid <= 0
            || symbol.is_empty()
            || currency.len() != 3
            || !currency.chars().all(|c| c.is_ascii_alphabetic())
        {
            return Err(ApiError::Invalid("invalid contract".into()));
        }
        let contract = Contract {
            conid: request.conid,
            symbol,
            sec_type,
            exchange,
            currency,
        };
        db::add_contract(&conn, &contract)
            .map_err(|_| ApiError::Conflict("contract already exists"))?;
        Ok((StatusCode::CREATED, Json(contract)).into_response())
    })
    .await
}

async fn orders(State(state): State<AppState>, headers: HeaderMap) -> Response {
    run_db(state, move |pool| {
        let (conn, account_id) = open_account_connection(pool, &headers)?;
        Ok(Json(db::list_account_orders(&conn, &account_id, None)).into_response())
    })
    .await
}

async fn place_order(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<OrderRequest>,
) -> Response {
    run_db(state, move |pool| {
        let (conn, account_id) = open_account_connection(pool, &headers)?;
        let quantity = parse_decimal(&request.quantity, "quantity")?;
        let lmt_price = request
            .lmt_price
            .as_deref()
            .map(|value| parse_decimal(value, "limit price"))
            .transpose()?;
        let aux_price = request
            .aux_price
            .as_deref()
            .map(|value| parse_decimal(value, "stop price"))
            .transpose()?;
        if !db::contract_exists(&conn, request.conid) {
            return Err(ApiError::Invalid("contract not found".into()));
        }
        // Validate before allocating the order ID so invalid requests never
        // take the per-account row lock that serializes ID assignment.
        let candidate = NewOrder {
            order_id: 0,
            account_id: account_id.clone(),
            conid: request.conid,
            side: request.side.to_uppercase(),
            order_type: request.order_type.to_uppercase(),
            quantity,
            lmt_price,
            aux_price,
        };
        db::validate_order(&candidate).map_err(ApiError::Invalid)?;
        let order_id = db::next_order_id(&conn, &account_id)
            .map_err(|_| ApiError::Internal("could not allocate order ID"))?;
        let order = NewOrder {
            order_id,
            ..candidate
        };
        db::place_order(&conn, &order).map_err(ApiError::Invalid)?;
        Ok(Json(Order {
            order_id,
            perm_id: None,
            account_id,
            conid: order.conid,
            side: order.side,
            order_type: order.order_type,
            total_quantity: order.quantity,
            filled_quantity: Decimal::ZERO,
            lmt_price: order.lmt_price,
            aux_price: order.aux_price,
            status: "Submitted".into(),
        })
        .into_response())
    })
    .await
}

async fn cancel_order(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(order_id): Path<i64>,
) -> Response {
    run_db(state, move |pool| {
        let (conn, account_id) = open_account_connection(pool, &headers)?;
        // cancel_order reports non-cancellable orders itself; no scan needed.
        db::cancel_order(&conn, order_id, &account_id)
            .map_err(|_| ApiError::Conflict("order is not cancellable"))?;
        Ok((StatusCode::NO_CONTENT, ()).into_response())
    })
    .await
}

async fn fill_order(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(order_id): Path<i64>,
    Json(request): Json<FillRequest>,
) -> Response {
    run_db(state, move |pool| {
        let (conn, account_id) = open_account_connection(pool, &headers)?;
        let price = parse_decimal(&request.price, "fill price")?;
        let exec_id = request.exec_id.unwrap_or_else(|| {
            let compact = Uuid::new_v4().simple().to_string();
            format!("WEB{}", &compact[..21])
        });
        let fill = NewFill {
            exec_id,
            order_id,
            account_id,
            price,
        };
        // record_fill already distinguishes the retryable (idempotent
        // same-exec-id) case from a genuine state conflict.
        db::record_fill(&conn, &fill)
            .map_err(|_| ApiError::Conflict("order cannot be filled in its current state"))?;
        Ok((StatusCode::NO_CONTENT, ()).into_response())
    })
    .await
}

async fn positions(State(state): State<AppState>, headers: HeaderMap) -> Response {
    run_db(state, move |pool| {
        let (conn, account_id) = open_account_connection(pool, &headers)?;
        Ok(Json(db::list_positions(&conn, Some(&account_id))).into_response())
    })
    .await
}

async fn cash(State(state): State<AppState>, headers: HeaderMap) -> Response {
    run_db(state, move |pool| {
        let (conn, account_id) = open_account_connection(pool, &headers)?;
        Ok(Json(db::list_cash(&conn, Some(&account_id))).into_response())
    })
    .await
}

async fn set_cash(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<CashRequest>,
) -> Response {
    run_db(state, move |pool| {
        let (conn, account_id) = open_account_connection(pool, &headers)?;
        let amount = parse_decimal(&request.amount, "cash amount")?;
        let currency = request.currency.trim().to_uppercase();
        if currency.len() != 3 || !currency.chars().all(|c| c.is_ascii_alphabetic()) {
            return Err(ApiError::Invalid("currency must be a 3-letter code".into()));
        }
        // A deposit endpoint must not accept a negative amount. The form in
        // `useTrading.setCash` already blocks this, but the check belongs to
        // the server too: the browser check is a convenience, not a control.
        if amount <= Decimal::ZERO {
            return Err(ApiError::Invalid("cash amount must be positive".into()));
        }
        db::set_cash(&conn, &account_id, &currency, amount).map_err(ApiError::Invalid)?;
        Ok(StatusCode::NO_CONTENT.into_response())
    })
    .await
}

async fn fills(State(state): State<AppState>, headers: HeaderMap) -> Response {
    run_db(state, move |pool| {
        let (conn, account_id) = open_account_connection(pool, &headers)?;
        Ok(Json(db::list_fills(&conn, &account_id)).into_response())
    })
    .await
}

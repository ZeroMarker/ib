use crate::{
    auth::{self, AppState},
    db,
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

#[derive(Debug, Serialize)]
struct ErrorResponse {
    error: String,
}

#[derive(Debug, Serialize)]
struct AccountResponse {
    account_id: String,
    account_type: String,
    currency: String,
    status: String,
}

#[derive(Debug, Serialize)]
struct ContractResponse {
    conid: i64,
    symbol: String,
    sec_type: String,
    exchange: String,
    currency: String,
}

#[derive(Debug, Serialize)]
struct OrderResponse {
    order_id: i64,
    perm_id: Option<i64>,
    account_id: String,
    conid: i64,
    side: String,
    order_type: String,
    total_quantity: String,
    filled_quantity: String,
    lmt_price: Option<String>,
    aux_price: Option<String>,
    status: String,
}

#[derive(Debug, Serialize)]
struct PositionResponse {
    account_id: String,
    conid: i64,
    position: String,
    avg_cost: Option<String>,
}

#[derive(Debug, Serialize)]
struct CashResponse {
    account_id: String,
    currency: String,
    cash: String,
}

#[derive(Debug, Serialize)]
struct FillResponse {
    exec_id: String,
    order_id: i64,
    account_id: String,
    conid: i64,
    side: String,
    quantity: String,
    price: String,
}

#[derive(Debug, Serialize)]
struct OverviewResponse {
    account: AccountResponse,
    contracts: Vec<ContractResponse>,
    orders: Vec<OrderResponse>,
    positions: Vec<PositionResponse>,
    cash: Vec<CashResponse>,
    fills: Vec<FillResponse>,
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

fn error(status: StatusCode, message: impl Into<String>) -> Response {
    (
        status,
        Json(ErrorResponse {
            error: message.into(),
        }),
    )
        .into_response()
}

fn boxed_error(status: StatusCode, message: impl Into<String>) -> Box<Response> {
    Box::new(error(status, message))
}

/// Run blocking Oracle work on the dedicated blocking thread pool so slow
/// queries never occupy a Tokio worker thread.
async fn run_db<F>(state: AppState, task: F) -> Response
where
    F: FnOnce(&oracle::pool::Pool) -> Response + Send + 'static,
{
    match tokio::task::spawn_blocking(move || task(&state.db)).await {
        Ok(response) => response,
        Err(_) => error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "internal server error",
        ),
    }
}

/// Acquire a pooled connection, authenticate the caller and ensure their
/// simulation account exists. Call from inside `run_db` closures only.
fn open_account_connection(
    pool: &oracle::pool::Pool,
    headers: &HeaderMap,
) -> Result<(oracle::Connection, String), Box<Response>> {
    let conn = pool
        .get()
        .map_err(|_| boxed_error(StatusCode::INTERNAL_SERVER_ERROR, "database unavailable"))?;
    let user = auth::current_user(&conn, headers)
        .map_err(|_| boxed_error(StatusCode::UNAUTHORIZED, "authentication required"))?;
    let account_id = db::ensure_user_account(&conn, &user.user_id).map_err(|_| {
        boxed_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "simulation account unavailable",
        )
    })?;
    Ok((conn, account_id))
}

fn parse_decimal(value: &str, field: &str) -> Result<Decimal, Box<Response>> {
    value
        .parse::<Decimal>()
        .map_err(|_| boxed_error(StatusCode::BAD_REQUEST, format!("invalid {field}")))
}

fn decimal(value: &Decimal) -> String {
    value.normalize().to_string()
}

fn optional_decimal(value: &Option<Decimal>) -> Option<String> {
    value.as_ref().map(decimal)
}

fn account_response(account: Account) -> AccountResponse {
    AccountResponse {
        account_id: account.account_id,
        account_type: account.account_type,
        currency: account.currency,
        status: account.status,
    }
}

fn contract_response(contract: Contract) -> ContractResponse {
    ContractResponse {
        conid: contract.conid,
        symbol: contract.symbol,
        sec_type: contract.sec_type,
        exchange: contract.exchange,
        currency: contract.currency,
    }
}

fn order_response(order: Order) -> OrderResponse {
    OrderResponse {
        order_id: order.order_id,
        perm_id: order.perm_id,
        account_id: order.account_id,
        conid: order.conid,
        side: order.side,
        order_type: order.order_type,
        total_quantity: decimal(&order.total_quantity),
        filled_quantity: decimal(&order.filled_quantity),
        lmt_price: optional_decimal(&order.lmt_price),
        aux_price: optional_decimal(&order.aux_price),
        status: order.status,
    }
}

fn position_response(position: Position) -> PositionResponse {
    PositionResponse {
        account_id: position.account_id,
        conid: position.conid,
        position: decimal(&position.position),
        avg_cost: optional_decimal(&position.avg_cost),
    }
}

fn cash_response(balance: CashBalance) -> CashResponse {
    CashResponse {
        account_id: balance.account_id,
        currency: balance.currency,
        cash: decimal(&balance.cash),
    }
}

fn fill_response(fill: Fill) -> FillResponse {
    FillResponse {
        exec_id: fill.exec_id,
        order_id: fill.order_id,
        account_id: fill.account_id,
        conid: fill.conid,
        side: fill.side,
        quantity: decimal(&fill.quantity),
        price: decimal(&fill.price),
    }
}

async fn overview(State(state): State<AppState>, headers: HeaderMap) -> Response {
    run_db(state, move |pool| {
        let (conn, account_id) = match open_account_connection(pool, &headers) {
            Ok(value) => value,
            Err(response) => return *response,
        };
        let account = match db::get_account(&conn, &account_id) {
            Some(account) => account_response(account),
            None => {
                return error(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "simulation account missing",
                )
            }
        };
        Json(OverviewResponse {
            account,
            contracts: db::list_contracts(&conn)
                .into_iter()
                .map(contract_response)
                .collect(),
            orders: db::list_account_orders(&conn, &account_id, None)
                .into_iter()
                .map(order_response)
                .collect(),
            positions: db::list_positions(&conn, Some(&account_id))
                .into_iter()
                .map(position_response)
                .collect(),
            cash: db::list_cash(&conn, Some(&account_id))
                .into_iter()
                .map(cash_response)
                .collect(),
            fills: db::list_fills(&conn, &account_id)
                .into_iter()
                .map(fill_response)
                .collect(),
        })
        .into_response()
    })
    .await
}

async fn contracts(State(state): State<AppState>, headers: HeaderMap) -> Response {
    run_db(state, move |pool| {
        let (conn, _) = match open_account_connection(pool, &headers) {
            Ok(value) => value,
            Err(response) => return *response,
        };
        Json(
            db::list_contracts(&conn)
                .into_iter()
                .map(contract_response)
                .collect::<Vec<_>>(),
        )
        .into_response()
    })
    .await
}

async fn create_contract(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<ContractRequest>,
) -> Response {
    run_db(state, move |pool| {
        let (conn, _) = match open_account_connection(pool, &headers) {
            Ok(value) => value,
            Err(response) => return *response,
        };
        let symbol = request.symbol.trim().to_uppercase();
        let sec_type = request.sec_type.trim().to_uppercase();
        let exchange = request.exchange.trim().to_uppercase();
        let currency = request.currency.trim().to_uppercase();
        if request.conid <= 0 || symbol.is_empty() || currency.len() != 3 {
            return error(StatusCode::BAD_REQUEST, "invalid contract");
        }
        let contract = Contract {
            conid: request.conid,
            symbol,
            sec_type,
            exchange,
            currency,
        };
        match db::add_contract(&conn, &contract) {
            Ok(()) => (StatusCode::CREATED, Json(contract_response(contract))).into_response(),
            Err(message) => error(StatusCode::CONFLICT, message),
        }
    })
    .await
}

async fn orders(State(state): State<AppState>, headers: HeaderMap) -> Response {
    run_db(state, move |pool| {
        let (conn, account_id) = match open_account_connection(pool, &headers) {
            Ok(value) => value,
            Err(response) => return *response,
        };
        Json(
            db::list_account_orders(&conn, &account_id, None)
                .into_iter()
                .map(order_response)
                .collect::<Vec<_>>(),
        )
        .into_response()
    })
    .await
}

async fn place_order(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<OrderRequest>,
) -> Response {
    run_db(state, move |pool| {
        let (conn, account_id) = match open_account_connection(pool, &headers) {
            Ok(value) => value,
            Err(response) => return *response,
        };
        let quantity = match parse_decimal(&request.quantity, "quantity") {
            Ok(value) => value,
            Err(response) => return *response,
        };
        let lmt_price = match request.lmt_price.as_deref() {
            Some(value) => match parse_decimal(value, "limit price") {
                Ok(value) => Some(value),
                Err(response) => return *response,
            },
            None => None,
        };
        let aux_price = match request.aux_price.as_deref() {
            Some(value) => match parse_decimal(value, "stop price") {
                Ok(value) => Some(value),
                Err(response) => return *response,
            },
            None => None,
        };
        if !db::contract_exists(&conn, request.conid) {
            return error(StatusCode::BAD_REQUEST, "contract not found");
        }
        // Validate before allocating the order ID so invalid requests never
        // take the per-account row lock that serializes ID assignment.
        let side = request.side.to_uppercase();
        let order_type = request.order_type.to_uppercase();
        let candidate = NewOrder {
            order_id: 0,
            account_id: account_id.clone(),
            conid: request.conid,
            side,
            order_type,
            quantity,
            lmt_price,
            aux_price,
        };
        if let Err(message) = db::validate_order(&candidate) {
            return error(StatusCode::BAD_REQUEST, message);
        }
        let order_id = match db::next_order_id(&conn, &account_id) {
            Ok(value) => value,
            Err(_) => {
                return error(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "could not allocate order ID",
                )
            }
        };
        let order = NewOrder { order_id, ..candidate };
        if let Err(message) = db::place_order(&conn, &order) {
            return error(StatusCode::BAD_REQUEST, message);
        }
        Json(order_response(Order {
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
        }))
        .into_response()
    })
    .await
}

async fn cancel_order(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(order_id): Path<i64>,
) -> Response {
    run_db(state, move |pool| {
        let (conn, account_id) = match open_account_connection(pool, &headers) {
            Ok(value) => value,
            Err(response) => return *response,
        };
        // cancel_order reports non-cancellable orders itself; no scan needed.
        match db::cancel_order(&conn, order_id, &account_id) {
            Ok(()) => (StatusCode::NO_CONTENT, ()).into_response(),
            Err(message) => error(StatusCode::CONFLICT, message),
        }
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
        let (conn, account_id) = match open_account_connection(pool, &headers) {
            Ok(value) => value,
            Err(response) => return *response,
        };
        let price = match parse_decimal(&request.price, "fill price") {
            Ok(value) => value,
            Err(response) => return *response,
        };
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
        match db::record_fill(&conn, &fill) {
            Ok(()) => (StatusCode::NO_CONTENT, ()).into_response(),
            Err(message) => error(StatusCode::CONFLICT, message),
        }
    })
    .await
}

async fn positions(State(state): State<AppState>, headers: HeaderMap) -> Response {
    run_db(state, move |pool| {
        let (conn, account_id) = match open_account_connection(pool, &headers) {
            Ok(value) => value,
            Err(response) => return *response,
        };
        Json(
            db::list_positions(&conn, Some(&account_id))
                .into_iter()
                .map(position_response)
                .collect::<Vec<_>>(),
        )
        .into_response()
    })
    .await
}

async fn cash(State(state): State<AppState>, headers: HeaderMap) -> Response {
    run_db(state, move |pool| {
        let (conn, account_id) = match open_account_connection(pool, &headers) {
            Ok(value) => value,
            Err(response) => return *response,
        };
        Json(
            db::list_cash(&conn, Some(&account_id))
                .into_iter()
                .map(cash_response)
                .collect::<Vec<_>>(),
        )
        .into_response()
    })
    .await
}

async fn set_cash(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<CashRequest>,
) -> Response {
    run_db(state, move |pool| {
        let (conn, account_id) = match open_account_connection(pool, &headers) {
            Ok(value) => value,
            Err(response) => return *response,
        };
        let amount = match parse_decimal(&request.amount, "cash amount") {
            Ok(value) => value,
            Err(response) => return *response,
        };
        let currency = request.currency.trim().to_uppercase();
        if currency.len() != 3
            || !currency
                .chars()
                .all(|character| character.is_ascii_alphabetic())
        {
            return error(StatusCode::BAD_REQUEST, "currency must be a 3-letter code");
        }
        match db::set_cash(&conn, &account_id, &currency, amount) {
            Ok(()) => StatusCode::NO_CONTENT.into_response(),
            Err(message) => error(StatusCode::BAD_REQUEST, message),
        }
    })
    .await
}

async fn fills(State(state): State<AppState>, headers: HeaderMap) -> Response {
    run_db(state, move |pool| {
        let (conn, account_id) = match open_account_connection(pool, &headers) {
            Ok(value) => value,
            Err(response) => return *response,
        };
        Json(
            db::list_fills(&conn, &account_id)
                .into_iter()
                .map(fill_response)
                .collect::<Vec<_>>(),
        )
        .into_response()
    })
    .await
}

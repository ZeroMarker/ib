//! End-to-end HTTP tests against a real in-process server.
//!
//! These cover what unit tests cannot reach: session cookies, per-user
//! account isolation, and the order -> fill -> ledger accounting loop driven
//! entirely through the public API. Every test gets its own SQLite file, so
//! they are independent and can run in parallel.

use ib::db::{self, Pool};
use std::net::SocketAddr;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

/// Shared password for the test users. Meets the 8-byte minimum.
const PASSWORD: &str = "hunter2hunter2";

/// A running server on an ephemeral port plus its scratch database.
pub struct TestServer {
    address: SocketAddr,
    /// Kept alive so the temp directory outlives the server.
    _dir: tempfile::TempDir,
    db_path: std::path::PathBuf,
}

impl TestServer {
    pub async fn start() -> Self {
        let dir = tempfile::tempdir().expect("temp dir");
        let db_path = dir.path().join("ib.sqlite3");
        let conn = db::open(&db_path);
        db::init_schema(&conn);

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind ephemeral port");
        let address = listener.local_addr().expect("local addr");
        let app = ib::app_router(Pool::new(conn));
        tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });

        Self {
            address,
            _dir: dir,
            db_path,
        }
    }
}

/// A cookie-aware HTTP client: the session cookie is captured on login and
/// replayed afterwards, which is what makes the isolation tests meaningful.
pub struct Client {
    address: SocketAddr,
    cookie: Option<String>,
}

impl Client {
    pub fn new(server: &TestServer) -> Self {
        Self {
            address: server.address,
            cookie: None,
        }
    }

    fn remember_cookie(&mut self, response: &Response) {
        if let Some(cookie) = response.header("set-cookie") {
            if let Some(pair) = cookie.split(';').next() {
                self.cookie = Some(pair.to_owned());
            }
        }
    }

    /// Register and log in, keeping the session cookie.
    ///
    /// CI runs without `RESEND_API_KEY`, so the verification gate is
    /// downgraded and login succeeds. With a key configured locally, login
    /// returns 403 until the mailbox is verified, so assert either and let the
    /// caller decide.
    pub async fn signed_in(server: &TestServer, email: &str) -> Self {
        let mut client = Self::new(server);
        let registered = client
            .post(
                "/api/auth/register",
                &format!(r#"{{"email":"{email}","password":"{PASSWORD}"}}"#),
            )
            .await;
        assert_eq!(registered.status, 201, "{}", registered.body);

        let response = client
            .post(
                "/api/auth/login",
                &format!(r#"{{"email":"{email}","password":"{PASSWORD}"}}"#),
            )
            .await;
        assert!(
            response.status == 200 || response.status == 403,
            "unexpected login status {}: {}",
            response.status,
            response.body
        );
        client.remember_cookie(&response);
        client
    }

    pub fn has_session(&self) -> bool {
        self.cookie.is_some()
    }

    pub async fn get(&self, path: &str) -> Response {
        self.request("GET", path, None).await
    }

    pub async fn post(&self, path: &str, body: &str) -> Response {
        self.request("POST", path, Some(body)).await
    }

    pub async fn request(&self, method: &str, path: &str, body: Option<&str>) -> Response {
        let mut stream = TcpStream::connect(self.address).await.expect("connect");
        let mut head = format!(
            "{method} {path} HTTP/1.1\r\nHost: {}\r\nConnection: close\r\n",
            self.address
        );
        if let Some(cookie) = &self.cookie {
            head.push_str(&format!("Cookie: {cookie}\r\n"));
        }
        match body {
            Some(body) => {
                head.push_str("Content-Type: application/json\r\n");
                head.push_str(&format!("Content-Length: {}\r\n\r\n", body.len()));
                head.push_str(body);
            }
            None => head.push_str("\r\n"),
        }
        stream.write_all(head.as_bytes()).await.expect("write");
        let mut raw = Vec::new();
        stream.read_to_end(&mut raw).await.expect("read");
        Response::parse(&raw)
    }
}

pub struct Response {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: String,
}

impl Response {
    fn parse(raw: &[u8]) -> Self {
        let split = raw
            .windows(4)
            .position(|window| window == b"\r\n\r\n")
            .expect("no header terminator");
        let head = String::from_utf8_lossy(&raw[..split]).to_string();
        let body = String::from_utf8_lossy(&raw[split + 4..]).to_string();
        let mut lines = head.lines();
        let status = lines
            .next()
            .and_then(|line| line.split_whitespace().nth(1))
            .and_then(|code| code.parse().ok())
            .expect("status line");
        let headers = lines
            .filter_map(|line| line.split_once(':'))
            .map(|(name, value)| (name.trim().to_lowercase(), value.trim().to_string()))
            .collect();
        Self {
            status,
            headers,
            body,
        }
    }

    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(key, _)| key == &name.to_lowercase())
            .map(|(_, value)| value.as_str())
    }

    pub fn contains(&self, needle: &str) -> bool {
        self.body.contains(needle)
    }

    /// All values for `key`, in document order, at any depth.
    ///
    /// Depth matters: the overview nests `account_id` inside an `account`
    /// object and repeats it on every order, so a top-level lookup would miss
    /// it. Walking the whole tree is what lets a test ask "does this response
    /// contain any order with this status" without knowing the envelope shape.
    pub fn values(&self, key: &str) -> Vec<serde_json::Value> {
        let parsed: serde_json::Value = serde_json::from_str(&self.body).expect("json body");
        let mut found = Vec::new();
        collect(parsed, key, &mut found);
        found
    }

    pub fn strings(&self, key: &str) -> Vec<String> {
        self.values(key)
            .iter()
            .filter_map(|value| value.as_str().map(str::to_owned))
            .collect()
    }

    pub fn string(&self, key: &str) -> Option<String> {
        self.strings(key).into_iter().next()
    }

    pub fn numbers(&self, key: &str) -> Vec<i64> {
        self.values(key)
            .iter()
            .filter_map(serde_json::Value::as_i64)
            .collect()
    }
}

/// Recursively append every value stored under `key`.
fn collect(value: serde_json::Value, key: &str, found: &mut Vec<serde_json::Value>) {
    match value {
        serde_json::Value::Object(map) => {
            for (name, child) in map {
                if name == key {
                    found.push(child.clone());
                }
                collect(child, key, found);
            }
        }
        serde_json::Value::Array(items) => {
            for item in items {
                collect(item, key, found);
            }
        }
        _ => {}
    }
}

async fn add_contract(client: &Client, conid: i64, symbol: &str) -> i64 {
    let response = client
        .post(
            "/api/trading/contracts",
            &format!(
                r#"{{"conid":{conid},"symbol":"{symbol}","sec_type":"STK","exchange":"SMART","currency":"USD"}}"#
            ),
        )
        .await;
    assert_eq!(response.status, 201, "{}", response.body);
    conid
}

async fn set_cash(client: &Client, amount: &str) {
    let response = client
        .post(
            "/api/trading/cash",
            &format!(r#"{{"currency":"USD","amount":"{amount}"}}"#),
        )
        .await;
    assert_eq!(response.status, 204, "{}", response.body);
}

async fn place_order(
    client: &Client,
    conid: i64,
    side: &str,
    order_type: &str,
    quantity: &str,
    lmt_price: Option<&str>,
) -> Response {
    let price = lmt_price
        .map(|price| format!(r#""{price}""#))
        .unwrap_or_else(|| "null".to_string());
    client
        .post(
            "/api/trading/orders",
            &format!(
                r#"{{"conid":{conid},"side":"{side}","order_type":"{order_type}","quantity":"{quantity}","lmt_price":{price},"aux_price":null}}"#
            ),
        )
        .await
}

#[tokio::test]
async fn health_reports_a_live_database() {
    let server = TestServer::start().await;
    let response = Client::new(&server).get("/api/health").await;
    assert_eq!(response.status, 200);
    assert!(response.contains("ok"));
}

#[tokio::test]
async fn index_is_served_with_no_store() {
    let server = TestServer::start().await;
    let response = Client::new(&server).get("/").await;
    assert_eq!(response.status, 200);
    assert!(response
        .header("cache-control")
        .is_some_and(|value| value.contains("no-store")));
}

#[tokio::test]
async fn protected_endpoints_require_a_session() {
    let server = TestServer::start().await;
    let client = Client::new(&server);
    for path in [
        "/api/auth/me",
        "/api/trading/overview",
        "/api/trading/orders",
        "/api/trading/positions",
        "/api/trading/cash",
        "/api/trading/fills",
        "/api/trading/contracts",
    ] {
        let response = client.get(path).await;
        assert_eq!(response.status, 401, "{path} should require a session");
    }
}

#[tokio::test]
async fn registration_does_not_issue_a_session() {
    let server = TestServer::start().await;
    let client = Client::new(&server);
    let response = client
        .post(
            "/api/auth/register",
            &format!(r#"{{"email":"new@example.com","password":"{PASSWORD}"}}"#),
        )
        .await;
    assert_eq!(response.status, 201, "{}", response.body);
    assert!(response.contains("\"email_verified\":false"));
    assert_eq!(
        response.header("set-cookie"),
        None,
        "register must not sign the user in"
    );
    assert!(!client.has_session());
    // And the new account cannot use the trading API without logging in.
    assert_eq!(client.get("/api/auth/me").await.status, 401);
}

#[tokio::test]
async fn duplicate_registration_is_a_conflict() {
    let server = TestServer::start().await;
    let client = Client::new(&server);
    let body = format!(r#"{{"email":"dup@example.com","password":"{PASSWORD}"}}"#);
    assert_eq!(client.post("/api/auth/register", &body).await.status, 201);
    let response = client.post("/api/auth/register", &body).await;
    assert_eq!(response.status, 409);
    // Case and whitespace must normalize to the same account.
    let repeated = client
        .post(
            "/api/auth/register",
            &format!(r#"{{"email":" DUP@example.com ","password":"{PASSWORD}"}}"#),
        )
        .await;
    assert_eq!(repeated.status, 409, "emails are compared normalized");
}

#[tokio::test]
async fn login_rejects_a_wrong_password_and_a_bad_email_alike() {
    let server = TestServer::start().await;
    let client = Client::new(&server);
    client
        .post(
            "/api/auth/register",
            &format!(r#"{{"email":"real@example.com","password":"{PASSWORD}"}}"#),
        )
        .await;

    let wrong_password = client
        .post(
            "/api/auth/login",
            r#"{"email":"real@example.com","password":"not-the-password"}"#,
        )
        .await;
    let unknown_email = client
        .post(
            "/api/auth/login",
            r#"{"email":"ghost@example.com","password":"whatever-password"}"#,
        )
        .await;

    assert_eq!(wrong_password.status, 401);
    assert_eq!(unknown_email.status, 401);
    // Identical bodies, so the response cannot be used to probe for accounts.
    assert_eq!(wrong_password.body, unknown_email.body);
}

#[tokio::test]
async fn invalid_input_is_rejected_before_touching_the_ledger() {
    let server = TestServer::start().await;
    let client = Client::signed_in(&server, "invalid@example.com").await;
    add_contract(&client, 1001, "AAA").await;
    set_cash(&client, "10000").await;

    // Quantity zero, unsupported type, bad side, and an unknown conid.
    for body in [
        r#"{"conid":1001,"side":"BUY","order_type":"MKT","quantity":"0","lmt_price":null,"aux_price":null}"#,
        r#"{"conid":1001,"side":"BUY","order_type":"XYZ","quantity":"10","lmt_price":null,"aux_price":null}"#,
        r#"{"conid":1001,"side":"HOLD","order_type":"MKT","quantity":"10","lmt_price":null,"aux_price":null}"#,
        r#"{"conid":9999,"side":"BUY","order_type":"MKT","quantity":"10","lmt_price":null,"aux_price":null}"#,
        // More than six decimal places must be rejected, not rounded.
        r#"{"conid":1001,"side":"BUY","order_type":"LMT","quantity":"10.0000001","lmt_price":"1","aux_price":null}"#,
        // LMT without a limit price.
        r#"{"conid":1001,"side":"BUY","order_type":"LMT","quantity":"10","lmt_price":null,"aux_price":null}"#,
    ] {
        let response = client.post("/api/trading/orders", body).await;
        assert_eq!(response.status, 400, "{body} -> {}", response.body);
    }

    // Nothing above may have reached the ledger.
    let overview = client.get("/api/trading/overview").await;
    assert_eq!(overview.strings("order_id").len(), 0);
    let cash = client.get("/api/trading/cash").await;
    assert_eq!(cash.strings("cash"), vec!["10000".to_string()]);
}

#[tokio::test]
async fn a_cash_deposit_must_be_positive() {
    let server = TestServer::start().await;
    let client = Client::signed_in(&server, "deposit@example.com").await;
    set_cash(&client, "10000").await;

    // The form in `useTrading.setCash` blocks these, but the server has to
    // enforce it too: the browser check is a convenience, not a control.
    for amount in ["0", "-1", "-0.000001", "-100000"] {
        let response = client
            .post(
                "/api/trading/cash",
                &format!(r#"{{"currency":"USD","amount":"{amount}"}}"#),
            )
            .await;
        assert_eq!(response.status, 400, "amount {amount} must be rejected");
        assert!(response.contains("must be positive"), "{}", response.body);
    }

    // Rejected amounts must not have touched the balance.
    let cash = client.get("/api/trading/cash").await;
    assert_eq!(cash.strings("cash"), vec!["10000".to_string()]);
}

#[tokio::test]
async fn users_only_see_their_own_account() {
    let server = TestServer::start().await;
    let alice = Client::signed_in(&server, "alice@example.com").await;
    let bob = Client::signed_in(&server, "bob@example.com").await;

    add_contract(&alice, 2001, "SHARED").await;
    set_cash(&alice, "50000").await;
    add_contract(&bob, 2002, "BOBONLY").await;
    set_cash(&bob, "7000").await;

    let placed = place_order(&alice, 2001, "BUY", "MKT", "10", None).await;
    assert_eq!(placed.status, 200, "{}", placed.body);
    let order_id = placed.numbers("order_id")[0];
    assert_eq!(
        alice
            .post(
                &format!("/api/trading/orders/{order_id}/fill"),
                r#"{"price":"10.00","exec_id":"EX-ISO-1"}"#,
            )
            .await
            .status,
        204
    );

    let alice_overview = alice.get("/api/trading/overview").await;
    let bob_overview = bob.get("/api/trading/overview").await;
    assert_eq!(alice_overview.status, 200);
    assert_eq!(bob_overview.status, 200);

    let alice_account = alice_overview.string("account_id").expect("account");
    let bob_account = bob_overview.string("account_id").expect("account");
    assert_ne!(alice_account, bob_account, "accounts must differ");

    // Alice's order, fill, position and cash must not appear in Bob's payload.
    assert_eq!(
        alice_overview.strings("exec_id"),
        vec!["EX-ISO-1".to_string()]
    );
    assert_eq!(alice_overview.strings("cash"), vec!["49900".to_string()]);
    assert_eq!(alice_overview.strings("position"), vec!["10".to_string()]);

    assert!(
        bob_overview.strings("exec_id").is_empty(),
        "Bob saw Alice's fill: {}",
        bob_overview.body
    );
    assert_eq!(bob_overview.strings("position").len(), 0);
    assert!(!bob_overview.contains("49900"), "Alice's balance leaked");
    assert!(!bob_overview.contains("EX-ISO-1"), "Alice's exec id leaked");
    // Bob was funded separately, so only his own balance may be present.
    assert_eq!(bob_overview.strings("cash"), vec!["7000".to_string()]);

    // Contracts are deliberately shared reference data.
    let bob_contracts = bob.get("/api/trading/contracts").await;
    assert!(bob_contracts.contains("SHARED"));
    assert!(bob_contracts.contains("BOBONLY"));
}

#[tokio::test]
async fn a_user_cannot_cancel_or_fill_another_users_order() {
    let server = TestServer::start().await;
    let alice = Client::signed_in(&server, "owner@example.com").await;
    let mallory = Client::signed_in(&server, "mallory@example.com").await;

    add_contract(&alice, 3001, "ALICE").await;
    set_cash(&alice, "100000").await;
    let placed = place_order(&alice, 3001, "BUY", "MKT", "10", None).await;
    assert_eq!(placed.status, 200, "{}", placed.body);
    let order_id = placed.numbers("order_id")[0];

    // The order id is the primary key together with ACCOUNT_ID, so Mallory
    // holding the same numeric id must not reach Alice's row.
    let cancel = mallory
        .post(&format!("/api/trading/orders/{order_id}/cancel"), "")
        .await;
    assert_eq!(cancel.status, 409, "cross-account cancel must be refused");

    let fill = mallory
        .post(
            &format!("/api/trading/orders/{order_id}/fill"),
            r#"{"price":"100.00"}"#,
        )
        .await;
    assert_eq!(fill.status, 409, "cross-account fill must be refused");

    // The order must be untouched: still open, no position, no cash movement.
    let orders = alice.get("/api/trading/orders").await;
    assert_eq!(orders.strings("status"), vec!["Submitted".to_string()]);
    let positions = alice.get("/api/trading/positions").await;
    assert_eq!(positions.strings("position").len(), 0);
    let cash = alice.get("/api/trading/cash").await;
    assert_eq!(cash.strings("cash"), vec!["100000".to_string()]);
}

#[tokio::test]
async fn order_fill_and_ledger_stay_consistent() {
    let server = TestServer::start().await;
    let client = Client::signed_in(&server, "ledger@example.com").await;
    add_contract(&client, 4001, "AAPL").await;
    set_cash(&client, "100000").await;

    let placed = place_order(&client, 4001, "BUY", "LMT", "100", Some("185.50")).await;
    assert_eq!(placed.status, 200, "{}", placed.body);
    let order_id = placed.numbers("order_id")[0];
    assert_eq!(placed.strings("status"), vec!["Submitted".to_string()]);
    assert_eq!(placed.strings("filled_quantity"), vec!["0".to_string()]);

    let fill = client
        .post(
            &format!("/api/trading/orders/{order_id}/fill"),
            r#"{"price":"185.52","exec_id":"EX-LEDGER-1"}"#,
        )
        .await;
    assert_eq!(fill.status, 204, "{}", fill.body);

    let orders = client.get("/api/trading/orders").await;
    assert_eq!(orders.strings("status"), vec!["Filled".to_string()]);
    assert_eq!(orders.strings("filled_quantity"), vec!["100".to_string()]);

    let positions = client.get("/api/trading/positions").await;
    assert_eq!(positions.strings("position"), vec!["100".to_string()]);
    assert_eq!(positions.strings("avg_cost"), vec!["185.52".to_string()]);

    // 100 * 185.52 * multiplier(1.0) leaves 100000 - 18552.
    let cash = client.get("/api/trading/cash").await;
    assert_eq!(cash.strings("cash"), vec!["81448".to_string()]);

    let fills = client.get("/api/trading/fills").await;
    assert_eq!(fills.strings("exec_id"), vec!["EX-LEDGER-1".to_string()]);
    assert_eq!(fills.strings("quantity"), vec!["100".to_string()]);
    assert_eq!(fills.strings("price"), vec!["185.52".to_string()]);

    // Cancelling a filled order is not allowed.
    let cancel = client
        .post(&format!("/api/trading/orders/{order_id}/cancel"), "")
        .await;
    assert_eq!(cancel.status, 409);
}

#[tokio::test]
async fn an_exec_id_retry_does_not_double_count() {
    let server = TestServer::start().await;
    let client = Client::signed_in(&server, "idem@example.com").await;
    add_contract(&client, 5001, "RETRY").await;
    set_cash(&client, "100000").await;

    let placed = place_order(&client, 5001, "BUY", "LMT", "10", Some("50")).await;
    let order_id = placed.numbers("order_id")[0];

    let body = r#"{"price":"50.00","exec_id":"EX-RETRY-1"}"#;
    let first = client
        .post(&format!("/api/trading/orders/{order_id}/fill"), body)
        .await;
    assert_eq!(first.status, 204, "{}", first.body);
    let cash_after_first = client
        .get("/api/trading/cash")
        .await
        .strings("cash")
        .concat();

    // The same exec id for the same fill is a no-op, not a second booking.
    let retry = client
        .post(&format!("/api/trading/orders/{order_id}/fill"), body)
        .await;
    assert_eq!(retry.status, 204, "{}", retry.body);

    let cash_after_retry = client
        .get("/api/trading/cash")
        .await
        .strings("cash")
        .concat();
    assert_eq!(cash_after_first, cash_after_retry, "retry moved cash again");

    let positions = client.get("/api/trading/positions").await;
    assert_eq!(positions.strings("position"), vec!["10".to_string()]);
    let fills = client.get("/api/trading/fills").await;
    assert_eq!(
        fills.strings("exec_id").len(),
        1,
        "the fill was recorded twice"
    );
}

#[tokio::test]
async fn cancelling_makes_an_order_unfillable() {
    let server = TestServer::start().await;
    let client = Client::signed_in(&server, "cancel@example.com").await;
    add_contract(&client, 6001, "CANCEL").await;
    set_cash(&client, "10000").await;

    let placed = place_order(&client, 6001, "BUY", "LMT", "10", Some("20")).await;
    let order_id = placed.numbers("order_id")[0];

    let cancel = client
        .post(&format!("/api/trading/orders/{order_id}/cancel"), "")
        .await;
    assert_eq!(cancel.status, 204, "{}", cancel.body);

    // Cancelling twice is a conflict, and a cancelled order cannot fill.
    let again = client
        .post(&format!("/api/trading/orders/{order_id}/cancel"), "")
        .await;
    assert_eq!(again.status, 409);
    let fill = client
        .post(
            &format!("/api/trading/orders/{order_id}/fill"),
            r#"{"price":"20.00"}"#,
        )
        .await;
    assert_eq!(fill.status, 409, "a cancelled order must not fill");

    let positions = client.get("/api/trading/positions").await;
    assert_eq!(positions.strings("position").len(), 0);
    let cash = client.get("/api/trading/cash").await;
    assert_eq!(cash.strings("cash"), vec!["10000".to_string()]);
}

#[tokio::test]
async fn logout_invalidates_the_session() {
    let server = TestServer::start().await;
    let client = Client::signed_in(&server, "logout@example.com").await;
    assert!(client.has_session());
    assert_eq!(client.get("/api/auth/me").await.status, 200);

    let logout = client.post("/api/auth/logout", "").await;
    assert_eq!(logout.status, 204, "{}", logout.body);
    // The server must not still accept the token the client is holding.
    assert_eq!(client.get("/api/auth/me").await.status, 401);
    assert_eq!(client.get("/api/trading/overview").await.status, 401);

    // A second logout is idempotent rather than an error.
    let again = client.post("/api/auth/logout", "").await;
    assert_eq!(again.status, 204);
}

#[tokio::test]
async fn verification_rejects_unknown_and_expired_tokens() {
    let server = TestServer::start().await;
    let client = Client::new(&server);

    let unknown = client
        .post("/api/auth/verify", r#"{"token":"not-a-real-token"}"#)
        .await;
    assert_eq!(unknown.status, 400, "{}", unknown.body);
    assert!(unknown.contains("invalid verification token"));

    let empty = client.post("/api/auth/verify", r#"{"token":"  "}"#).await;
    assert_eq!(empty.status, 400);

    // An expired row must read as gone (410) rather than as a bad token (400):
    // the two are different user problems, and a stale link is a 410.
    let conn = db::open(&server.db_path);
    db::init_schema(&conn);
    let verification_id = uuid::Uuid::new_v4().to_string();
    let stale_hash = format!(
        "{:x}",
        <sha2::Sha256 as sha2::Digest>::digest(b"stale-token")
    );
    conn.execute(
        "INSERT INTO USERS (USER_ID, EMAIL, PASSWORD_HASH) VALUES ('u-stale','stale@example.com','x')",
        [],
    )
    .expect("insert user");
    conn.execute(
        "INSERT INTO EMAIL_VERIFICATIONS (VERIFICATION_ID, USER_ID, TOKEN_HASH, EXPIRES_AT) \
         VALUES (?1, 'u-stale', ?2, datetime('now', '-1 hour'))",
        rusqlite::params![verification_id, stale_hash],
    )
    .expect("insert stale verification");

    let expired = client
        .post("/api/auth/verify", r#"{"token":"stale-token"}"#)
        .await;
    assert_eq!(expired.status, 410, "{}", expired.body);
    assert!(expired.contains("expired"));
}

#[tokio::test]
async fn resend_does_not_reveal_whether_an_email_exists() {
    let server = TestServer::start().await;
    let client = Client::new(&server);
    // A real, just-registered account: unverified, so a token is minted.
    client
        .post(
            "/api/auth/register",
            &format!(r#"{{"email":"known@example.com","password":"{PASSWORD}"}}"#),
        )
        .await;

    let known = client
        .post(
            "/api/auth/resend-verification",
            r#"{"email":"known@example.com"}"#,
        )
        .await;
    let unknown = client
        .post(
            "/api/auth/resend-verification",
            r#"{"email":"nobody@example.com"}"#,
        )
        .await;

    assert_eq!(known.status, 200, "{}", known.body);
    assert_eq!(unknown.status, 200);
    // Byte-identical bodies: a difference here would be an enumeration oracle.
    assert_eq!(known.body, unknown.body);
}

#[tokio::test]
async fn a_signed_in_user_always_gets_a_stable_simulation_account() {
    let server = TestServer::start().await;
    let client = Client::signed_in(&server, "stable@example.com").await;
    let first = client
        .get("/api/trading/overview")
        .await
        .string("account_id")
        .expect("account id");
    assert!(first.starts_with("SIM"), "{first}");
    assert_eq!(first.len(), 16, "{first}");

    // A second, independent session for the same user resolves to the same
    // account, and the first session keeps working alongside it.
    let body = format!(r#"{{"email":"stable@example.com","password":"{PASSWORD}"}}"#);
    let mut second_session = Client::new(&server);
    let response = second_session.post("/api/auth/login", &body).await;
    assert!(response.status == 200 || response.status == 403);
    second_session.remember_cookie(&response);
    assert!(
        second_session.has_session(),
        "the second login must issue its own cookie"
    );

    let again = second_session
        .get("/api/trading/overview")
        .await
        .string("account_id")
        .expect("account id");
    assert_eq!(first, again, "account id must be stable across sessions");

    // Logging in twice must not create a second simulation account.
    let accounts: i64 = db::open(&server.db_path)
        .query_row("SELECT COUNT(*) FROM ACCOUNTS", [], |row| row.get(0))
        .expect("count accounts");
    assert_eq!(accounts, 1, "one user must map to exactly one account");
}

#[tokio::test]
async fn a_duplicate_contract_is_a_conflict() {
    let server = TestServer::start().await;
    let client = Client::signed_in(&server, "contract@example.com").await;
    add_contract(&client, 7001, "UNIQUE").await;

    let same_conid = client
        .post(
            "/api/trading/contracts",
            r#"{"conid":7001,"symbol":"OTHER","sec_type":"STK","exchange":"SMART","currency":"USD"}"#,
        )
        .await;
    assert_eq!(same_conid.status, 409, "{}", same_conid.body);

    // A different conid but the same symbol tuple also violates the unique index.
    let same_symbol = client
        .post(
            "/api/trading/contracts",
            r#"{"conid":7002,"symbol":"UNIQUE","sec_type":"STK","exchange":"SMART","currency":"USD"}"#,
        )
        .await;
    assert_eq!(same_symbol.status, 409, "{}", same_symbol.body);

    for body in [
        r#"{"conid":0,"symbol":"BAD","sec_type":"STK","exchange":"SMART","currency":"USD"}"#,
        r#"{"conid":7003,"symbol":"","sec_type":"STK","exchange":"SMART","currency":"USD"}"#,
        r#"{"conid":7003,"symbol":"BAD","sec_type":"STK","exchange":"SMART","currency":"US"}"#,
        r#"{"conid":7003,"symbol":"BAD","sec_type":"STK","exchange":"SMART","currency":"US1"}"#,
    ] {
        let response = client.post("/api/trading/contracts", body).await;
        assert_eq!(response.status, 400, "{body} -> {}", response.body);
    }
}

#[tokio::test]
async fn order_ids_increment_per_account() {
    let server = TestServer::start().await;
    let alice = Client::signed_in(&server, "seq-alice@example.com").await;
    let bob = Client::signed_in(&server, "seq-bob@example.com").await;
    add_contract(&alice, 8001, "SEQ").await;
    set_cash(&alice, "100000").await;
    set_cash(&bob, "100000").await;

    let mut alice_ids = Vec::new();
    for _ in 0..3 {
        let placed = place_order(&alice, 8001, "BUY", "MKT", "1", None).await;
        assert_eq!(placed.status, 200, "{}", placed.body);
        alice_ids.push(placed.numbers("order_id")[0]);
    }
    assert_eq!(alice_ids, vec![1, 2, 3], "ids must increment per account");

    // Bob's counter is independent, so his first order is also 1.
    let bobs = place_order(&bob, 8001, "BUY", "MKT", "1", None).await;
    assert_eq!(bobs.numbers("order_id")[0], 1);
}

#[tokio::test]
async fn expired_sessions_are_reclaimed_on_login() {
    let server = TestServer::start().await;
    let client = Client::signed_in(&server, "purge@example.com").await;

    let conn = db::open(&server.db_path);
    let live_before: i64 = conn
        .query_row("SELECT COUNT(*) FROM SESSIONS", [], |row| row.get(0))
        .expect("count sessions");
    assert!(live_before > 0);

    // Plant an expired row alongside the live session.
    conn.execute(
        "INSERT INTO SESSIONS (SESSION_ID, USER_ID, TOKEN_HASH, EXPIRES_AT) \
         SELECT 'stale-session', USER_ID, 'stale-hash', datetime('now', '-1 day') FROM USERS \
         WHERE EMAIL = 'purge@example.com'",
        [],
    )
    .expect("insert expired session");
    let with_stale: i64 = conn
        .query_row("SELECT COUNT(*) FROM SESSIONS", [], |row| row.get(0))
        .expect("count sessions");
    assert_eq!(with_stale, live_before + 1);

    // Logging in runs the sweep, which must delete the expired row and keep
    // the live one. Sweeping on write rather than on a timer is what stops
    // SESSIONS from growing without bound.
    let fresh = Client::new(&server);
    let response = fresh
        .post(
            "/api/auth/login",
            &format!(r#"{{"email":"purge@example.com","password":"{PASSWORD}"}}"#),
        )
        .await;
    assert!(response.status == 200 || response.status == 403);

    let stale_left: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM SESSIONS WHERE SESSION_ID = 'stale-session'",
            [],
            |row| row.get(0),
        )
        .expect("count stale");
    assert_eq!(stale_left, 0, "expired session was not swept");

    // The other user's session is untouched.
    let client_still_valid = client.get("/api/auth/me").await.status;
    assert_eq!(client_still_valid, 200);
}

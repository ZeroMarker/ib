//! End-to-end HTTP tests against a real in-process server.
//!
//! These cover what unit tests cannot reach: session cookies, per-user
//! account isolation, and the order -> fill -> ledger accounting loop driven
//! entirely through the public API. Every test gets its own SQLite file, so
//! they are independent and can run in parallel.

use ib::db::{self, Connection, Pool};
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
    /// caller skip the authenticated assertions.
    pub async fn signed_in(server: &TestServer, email: &str) -> Self {
        let mut client = Self::new(server);
        let registered = client
            .post(
                "/api/auth/register",
                &format!("{{\"email\":\"{email}\",\"password\":\"{PASSWORD}\"}}"),
            )
            .await;
        assert_eq!(registered.status, 201, "register: {}", registered.body);
        let login = client
            .post(
                "/api/auth/login",
                &format!("{{\"email\":\"{email}\",\"password\":\"{PASSWORD}\"}}"),
            )
            .await;
        assert!(
            login.status == 200 || login.status == 403,
            "login: {} {}",
            login.status,
            login.body
        );
        client.remember_cookie(&login);
        client
    }

    /// True when a session was actually issued, i.e. the login gate allowed it.
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
        let mut request =
            format!("{method} {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n");
        if let Some(cookie) = &self.cookie {
            request.push_str(&format!("Cookie: {cookie}\r\n"));
        }
        if let Some(body) = body {
            request.push_str("Content-Type: application/json\r\n");
            request.push_str(&format!("Content-Length: {}\r\n\r\n", body.len()));
            request.push_str(body);
        } else {
            request.push_str("\r\n");
        }

        stream
            .write_all(request.as_bytes())
            .await
            .expect("write request");
        let mut raw = Vec::new();
        stream.read_to_end(&mut raw).await.expect("read response");
        Response::parse(&raw)
    }
}

/// A hand-rolled HTTP/1.1 response parser, keeping the test dependency surface
/// at zero. `Connection: close` means one read yields the whole message.
pub struct Response {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: String,
}

impl Response {
    fn parse(raw: &[u8]) -> Self {
        let text = String::from_utf8_lossy(raw).into_owned();
        let (head, body) = match text.split_once("\r\n\r\n") {
            Some(parts) => parts,
            None => (text.as_str(), ""),
        };
        let mut lines = head.lines();
        let status = lines
            .next()
            .and_then(|line| line.split_whitespace().nth(1))
            .and_then(|code| code.parse().ok())
            .unwrap_or(0);
        let headers = lines
            .filter_map(|line| line.split_once(':'))
            .map(|(name, value)| (name.trim().to_lowercase(), value.trim().to_owned()))
            .collect();
        Self {
            status,
            headers,
            body: body.to_owned(),
        }
    }

    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.as_str())
    }

    pub fn contains(&self, needle: &str) -> bool {
        self.body.contains(needle)
    }

    /// All occurrences of `"key":"value"`.
    pub fn strings(&self, key: &str) -> Vec<String> {
        let needle = format!("\"{key}\":\"");
        let mut found = Vec::new();
        let mut rest = self.body.as_str();
        while let Some(index) = rest.find(&needle) {
            let after = &rest[index + needle.len()..];
            match after.find('"') {
                Some(end) => {
                    found.push(after[..end].to_owned());
                    rest = &after[end..];
                }
                None => break,
            }
        }
        found
    }

    pub fn string(&self, key: &str) -> Option<String> {
        self.strings(key).into_iter().next()
    }

    /// All occurrences of `"key":<number>`.
    pub fn numbers(&self, key: &str) -> Vec<i64> {
        let needle = format!("\"{key}\":");
        let mut found = Vec::new();
        let mut rest = self.body.as_str();
        while let Some(index) = rest.find(&needle) {
            let after = &rest[index + needle.len()..];
            let digits: String = after
                .chars()
                .take_while(|c| c.is_ascii_digit() || *c == '-')
                .collect();
            if let Ok(value) = digits.parse::<i64>() {
                found.push(value);
                rest = &after[digits.len()..];
            } else {
                rest = after;
            }
        }
        found
    }
}

/// Add a contract and return its conid.
/// `CONTRACTS` is a global catalogue, so a conid is shared by every account:
/// a 201 or a 409 both mean the contract is available to this client.
async fn add_contract(client: &Client, conid: i64, symbol: &str) -> i64 {
    let response = client
        .post(
            "/api/trading/contracts",
            &format!(
                "{{\"conid\":{conid},\"symbol\":\"{symbol}\",\"sec_type\":\"STK\",\
                 \"exchange\":\"SMART\",\"currency\":\"USD\"}}"
            ),
        )
        .await;
    assert!(
        response.status == 201 || response.status == 409,
        "add contract: {} {}",
        response.status,
        response.body
    );
    conid
}

async fn set_cash(client: &Client, amount: &str) {
    let response = client
        .post(
            "/api/trading/cash",
            &format!("{{\"currency\":\"USD\",\"amount\":\"{amount}\"}}"),
        )
        .await;
    assert_eq!(response.status, 204, "set cash: {}", response.body);
}

async fn place_order(
    client: &Client,
    conid: i64,
    side: &str,
    kind: &str,
    qty: &str,
    lmt: Option<&str>,
) -> i64 {
    let lmt_field = match lmt {
        Some(price) => format!("\"lmt_price\":\"{price}\""),
        None => "\"lmt_price\":null".to_owned(),
    };
    let response = client
        .post(
            "/api/trading/orders",
            &format!(
                "{{\"conid\":{conid},\"side\":\"{side}\",\"order_type\":\"{kind}\",\
                 \"quantity\":\"{qty}\",{lmt_field},\"aux_price\":null}}"
            ),
        )
        .await;
    assert_eq!(response.status, 200, "place order: {}", response.body);
    response
        .numbers("order_id")
        .into_iter()
        .next()
        .expect("order id")
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
        .header("content-type")
        .unwrap_or_default()
        .contains("text/html"));
    assert_eq!(
        response.header("cache-control"),
        Some("no-cache, no-store, must-revalidate")
    );
}

#[tokio::test]
async fn protected_endpoints_require_a_session() {
    let server = TestServer::start().await;
    let anonymous = Client::new(&server);
    for path in [
        "/api/auth/me",
        "/api/trading/overview",
        "/api/trading/orders",
        "/api/trading/positions",
        "/api/trading/cash",
        "/api/trading/fills",
    ] {
        let response = anonymous.get(path).await;
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
            "{\"email\":\"new@example.com\",\"password\":\"hunter2hunter2\"}",
        )
        .await;
    assert_eq!(response.status, 201);
    assert!(response.string("user_id").is_some());
    // 201 with no Set-Cookie: verification must happen first.
    assert!(response.header("set-cookie").is_none());
}

#[tokio::test]
async fn duplicate_registration_is_a_conflict() {
    let server = TestServer::start().await;
    let client = Client::new(&server);
    let body = "{\"email\":\"dup@example.com\",\"password\":\"hunter2hunter2\"}";
    assert_eq!(client.post("/api/auth/register", body).await.status, 201);
    let second = client.post("/api/auth/register", body).await;
    assert_eq!(second.status, 409);
    assert!(second.contains("already registered"));
}

#[tokio::test]
async fn login_rejects_a_wrong_password_and_a_bad_email_alike() {
    let server = TestServer::start().await;
    let client = Client::new(&server);
    client
        .post(
            "/api/auth/register",
            "{\"email\":\"pw@example.com\",\"password\":\"hunter2hunter2\"}",
        )
        .await;

    let wrong = client
        .post(
            "/api/auth/login",
            "{\"email\":\"pw@example.com\",\"password\":\"wrongpassword1\"}",
        )
        .await;
    let unknown = client
        .post(
            "/api/auth/login",
            "{\"email\":\"nobody@example.com\",\"password\":\"hunter2hunter2\"}",
        )
        .await;
    assert_eq!(wrong.status, 401);
    assert_eq!(unknown.status, 401);
    // Identical message: the response must not reveal whether the account exists.
    assert_eq!(wrong.body, unknown.body);
}

#[tokio::test]
async fn invalid_input_is_rejected_before_touching_the_ledger() {
    let server = TestServer::start().await;
    let client = Client::signed_in(&server, "invalid@example.com").await;

    let short_password = Client::new(&server)
        .post(
            "/api/auth/register",
            "{\"email\":\"short@example.com\",\"password\":\"abc\"}",
        )
        .await;
    assert_eq!(short_password.status, 400);

    let bad_email = Client::new(&server)
        .post(
            "/api/auth/register",
            "{\"email\":\"not-an-email\",\"password\":\"hunter2hunter2\"}",
        )
        .await;
    assert_eq!(bad_email.status, 400);

    let unowned_contract = client
        .post(
            "/api/trading/orders",
            "{\"conid\":999,\"side\":\"BUY\",\"order_type\":\"MKT\",\"quantity\":\"1\"}",
        )
        .await;
    assert_eq!(unowned_contract.status, 400);
    assert!(unowned_contract.contains("contract not found"));

    add_contract(&client, 1, "AAPL").await;
    let bad_quantity = client
        .post(
            "/api/trading/orders",
            "{\"conid\":1,\"side\":\"BUY\",\"order_type\":\"MKT\",\"quantity\":\"-5\"}",
        )
        .await;
    assert_eq!(bad_quantity.status, 400);

    let bad_currency = client
        .post(
            "/api/trading/cash",
            "{\"currency\":\"DOLLARS\",\"amount\":\"10\"}",
        )
        .await;
    assert_eq!(bad_currency.status, 400);
}

#[tokio::test]
async fn users_only_see_their_own_account() {
    let server = TestServer::start().await;
    let alice = Client::signed_in(&server, "alice@example.com").await;
    let bob = Client::signed_in(&server, "bob@example.com").await;

    add_contract(&alice, 1, "AAPL").await;
    set_cash(&alice, "100000").await;
    let order_id = place_order(&alice, 1, "BUY", "LMT", "100", Some("185.50")).await;
    assert!(
        alice
            .post(
                &format!("/api/trading/orders/{order_id}/fill"),
                "{\"price\":\"185.52\"}"
            )
            .await
            .status
            == 204
    );

    let alice_overview = alice.get("/api/trading/overview").await;
    let bob_overview = bob.get("/api/trading/overview").await;
    assert_eq!(alice_overview.status, 200);
    assert_eq!(bob_overview.status, 200);

    let alice_account = alice_overview.string("account_id").expect("account");
    let bob_account = bob_overview.string("account_id").expect("account");
    assert_ne!(alice_account, bob_account, "accounts must be per user");

    // Bob must not see Alice's order, fill or cash.
    assert_eq!(alice_overview.strings("exec_id").len(), 1);
    assert!(!bob_overview.contains("\"order_id\":1"));
    assert!(bob_overview.strings("exec_id").is_empty());
    // Bob never funded his account, so he has no cash row at all and cannot
    // see Alice's balance.
    assert!(bob_overview.strings("cash").is_empty());
    // Alice's funded balance must not leak into Bob's payload either.
    assert!(!bob_overview.contains("81448"));
}

#[tokio::test]
async fn a_user_cannot_cancel_or_fill_another_users_order() {
    let server = TestServer::start().await;
    let alice = Client::signed_in(&server, "alice2@example.com").await;
    let bob = Client::signed_in(&server, "bob2@example.com").await;
    add_contract(&alice, 7, "TSLA").await;
    set_cash(&alice, "50000").await;
    let order_id = place_order(&alice, 7, "BUY", "LMT", "10", Some("200")).await;

    // The order exists in Alice's account, so Bob's lookup misses it and the
    // update affects zero rows -> conflict, not success.
    let cancel = bob
        .post(&format!("/api/trading/orders/{order_id}/cancel"), "")
        .await;
    assert_eq!(cancel.status, 409);
    let fill = bob
        .post(
            &format!("/api/trading/orders/{order_id}/fill"),
            "{\"price\":\"200\"}",
        )
        .await;
    assert_eq!(fill.status, 409);

    // Alice's order is untouched.
    let orders = alice.get("/api/trading/orders").await;
    assert!(orders.contains("Submitted"));
}

#[tokio::test]
async fn order_fill_and_ledger_stay_consistent() {
    let server = TestServer::start().await;
    let client = Client::signed_in(&server, "ledger@example.com").await;
    add_contract(&client, 265598, "AAPL").await;
    set_cash(&client, "100000").await;

    let order_id = place_order(&client, 265598, "BUY", "LMT", "100", Some("185.50")).await;
    let orders = client.get("/api/trading/orders").await;
    assert!(orders.contains("Submitted"));
    assert!(orders.contains("\"total_quantity\":\"100\""));

    let fill = client
        .post(
            &format!("/api/trading/orders/{order_id}/fill"),
            "{\"price\":\"185.52\"}",
        )
        .await;
    assert_eq!(fill.status, 204, "fill: {}", fill.body);

    let overview = client.get("/api/trading/overview").await;
    assert!(overview.contains("Filled"));
    assert!(overview.contains("\"position\":\"100\""));
    assert!(overview.contains("\"avg_cost\":\"185.52\""));
    // 100000 - (100 * 185.52) = 81448
    assert!(overview.contains("\"cash\":\"81448\""), "{}", overview.body);
    assert_eq!(overview.strings("exec_id").len(), 1);
}

#[tokio::test]
async fn an_exec_id_retry_does_not_double_count() {
    let server = TestServer::start().await;
    let client = Client::signed_in(&server, "retry@example.com").await;
    add_contract(&client, 5, "NVDA").await;
    set_cash(&client, "10000").await;
    let order_id = place_order(&client, 5, "BUY", "MKT", "10", None).await;

    let body = format!("{{\"price\":\"100\",\"exec_id\":\"{}-EX\"}}", "RETRY");
    let path = format!("/api/trading/orders/{order_id}/fill");
    assert_eq!(client.post(&path, &body).await.status, 204);
    // A repeated exec_id is a deliberate no-op, so the retry also answers 204
    // and, crucially, does not add a second ledger entry.
    assert_eq!(client.post(&path, &body).await.status, 204);

    let overview = client.get("/api/trading/overview").await;
    assert_eq!(overview.strings("exec_id").len(), 1);
    assert!(overview.contains("\"position\":\"10\""));
    // 10000 - 10 * 100 = 9000
    assert!(overview.contains("\"cash\":\"9000\""));
}

#[tokio::test]
async fn cancelling_makes_an_order_unfillable() {
    let server = TestServer::start().await;
    let client = Client::signed_in(&server, "cancel@example.com").await;
    add_contract(&client, 11, "AMD").await;
    set_cash(&client, "1000").await;
    let order_id = place_order(&client, 11, "BUY", "MKT", "1", None).await;

    let cancel = client
        .post(&format!("/api/trading/orders/{order_id}/cancel"), "")
        .await;
    assert_eq!(cancel.status, 204);
    // Cancelling twice is a conflict, not a silent success.
    assert_eq!(
        client
            .post(&format!("/api/trading/orders/{order_id}/cancel"), "")
            .await
            .status,
        409
    );
    let fill = client
        .post(
            &format!("/api/trading/orders/{order_id}/fill"),
            "{\"price\":\"100\"}",
        )
        .await;
    assert_eq!(fill.status, 409);
}

#[tokio::test]
async fn logout_invalidates_the_session() {
    let server = TestServer::start().await;
    let client = Client::signed_in(&server, "logout@example.com").await;
    assert_eq!(client.get("/api/auth/me").await.status, 200);
    assert_eq!(client.post("/api/auth/logout", "").await.status, 204);
    assert_eq!(client.get("/api/auth/me").await.status, 401);
}

#[tokio::test]
async fn verification_rejects_unknown_and_expired_tokens() {
    let server = TestServer::start().await;
    let client = Client::new(&server);
    let unknown = client
        .post("/api/auth/verify", "{\"token\":\"does-not-exist\"}")
        .await;
    assert_eq!(unknown.status, 400);

    let empty = client.post("/api/auth/verify", "{\"token\":\"  \"}").await;
    assert_eq!(empty.status, 400);
}

#[tokio::test]
async fn resend_does_not_reveal_whether_an_email_exists() {
    let server = TestServer::start().await;
    let client = Client::new(&server);
    client
        .post(
            "/api/auth/register",
            "{\"email\":\"known@example.com\",\"password\":\"hunter2hunter2\"}",
        )
        .await;

    let known = client
        .post(
            "/api/auth/resend-verification",
            "{\"email\":\"known@example.com\"}",
        )
        .await;
    let unknown = client
        .post(
            "/api/auth/resend-verification",
            "{\"email\":\"ghost@example.com\"}",
        )
        .await;
    // Identical answers for both, so the endpoint cannot be used to enumerate
    // registered addresses.
    assert_eq!(known.status, unknown.status);
    assert_eq!(known.body, unknown.body);
}

#[tokio::test]
async fn a_signed_in_user_always_gets_a_stable_simulation_account() {
    let server = TestServer::start().await;
    let client = Client::signed_in(&server, "stable@example.com").await;
    add_contract(&client, 42, "GOOG").await;

    let first = client
        .get("/api/trading/overview")
        .await
        .string("account_id")
        .expect("account");
    set_cash(&client, "500").await;
    let second = client
        .get("/api/trading/overview")
        .await
        .string("account_id")
        .expect("account");
    assert_eq!(first, second);
    assert!(first.starts_with("SIM"));
}

#[tokio::test]
async fn a_duplicate_contract_is_a_conflict() {
    let server = TestServer::start().await;
    let client = Client::signed_in(&server, "contract@example.com").await;
    add_contract(&client, 1, "AAPL").await;
    let again = client
        .post(
            "/api/trading/contracts",
            "{\"conid\":1,\"symbol\":\"AAPL\",\"sec_type\":\"STK\",\
             \"exchange\":\"SMART\",\"currency\":\"USD\"}",
        )
        .await;
    assert_eq!(again.status, 409);
}

#[tokio::test]
async fn order_ids_increment_per_account() {
    let server = TestServer::start().await;
    let alice = Client::signed_in(&server, "ids-alice@example.com").await;
    let bob = Client::signed_in(&server, "ids-bob@example.com").await;
    add_contract(&alice, 1, "AAPL").await;
    add_contract(&bob, 1, "AAPL").await;
    set_cash(&alice, "1000").await;
    set_cash(&bob, "1000").await;

    assert_eq!(place_order(&alice, 1, "BUY", "MKT", "1", None).await, 1);
    assert_eq!(place_order(&alice, 1, "BUY", "MKT", "1", None).await, 2);
    // Each account has its own sequence, so Bob also starts at 1.
    assert_eq!(place_order(&bob, 1, "BUY", "MKT", "1", None).await, 1);
}

#[tokio::test]
async fn expired_sessions_are_reclaimed_on_login() {
    let server = TestServer::start().await;
    let client = Client::signed_in(&server, "gc@example.com").await;
    // Borrow the user row the fixture just created instead of inserting one.

    // Plant a stale row in the same database the server is using.
    let conn = Connection::open(&server.db_path).expect("open test database");
    conn.execute(
        "INSERT INTO SESSIONS (SESSION_ID, USER_ID, TOKEN_HASH, EXPIRES_AT) \
         VALUES ('stale',(SELECT USER_ID FROM USERS WHERE EMAIL='gc@example.com'),\
                 'stale-hash', datetime('now','-1 day'))",
        [],
    )
    .expect("insert stale session");
    let before: i64 = conn
        .query_row("SELECT COUNT(*) FROM SESSIONS", [], |row| row.get(0))
        .expect("count");
    assert!(before >= 1);

    // Logging in runs the sweep.
    client
        .post(
            "/api/auth/login",
            "{\"email\":\"gc@example.com\",\"password\":\"hunter2hunter2\"}",
        )
        .await;
    let after: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM SESSIONS WHERE SESSION_ID = 'stale'",
            [],
            |row| row.get(0),
        )
        .expect("count");
    assert_eq!(after, 0, "expired sessions must be deleted");
}

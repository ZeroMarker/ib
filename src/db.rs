use crate::models::*;
pub use rusqlite::Connection;
use rusqlite::{params, OptionalExtension};
use rust_decimal::prelude::ToPrimitive;
use rust_decimal::Decimal;
use std::error::Error;
use std::path::Path;
use std::sync::{Mutex, MutexGuard};

const DECIMAL_SCALE: i64 = 1_000_000;

/// Single-connection pool over a SQLite database file.
///
/// SQLite serializes writers at the database level, so one
/// `Mutex`-guarded connection replaces a pooled design. Every web
/// handler holds the guard for its whole request, which also serializes
/// `MAX(ORDER_ID) + 1` allocation — `next_order_id` needs no row lock.
pub struct Pool {
    inner: Mutex<Connection>,
}

impl Pool {
    pub fn new(conn: Connection) -> Self {
        Self {
            inner: Mutex::new(conn),
        }
    }

    pub fn get(&self) -> Result<MutexGuard<'_, Connection>, String> {
        self.inner
            .lock()
            .map_err(|_| "database lock poisoned".to_string())
    }
}

/// Open (creating parent directories as needed) and tune a SQLite database file.
pub fn open(path: &Path) -> Connection {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)
                .unwrap_or_else(|e| panic!("cannot create db dir {}: {e}", parent.display()));
        }
    }
    let conn = Connection::open(path)
        .unwrap_or_else(|e| panic!("SQLite open failed {}: {e}", path.display()));
    conn.execute_batch(
        "PRAGMA journal_mode = WAL;
         PRAGMA busy_timeout = 5000;
         PRAGMA foreign_keys = ON;",
    )
    .expect("SQLite pragma setup failed");
    conn
}

fn begin(conn: &Connection) {
    conn.execute_batch("BEGIN IMMEDIATE")
        .expect("begin transaction failed");
}

fn commit(conn: &Connection, what: &str) -> Result<(), String> {
    conn.execute_batch("COMMIT").map_err(|error| {
        let _ = conn.execute_batch("ROLLBACK");
        format!("{what} commit failed: {error}")
    })
}

/// Strip `--` comment lines and yield one statement per top-level `;`.
///
/// A naive `split(';')` corrupts SQL in three ways, all of which this handles:
///
/// 1. a `;` inside a `'...'` or `"..."` literal;
/// 2. a `--` comment containing a `;`;
/// 3. the body of a `CREATE TRIGGER ... BEGIN ... END;`, which contains both
///    semicolons and its own `END` keyword.
///
/// Case 3 is why this cannot be a plain scanner: `END` only closes a trigger
/// body when it is followed by a `;`, and a bare `END` can also appear inside
/// a string or a `CASE ... END` expression. Tracking a `BEGIN`/`END` depth
/// counter and only splitting on a `;` at depth zero is what lets a migration
/// add a trigger without silently executing its body as separate statements.
fn split_statements(sql: &str) -> Vec<String> {
    let mut statements = Vec::new();
    let mut current = String::new();
    let mut quote: Option<char> = None;
    // Non-zero while inside a `BEGIN ... END` block (a trigger body). Semicolons
    // at depth > 0 belong to the block and must not split the statement.
    let mut block_depth: usize = 0;
    for line in sql.lines() {
        let trimmed = line.trim_start();
        if quote.is_none() && trimmed.starts_with("--") {
            continue;
        }
        let characters: Vec<char> = line.chars().collect();
        for (index, character) in characters.iter().copied().enumerate() {
            match quote {
                Some(open) if character == open => quote = None,
                Some(_) => {}
                None => match character {
                    '\'' | '"' => quote = Some(character),
                    _ => {}
                },
            }

            // Keywords are recognized only outside quotes, and only when they
            // stand alone as words: `BEGINNING` or `APPEND` must not count.
            if quote.is_none() && is_keyword(&characters, index, "BEGIN") {
                block_depth += 1;
            } else if quote.is_none() && is_keyword(&characters, index, "END") {
                // `END` inside a `CASE ... END` expression closes the case, not
                // a block. Distinguishing the two needs real parsing; the
                // conservative choice is to only decrement on a bare `END`
                // that is itself followed by `;` or end-of-input, which is how
                // a trigger body always terminates.
                if block_depth > 0 && ends_block(&characters, index) {
                    block_depth -= 1;
                }
            }

            // The `END` that closes a trigger is followed by the statement's
            // own `;`; splitting on that `;` is what emits the whole trigger.
            if character == ';' && quote.is_none() && block_depth == 0 {
                let statement = current.trim().to_owned();
                if !statement.is_empty() {
                    statements.push(statement);
                }
                current.clear();
                continue;
            }
            current.push(character);
        }
        current.push('\n');
    }
    let trailing = current.trim().to_owned();
    if !trailing.is_empty() {
        statements.push(trailing);
    }
    statements
}

/// True when `keyword` appears at `index` in `line` as a standalone word.
///
/// Requires a non-word character (or line end) on both sides, so `END` does
/// not match inside `APPEND` and `BEGIN` does not match inside `BEGINNER`.
fn is_keyword(line: &[char], index: usize, keyword: &str) -> bool {
    let starts_word = index == 0 || !is_word_char(line[index - 1]);
    if !starts_word {
        return false;
    }
    let end = index + keyword.chars().count();
    if end > line.len() {
        return false;
    }
    let matches = keyword
        .chars()
        .enumerate()
        .all(|(offset, expected)| line[index + offset].eq_ignore_ascii_case(&expected));
    if !matches {
        return false;
    }
    end == line.len() || !is_word_char(line[end])
}

/// True when this `END` is the only word on its line.
///
/// A trigger body always closes with `END;` alone on its own line, which is
/// what SQLite's own trigger syntax requires. Requiring the line to be
/// otherwise blank is what keeps a `CASE ... END` from closing the block: that
/// `END` has SQL in front of it on the same line.
fn ends_block(line: &[char], index: usize) -> bool {
    let before_is_blank = line[..index]
        .iter()
        .all(|character| character.is_whitespace());
    let after_is_terminator = line[index + "END".chars().count()..]
        .iter()
        .all(|character| character.is_whitespace() || *character == ';');
    before_is_blank && after_is_terminator
}

fn is_word_char(character: char) -> bool {
    character.is_ascii_alphanumeric() || character == '_'
}

fn apply_migration(conn: &Connection, sql: &str, name: &str) {
    for statement in split_statements(sql) {
        conn.execute_batch(&statement)
            .unwrap_or_else(|error| panic!("SQL failed in {name}: {error}\n{statement}"));
        println!(
            "ok [{name}]: {}",
            statement.lines().next().unwrap_or("").trim()
        );
    }
}

/// Collect every row from a prepared statement, panicking on a row-level
/// error. Every list in this module already treats a query failure as fatal,
/// so this removes ~8 copies of the same three-line collect chain.
fn collect_rows<T>(
    stmt: &mut rusqlite::Statement<'_>,
    params: impl rusqlite::Params,
    map: impl FnMut(&rusqlite::Row<'_>) -> rusqlite::Result<T>,
) -> Vec<T> {
    stmt.query_map(params, map)
        .expect("query failed")
        .map(|row| row.expect("row error"))
        .collect()
}

fn decimal_at(row: &rusqlite::Row, index: usize) -> Decimal {
    let micros: i64 = row.get(index).expect("invalid INTEGER micros value");
    Decimal::new(micros, 6)
}

fn optional_decimal_at(row: &rusqlite::Row, index: usize) -> Option<Decimal> {
    let micros: Option<i64> = row
        .get(index)
        .expect("invalid nullable INTEGER micros value");
    micros.map(|value| Decimal::new(value, 6))
}

/// Convert a value to the six-decimal fixed-point micro-units used by the schema.
fn scaled(value: &Decimal) -> Result<i64, Box<dyn Error>> {
    if value.round_dp(6) != *value {
        return Err(format!("value {value} has more than 6 decimal places").into());
    }
    (value * Decimal::from(DECIMAL_SCALE))
        .to_i64()
        .ok_or_else(|| format!("value {value} is outside the fixed-point range").into())
}

fn scaled_round(value: &Decimal) -> Result<i64, Box<dyn Error>> {
    use rust_decimal::prelude::ToPrimitive;
    (value.round_dp(6) * Decimal::from(DECIMAL_SCALE))
        .to_i64()
        .ok_or_else(|| format!("value {value} is outside the fixed-point range").into())
}

/// Migrations embedded in the binary, applied in order.
const MIGRATIONS: &[(&str, &str)] = &[
    (
        "001_simulation_schema.sql",
        include_str!("../migrations/001_simulation_schema.sql"),
    ),
    (
        "002_auth_schema.sql",
        include_str!("../migrations/002_auth_schema.sql"),
    ),
    (
        "003_email_verification.sql",
        include_str!("../migrations/003_email_verification.sql"),
    ),
    (
        "004_maintenance_indexes.sql",
        include_str!("../migrations/004_maintenance_indexes.sql"),
    ),
];

pub fn init_schema(conn: &Connection) {
    apply_migrations(conn, MIGRATIONS);
    println!("schema created");
}

/// Append the auth tables to a database that already has trading data.
/// Mirrors `init_schema` minus the trading migration, so an existing
/// deployment can gain login support without touching its ledger.
pub fn init_auth_schema(conn: &Connection) {
    let migrations: Vec<(&str, &str)> = MIGRATIONS
        .iter()
        .filter(|(name, _)| *name != "001_simulation_schema.sql")
        .copied()
        .collect();
    apply_migrations(conn, &migrations);
    println!("auth schema created");
}

/// Apply migrations in filename order, skipping any already recorded.
fn apply_migrations(conn: &Connection, migrations: &[(&str, &str)]) {
    ensure_migration_ledger(conn);
    for (name, sql) in migrations {
        if migration_applied(conn, name) {
            continue;
        }
        apply_migration(conn, sql, name);
        // Recorded after the statements succeed: a failed migration must stay
        // pending so the next run retries it instead of skipping it.
        if let Err(error) = conn.execute(
            "INSERT INTO SCHEMA_MIGRATIONS (NAME) VALUES (?1)",
            params![name],
        ) {
            panic!("cannot record migration {name}: {error}");
        }
    }
}

fn ensure_migration_ledger(conn: &Connection) {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS SCHEMA_MIGRATIONS (
             NAME       TEXT NOT NULL,
             APPLIED_AT TEXT DEFAULT (datetime('now')) NOT NULL,
             CONSTRAINT PK_SCHEMA_MIGRATIONS PRIMARY KEY (NAME)
         )",
    )
    .unwrap_or_else(|error| panic!("cannot create migration ledger: {error}"));
}

fn migration_applied(conn: &Connection, name: &str) -> bool {
    conn.query_row(
        "SELECT COUNT(*) FROM SCHEMA_MIGRATIONS WHERE NAME = ?1",
        params![name],
        |row| row.get::<_, i64>(0),
    )
    .unwrap_or(0)
        > 0
}

pub fn drop_schema(conn: &Connection) {
    // SCHEMA_MIGRATIONS goes last: while it exists the ledger claims the
    // schema is current, so it must not outlive the tables it describes.
    for table in [
        "EMAIL_VERIFICATIONS",
        "SESSIONS",
        "USERS",
        "CASH_BALANCES",
        "POSITIONS",
        "FILLS",
        "ORDERS",
        "CONTRACTS",
        "ACCOUNTS",
        "SCHEMA_MIGRATIONS",
    ] {
        let sql = format!("DROP TABLE IF EXISTS {table}");
        match conn.execute_batch(&sql) {
            Ok(()) => println!("dropped {table}"),
            Err(e) => println!("skip {table}: {e}"),
        }
    }
}

/// Create a simulation account.
///
/// Returns the constraint message instead of panicking: a duplicate id is an
/// ordinary user error that the CLI reports as `ib: account failed: ...`
/// with exit code 1, and that a library caller can handle.
pub fn add_account(conn: &Connection, account_id: &str, account_type: &str) -> Result<(), String> {
    let account_type = account_type.to_uppercase();
    if !["CASH", "MARGIN", "IRA"].contains(&account_type.as_str()) {
        return Err(format!("unsupported account type: {account_type}"));
    }
    conn.execute(
        "INSERT INTO ACCOUNTS (ACCOUNT_ID, ACCOUNT_TYPE) VALUES (?1, ?2)",
        params![account_id, account_type],
    )
    .map(|_| ())
    .map_err(|error| error.to_string())
}

/// Derive a stable, private simulation account ID from an authenticated user ID.
pub fn user_account_id(user_id: &str) -> String {
    let suffix: String = user_id
        .chars()
        .filter(|character| character.is_ascii_hexdigit())
        .take(13)
        .collect();
    format!("SIM{suffix}")
}

/// Ensure that an authenticated user has one simulation account.
pub fn ensure_user_account(conn: &Connection, user_id: &str) -> Result<String, rusqlite::Error> {
    let account_id = user_account_id(user_id);
    let existing: i64 = conn.query_row(
        "SELECT COUNT(*) FROM ACCOUNTS WHERE ACCOUNT_ID = ?1",
        params![account_id],
        |row| row.get(0),
    )?;
    if existing == 0 {
        // INSERT OR IGNORE lets a concurrent first-login race settle on the
        // primary key instead of failing one of the requests.
        conn.execute(
            "INSERT OR IGNORE INTO ACCOUNTS (ACCOUNT_ID, ACCOUNT_TYPE) VALUES (?1, 'MARGIN')",
            params![account_id],
        )?;
    }
    Ok(account_id)
}

pub fn list_accounts(conn: &Connection) -> Vec<Account> {
    let mut stmt = conn
        .prepare(
            "SELECT ACCOUNT_ID, ACCOUNT_TYPE, CURRENCY, STATUS FROM ACCOUNTS ORDER BY ACCOUNT_ID",
        )
        .expect("prepare failed");
    collect_rows(&mut stmt, [], |r| {
        Ok(Account {
            account_id: r.get(0)?,
            account_type: r.get(1)?,
            currency: r.get(2)?,
            status: r.get(3)?,
        })
    })
}

/// Fetch a single account by ID instead of scanning the whole table.
pub fn get_account(conn: &Connection, account_id: &str) -> Option<Account> {
    conn.query_row(
        "SELECT ACCOUNT_ID, ACCOUNT_TYPE, CURRENCY, STATUS FROM ACCOUNTS \
         WHERE ACCOUNT_ID = ?1",
        params![account_id],
        |r| {
            Ok(Account {
                account_id: r.get(0)?,
                account_type: r.get(1)?,
                currency: r.get(2)?,
                status: r.get(3)?,
            })
        },
    )
    .optional()
    .expect("query failed")
}

pub fn add_contract(conn: &Connection, c: &Contract) -> Result<(), String> {
    let sec_type = c.sec_type.to_uppercase();
    let exchange = c.exchange.to_uppercase();
    let currency = c.currency.to_uppercase();
    conn.execute(
        "INSERT INTO CONTRACTS (CONID, SYMBOL, SEC_TYPE, EXCHANGE, CURRENCY) \
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![c.conid, c.symbol, sec_type, exchange, currency],
    )
    .map_err(|error| error.to_string())?;
    Ok(())
}

/// Check contract existence with a targeted lookup instead of loading all rows.
pub fn contract_exists(conn: &Connection, conid: i64) -> bool {
    conn.query_row(
        "SELECT COUNT(*) FROM CONTRACTS WHERE CONID = ?1",
        params![conid],
        |row| row.get::<_, i64>(0),
    )
    .map(|count| count > 0)
    .unwrap_or(false)
}

pub fn list_contracts(conn: &Connection) -> Vec<Contract> {
    let mut stmt = conn
        .prepare("SELECT CONID, SYMBOL, SEC_TYPE, EXCHANGE, CURRENCY FROM CONTRACTS ORDER BY CONID")
        .expect("prepare failed");
    collect_rows(&mut stmt, [], |r| {
        Ok(Contract {
            conid: r.get(0)?,
            symbol: r.get(1)?,
            sec_type: r.get(2)?,
            exchange: r.get(3)?,
            currency: r.get(4)?,
        })
    })
}

pub fn validate_order(o: &NewOrder) -> Result<(), String> {
    if !["MKT", "LMT", "STP", "STP_LMT"].contains(&o.order_type.as_str()) {
        return Err(format!("unsupported order type: {}", o.order_type));
    }
    if o.quantity <= Decimal::ZERO {
        return Err("quantity must be positive".into());
    }
    if matches!(o.order_type.as_str(), "LMT" | "STP_LMT") && o.lmt_price.is_none() {
        return Err(format!("{} requires LMT_PRICE", o.order_type));
    }
    if matches!(o.order_type.as_str(), "STP" | "STP_LMT") && o.aux_price.is_none() {
        return Err(format!("{} requires AUX_PRICE", o.order_type));
    }
    if !["BUY", "SELL"].contains(&o.side.as_str()) {
        return Err("side must be BUY or SELL".into());
    }
    if o.lmt_price.is_some_and(|price| price <= Decimal::ZERO) {
        return Err("limit price must be positive".into());
    }
    if o.aux_price.is_some_and(|price| price <= Decimal::ZERO) {
        return Err("aux price must be positive".into());
    }
    Ok(())
}

pub fn place_order(conn: &Connection, o: &NewOrder) -> Result<(), String> {
    validate_order(o)?;
    let quantity = scaled(&o.quantity).map_err(|error| error.to_string())?;

    let lmt_price = o
        .lmt_price
        .as_ref()
        .map(scaled)
        .transpose()
        .map_err(|error| error.to_string())?;
    let aux_price = o
        .aux_price
        .as_ref()
        .map(scaled)
        .transpose()
        .map_err(|error| error.to_string())?;

    begin(conn);
    let result = conn
        .execute(
            "INSERT INTO ORDERS (ORDER_ID, ACCOUNT_ID, CONID, SIDE, ORDER_TYPE, \
                                 LMT_PRICE, AUX_PRICE, TOTAL_QUANTITY, STATUS) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 'Submitted')",
            params![
                o.order_id,
                o.account_id,
                o.conid,
                o.side,
                o.order_type,
                lmt_price,
                aux_price,
                quantity,
            ],
        )
        .map_err(|error| {
            // Release the transaction before the pooled connection goes back
            // to the pool with an open transaction.
            let _ = conn.execute_batch("ROLLBACK");
            error.to_string()
        });
    match result {
        Ok(_) => commit(conn, "order"),
        Err(error) => Err(error),
    }
}

pub fn next_order_id(conn: &Connection, account_id: &str) -> Result<i64, rusqlite::Error> {
    // Allocation is serialized by the Pool guard held across the whole
    // request, so MAX(ORDER_ID) + 1 cannot race on this connection.
    conn.query_row(
        "SELECT COALESCE(MAX(ORDER_ID), 0) + 1 FROM ORDERS WHERE ACCOUNT_ID = ?1",
        params![account_id],
        |row| row.get(0),
    )
}

fn map_order(row: &rusqlite::Row) -> rusqlite::Result<Order> {
    Ok(Order {
        order_id: row.get(0)?,
        perm_id: row.get(1)?,
        account_id: row.get(2)?,
        conid: row.get(3)?,
        side: row.get(4)?,
        order_type: row.get(5)?,
        total_quantity: decimal_at(row, 6),
        filled_quantity: decimal_at(row, 7),
        lmt_price: optional_decimal_at(row, 8),
        aux_price: optional_decimal_at(row, 9),
        status: row.get(10)?,
    })
}

pub fn list_account_orders(
    conn: &Connection,
    account_id: &str,
    status: Option<&str>,
) -> Vec<Order> {
    let mut stmt = conn
        .prepare(&format!(
            "{ORDER_SELECT} WHERE ACCOUNT_ID = ?1 \
             AND (?2 IS NULL OR STATUS = ?2) ORDER BY ORDER_ID"
        ))
        .expect("prepare failed");
    collect_rows(&mut stmt, params![account_id, status], map_order)
}

pub fn list_fills(conn: &Connection, account_id: &str) -> Vec<Fill> {
    let mut stmt = conn
        .prepare(
            "SELECT EXEC_ID, ORDER_ID, ACCOUNT_ID, CONID, SIDE, QUANTITY, PRICE \
             FROM FILLS WHERE ACCOUNT_ID = ?1 ORDER BY EXEC_TIME DESC",
        )
        .expect("prepare failed");
    collect_rows(&mut stmt, [account_id], |r| {
        Ok(Fill {
            exec_id: r.get(0)?,
            order_id: r.get(1)?,
            account_id: r.get(2)?,
            conid: r.get(3)?,
            side: r.get(4)?,
            quantity: decimal_at(r, 5),
            price: decimal_at(r, 6),
        })
    })
}

const ORDER_SELECT: &str = "SELECT ORDER_ID, PERM_ID, ACCOUNT_ID, CONID, SIDE, ORDER_TYPE, \
     TOTAL_QUANTITY, FILLED_QUANTITY, LMT_PRICE, AUX_PRICE, STATUS FROM ORDERS";

pub fn list_orders(conn: &Connection, status: Option<&str>) -> Vec<Order> {
    let mut stmt = conn
        .prepare(&format!(
            "{ORDER_SELECT} WHERE (?1 IS NULL OR STATUS = ?1) ORDER BY ORDER_ID"
        ))
        .expect("prepare failed");
    collect_rows(&mut stmt, params![status], map_order)
}

pub fn cancel_order(conn: &Connection, order_id: i64, account_id: &str) -> Result<(), String> {
    let changed = conn
        .execute(
            "UPDATE ORDERS SET STATUS = 'Cancelled', UPDATED_AT = datetime('now') \
             WHERE ORDER_ID = ?1 AND ACCOUNT_ID = ?2 \
               AND STATUS NOT IN ('Filled','Cancelled')",
            params![order_id, account_id],
        )
        .map_err(|error| error.to_string())?;
    if changed == 0 {
        return Err(format!(
            "order {order_id} not cancellable in account {account_id}"
        ));
    }
    Ok(())
}

/// Record a full remaining fill and roll up order, position and cash atomically.
pub fn record_fill(conn: &Connection, f: &NewFill) -> Result<(), String> {
    if f.price <= Decimal::ZERO {
        return Err("fill failed: fill price must be positive".into());
    }
    if f.exec_id.is_empty() || f.exec_id.len() > 24 {
        return Err("fill failed: execution ID must contain 1 to 24 bytes".into());
    }
    let price_scaled = scaled(&f.price).map_err(|error| format!("fill failed: {error}"))?;

    begin(conn);
    let result = record_fill_txn(conn, f, price_scaled);
    match result {
        Ok(()) => commit(conn, "fill"),
        Err(error) => {
            let _ = conn.execute_batch("ROLLBACK");
            Err(format!("fill failed: {error}"))
        }
    }
}

fn record_fill_txn(conn: &Connection, f: &NewFill, price_scaled: i64) -> Result<(), String> {
    // A retry with the same execution ID is a no-op. Reusing it for another fill is rejected.
    let same_fill: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM FILLS \
             WHERE EXEC_ID = ?1 AND ORDER_ID = ?2 AND ACCOUNT_ID = ?3 \
               AND PRICE = ?4",
            params![f.exec_id, f.order_id, f.account_id, price_scaled],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())?;
    if same_fill > 0 {
        return Ok(());
    }
    let used_exec_id: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM FILLS WHERE EXEC_ID = ?1",
            params![f.exec_id],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())?;
    if used_exec_id > 0 {
        return Err(format!(
            "execution ID {} belongs to another fill",
            f.exec_id
        ));
    }

    let (conid, side, total, filled): (i64, String, i64, i64) = conn
        .query_row(
            "SELECT CONID, SIDE, TOTAL_QUANTITY, FILLED_QUANTITY FROM ORDERS \
             WHERE ORDER_ID = ?1 AND ACCOUNT_ID = ?2 \
               AND STATUS IN ('Submitted', 'PreSubmitted')",
            params![f.order_id, f.account_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .map_err(|error| match error {
            rusqlite::Error::QueryReturnedNoRows => {
                "order not found or not submittable".to_string()
            }
            other => other.to_string(),
        })?;
    let remaining = total - filled;
    if remaining <= 0 {
        return Err("order already fully filled".into());
    }

    conn.execute(
        "INSERT INTO FILLS (EXEC_ID, ORDER_ID, ACCOUNT_ID, CONID, SIDE, QUANTITY, PRICE) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            f.exec_id,
            f.order_id,
            f.account_id,
            conid,
            side,
            remaining,
            price_scaled,
        ],
    )
    .map_err(|error| error.to_string())?;

    conn.execute(
        "UPDATE ORDERS SET FILLED_QUANTITY = ?1, \
                AVG_FILL_PRICE = ?2, STATUS = 'Filled', \
                UPDATED_AT = datetime('now') \
         WHERE ORDER_ID = ?3 AND ACCOUNT_ID = ?4 \
           AND STATUS IN ('Submitted', 'PreSubmitted')",
        params![total, price_scaled, f.order_id, f.account_id],
    )
    .map_err(|error| error.to_string())?;

    roll_up_position(conn, &f.account_id, conid, &side, remaining, price_scaled)?;

    let (currency, multiplier) = contract_details(conn, conid)?;
    let price = Decimal::new(price_scaled, 6);
    let remaining_decimal = Decimal::new(remaining, 6);
    let cash_delta = if side == "BUY" {
        -(remaining_decimal * price * multiplier)
    } else {
        remaining_decimal * price * multiplier
    };
    let cash_delta_scaled = scaled_round(&cash_delta).map_err(|error| error.to_string())?;
    let current: Option<i64> = conn
        .query_row(
            "SELECT CASH FROM CASH_BALANCES WHERE ACCOUNT_ID = ?1 AND CURRENCY = ?2",
            params![f.account_id, currency],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| error.to_string())?;
    let balance = current.unwrap_or(0) + cash_delta_scaled;
    conn.execute(
        "INSERT INTO CASH_BALANCES (ACCOUNT_ID, CURRENCY, CASH, UPDATED_AT) \
         VALUES (?1, ?2, ?3, datetime('now')) \
         ON CONFLICT (ACCOUNT_ID, CURRENCY) DO UPDATE SET \
            CASH = excluded.CASH, UPDATED_AT = datetime('now')",
        params![f.account_id, currency, balance],
    )
    .map_err(|error| error.to_string())?;

    Ok(())
}

/// Fold a fill quantity into the position row, keeping the average-cost rule
/// previously expressed as a single MERGE statement: clearing the position resets the
/// cost, opening or adding on the same side blends it, and a flip takes the
/// fill price while a partial reduction keeps the old cost.
fn roll_up_position(
    conn: &Connection,
    account_id: &str,
    conid: i64,
    side: &str,
    fill_qty: i64,
    price_scaled: i64,
) -> Result<(), String> {
    let (old_position, old_avg): (i64, Option<i64>) = conn
        .query_row(
            "SELECT POSITION, AVG_COST FROM POSITIONS WHERE ACCOUNT_ID = ?1 AND CONID = ?2",
            params![account_id, conid],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
        .map_err(|error| error.to_string())?
        .unwrap_or((0, None));
    let delta = if side == "BUY" { fill_qty } else { -fill_qty };
    let new_position = old_position + delta;
    let price = Decimal::new(price_scaled, 6);
    let new_avg: Option<i64> = if new_position == 0 {
        None
    } else if old_position == 0
        || (old_position.signum() != delta.signum()
            && new_position.signum() != old_position.signum())
    {
        Some(price_scaled)
    } else if old_position.signum() == delta.signum() {
        let old_cost = old_avg.map(|value| Decimal::new(value, 6)).unwrap_or(price);
        let blended = (Decimal::new(old_position.abs(), 6) * old_cost
            + Decimal::new(delta.abs(), 6) * price)
            / Decimal::new(old_position.abs() + delta.abs(), 6);
        Some(scaled_round(&blended).map_err(|error| error.to_string())?)
    } else {
        old_avg
    };
    conn.execute(
        "INSERT INTO POSITIONS (ACCOUNT_ID, CONID, POSITION, AVG_COST, UPDATED_AT) \
         VALUES (?1, ?2, ?3, ?4, datetime('now')) \
         ON CONFLICT (ACCOUNT_ID, CONID) DO UPDATE SET \
            POSITION = excluded.POSITION, AVG_COST = excluded.AVG_COST, \
            UPDATED_AT = datetime('now')",
        params![account_id, conid, new_position, new_avg],
    )
    .map_err(|error| error.to_string())?;
    Ok(())
}

fn contract_details(conn: &Connection, conid: i64) -> Result<(String, Decimal), String> {
    let (currency, multiplier): (String, i64) = conn
        .query_row(
            "SELECT CURRENCY, MULTIPLIER FROM CONTRACTS WHERE CONID = ?1",
            params![conid],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .map_err(|error| error.to_string())?;
    Ok((currency, Decimal::new(multiplier, 6)))
}

/// Overwrite a position snapshot directly, bypassing the fill ledger.
///
/// Still returns `Result` rather than panicking so the CLI can report a bad
/// conid or an out-of-range quantity as a normal error.
pub fn set_position(
    conn: &Connection,
    account_id: &str,
    conid: i64,
    position: Decimal,
    avg_cost: Option<Decimal>,
) -> Result<(), String> {
    let position = scaled(&position).map_err(|error| error.to_string())?;
    let avg_cost = avg_cost
        .map(|value| scaled(&value).map_err(|error| error.to_string()))
        .transpose()?;
    conn.execute(
        "INSERT INTO POSITIONS (ACCOUNT_ID, CONID, POSITION, AVG_COST, UPDATED_AT) \
         VALUES (?1, ?2, ?3, ?4, datetime('now')) \
         ON CONFLICT (ACCOUNT_ID, CONID) DO UPDATE SET \
            POSITION = excluded.POSITION, AVG_COST = excluded.AVG_COST, \
            UPDATED_AT = datetime('now')",
        params![account_id, conid, position, avg_cost],
    )
    .map(|_| ())
    .map_err(|error| format!("position update failed: {error}"))
}

pub fn set_cash(
    conn: &Connection,
    account_id: &str,
    currency: &str,
    amount: Decimal,
) -> Result<(), String> {
    let amount = scaled(&amount).map_err(|error| error.to_string())?;
    let currency = currency.to_uppercase();
    conn.execute(
        "INSERT INTO CASH_BALANCES (ACCOUNT_ID, CURRENCY, CASH, UPDATED_AT) \
         VALUES (?1, ?2, ?3, datetime('now')) \
         ON CONFLICT (ACCOUNT_ID, CURRENCY) DO UPDATE SET \
            CASH = excluded.CASH, UPDATED_AT = datetime('now')",
        params![account_id, currency, amount],
    )
    .map_err(|error| error.to_string())?;
    Ok(())
}

pub fn list_positions(conn: &Connection, account_id: Option<&str>) -> Vec<Position> {
    let mut stmt = conn
        .prepare(
            "SELECT ACCOUNT_ID, CONID, POSITION, AVG_COST FROM POSITIONS \
             WHERE (?1 IS NULL OR ACCOUNT_ID = ?1) ORDER BY ACCOUNT_ID, CONID",
        )
        .expect("prepare failed");
    collect_rows(&mut stmt, [account_id], |r| {
        Ok(Position {
            account_id: r.get(0)?,
            conid: r.get(1)?,
            position: decimal_at(r, 2),
            avg_cost: optional_decimal_at(r, 3),
        })
    })
}

pub fn list_cash(conn: &Connection, account_id: Option<&str>) -> Vec<CashBalance> {
    let mut stmt = conn
        .prepare(
            "SELECT ACCOUNT_ID, CURRENCY, CASH FROM CASH_BALANCES \
             WHERE (?1 IS NULL OR ACCOUNT_ID = ?1) ORDER BY ACCOUNT_ID, CURRENCY",
        )
        .expect("prepare failed");
    collect_rows(&mut stmt, [account_id], |r| {
        Ok(CashBalance {
            account_id: r.get(0)?,
            currency: r.get(1)?,
            cash: decimal_at(r, 2),
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::purge_expired;
    use std::str::FromStr;

    #[test]
    fn fixed_point_values_are_scaled_without_binary_rounding() {
        assert_eq!(
            scaled(&Decimal::from_str("185.50").unwrap()).unwrap(),
            185_500_000
        );
        assert_eq!(scaled(&Decimal::from_str("0.000001").unwrap()).unwrap(), 1);
    }

    #[test]
    fn values_with_more_than_six_places_are_rejected_for_input() {
        assert!(scaled(&Decimal::from_str("1.0000001").unwrap()).is_err());
        assert_eq!(
            scaled_round(&Decimal::from_str("1.0000006").unwrap()).unwrap(),
            1_000_001
        );
    }

    #[test]
    fn user_account_id_is_stable_and_fits_account_limit() {
        let account_id = user_account_id("9eab5226-3a10-42a8-aed1-5aea54b8b5d3");
        assert_eq!(account_id, "SIM9eab52263a104");
        assert_eq!(account_id.len(), 16);
    }

    #[test]
    fn migration_statements_survive_quotes_and_comments() {
        // A naive `;` split corrupts both of these; the parser must not.
        let statements = split_statements(
            "-- leading comment; with a semicolon\n\
             CREATE TABLE T (A TEXT DEFAULT 'x;y');\n\
             -- trailing comment\n\
             CREATE INDEX IX_T ON T (A);\n",
        );
        assert_eq!(statements.len(), 2);
        assert!(statements[0].contains("'x;y'"));
        assert!(statements[1].contains("IX_T"));
    }

    #[test]
    fn trigger_bodies_stay_in_one_statement() {
        // The trigger body has two internal semicolons and its own END; a naive
        // split would execute the fragments as separate statements and fail.
        let sql = "\
CREATE TABLE AUDIT (ID INTEGER PRIMARY KEY, NOTE TEXT);
CREATE TRIGGER IX_AUDIT_UPDATED AFTER UPDATE ON AUDIT
BEGIN
  INSERT INTO AUDIT (NOTE) VALUES ('touched');
  UPDATE AUDIT SET NOTE = 'done;' WHERE ID = NEW.ID;
END;
CREATE INDEX IX_AUDIT_NOTE ON AUDIT (NOTE);
";
        let statements = split_statements(sql);
        assert_eq!(statements.len(), 3, "{statements:#?}");
        assert!(statements[0].starts_with("CREATE TABLE AUDIT"));
        let trigger = &statements[1];
        assert!(trigger.starts_with("CREATE TRIGGER"));
        assert!(trigger.contains("'touched'"));
        assert!(trigger.contains("'done;'"));
        // The statement's own `;` is the split point and is not retained,
        // matching every other statement this function returns.
        assert!(trigger.trim_end().ends_with("END"), "{trigger}");
        assert!(statements[2].starts_with("CREATE INDEX"));
    }

    #[test]
    fn trigger_keywords_must_stand_alone() {
        // A column named APPENDEND or a table called BEGINNER must not be
        // mistaken for a block boundary, and a trailing comment mentioning
        // BEGIN must not open a block.
        let statements = split_statements(
            "-- this mentions BEGIN and END in prose\n\
             CREATE TABLE BEGINNER (APPENDEND TEXT, BEGIN_LABEL TEXT);\n\
             CREATE TABLE PLAIN (A TEXT);\n",
        );
        assert_eq!(statements.len(), 2, "{statements:#?}");
        assert!(statements[0].contains("BEGINNER"));
        assert!(statements[1].contains("PLAIN"));
    }

    #[test]
    fn a_case_expression_inside_a_trigger_does_not_close_the_block() {
        // `CASE ... END` ends with END, but it is followed by more SQL rather
        // than by `;`, so it must not decrement the block depth.
        let sql = "\
CREATE TRIGGER T AFTER INSERT ON AUDIT
BEGIN
  UPDATE AUDIT SET NOTE = CASE WHEN NEW.NOTE IS NULL THEN 'a' ELSE 'b' END;
END;
";
        let statements = split_statements(sql);
        assert_eq!(statements.len(), 1, "{statements:#?}");
        assert!(statements[0].contains("CASE WHEN"));
        assert!(statements[0].trim_end().ends_with("END"));
    }

    #[test]
    fn init_schema_is_idempotent_and_records_a_ledger() {
        let conn = Connection::open_in_memory().expect("in-memory db");
        init_schema(&conn);
        let applied: i64 = conn
            .query_row("SELECT COUNT(*) FROM SCHEMA_MIGRATIONS", [], |row| {
                row.get(0)
            })
            .expect("ledger query");
        assert_eq!(applied as usize, MIGRATIONS.len());

        // Re-running must neither fail nor re-execute: an existing deployment
        // calls `init-db` on a database created before the ledger existed.
        init_schema(&conn);
        let after: i64 = conn
            .query_row("SELECT COUNT(*) FROM SCHEMA_MIGRATIONS", [], |row| {
                row.get(0)
            })
            .expect("ledger query");
        assert_eq!(after, applied);
    }

    #[test]
    fn init_auth_schema_adds_login_tables_without_dropping_trading_data() {
        let conn = Connection::open_in_memory().expect("in-memory db");
        init_schema(&conn);
        add_account(&conn, "U1", "MARGIN").expect("insert account");
        set_cash(&conn, "U1", "USD", Decimal::from_str("500").unwrap()).unwrap();
        // Simulate a pre-ledger database: the trading tables exist but are
        // not recorded, which is exactly the state of deployed installs.
        conn.execute("DELETE FROM SCHEMA_MIGRATIONS", [])
            .expect("clear ledger");
        conn.execute_batch("DROP TABLE IF EXISTS SESSIONS")
            .expect("drop sessions");

        init_auth_schema(&conn);
        let user_table: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='USERS'",
                [],
                |row| row.get(0),
            )
            .expect("users table");
        assert_eq!(user_table, 1);
        let cash = list_cash(&conn, Some("U1"));
        assert_eq!(cash[0].cash, Decimal::from_str("500").unwrap());
    }

    #[test]
    fn expired_session_and_verification_rows_are_pruned() {
        let conn = Connection::open_in_memory().expect("in-memory db");
        init_schema(&conn);
        conn.execute(
            "INSERT INTO USERS (USER_ID, EMAIL, PASSWORD_HASH) VALUES ('u1','a@b.com','x')",
            [],
        )
        .expect("insert user");
        for (sql, args) in [
            (
                "INSERT INTO SESSIONS (SESSION_ID, USER_ID, TOKEN_HASH, EXPIRES_AT) \
                 VALUES (?1,'u1',?2,?3)",
                vec!["fresh", "hash-fresh", "datetime('now', '+1 day')"],
            ),
            (
                "INSERT INTO SESSIONS (SESSION_ID, USER_ID, TOKEN_HASH, EXPIRES_AT) \
                 VALUES ('stale','u1','hash-stale', datetime('now', '-1 day'))",
                vec![],
            ),
            (
                "INSERT INTO EMAIL_VERIFICATIONS (VERIFICATION_ID, USER_ID, TOKEN_HASH, EXPIRES_AT) \
                 VALUES ('v-stale','u1','vhash-stale', datetime('now', '-1 day'))",
                vec![],
            ),
        ] {
            if args.is_empty() {
                conn.execute(sql, []).expect("insert expired row");
            } else {
                conn.execute(sql, rusqlite::params_from_iter(args)).expect("insert row");
            }
        }

        purge_expired(&conn);

        let sessions: i64 = conn
            .query_row("SELECT COUNT(*) FROM SESSIONS", [], |row| row.get(0))
            .expect("count sessions");
        assert_eq!(sessions, 1, "only the unexpired session should survive");
        let verifications: i64 = conn
            .query_row("SELECT COUNT(*) FROM EMAIL_VERIFICATIONS", [], |row| {
                row.get(0)
            })
            .expect("count verifications");
        assert_eq!(verifications, 0);
    }

    #[test]
    fn sqlite_round_trip_covers_order_fill_position_and_cash() {
        let conn = Connection::open_in_memory().expect("in-memory db");
        conn.execute_batch("PRAGMA foreign_keys = ON;")
            .expect("pragma");
        init_schema(&conn);
        add_account(&conn, "U1234567", "MARGIN").expect("insert account");
        add_contract(
            &conn,
            &Contract {
                conid: 265598,
                symbol: "AAPL".into(),
                sec_type: "STK".into(),
                exchange: "SMART".into(),
                currency: "USD".into(),
            },
        )
        .unwrap();
        set_cash(
            &conn,
            "U1234567",
            "USD",
            Decimal::from_str("100000").unwrap(),
        )
        .unwrap();
        place_order(
            &conn,
            &NewOrder {
                order_id: 1,
                account_id: "U1234567".into(),
                conid: 265598,
                side: "BUY".into(),
                order_type: "LMT".into(),
                quantity: Decimal::from_str("100").unwrap(),
                lmt_price: Some(Decimal::from_str("185.50").unwrap()),
                aux_price: None,
            },
        )
        .unwrap();
        record_fill(
            &conn,
            &NewFill {
                exec_id: "EX-TEST-0001".into(),
                order_id: 1,
                account_id: "U1234567".into(),
                price: Decimal::from_str("185.52").unwrap(),
            },
        )
        .unwrap();
        // Idempotent retry with the same execution ID is a no-op.
        record_fill(
            &conn,
            &NewFill {
                exec_id: "EX-TEST-0001".into(),
                order_id: 1,
                account_id: "U1234567".into(),
                price: Decimal::from_str("185.52").unwrap(),
            },
        )
        .unwrap();

        let orders = list_account_orders(&conn, "U1234567", None);
        assert_eq!(orders.len(), 1);
        assert_eq!(orders[0].status, "Filled");
        assert_eq!(orders[0].total_quantity, Decimal::from_str("100").unwrap());

        let positions = list_positions(&conn, Some("U1234567"));
        assert_eq!(positions.len(), 1);
        assert_eq!(positions[0].position, Decimal::from_str("100").unwrap());
        assert_eq!(
            positions[0].avg_cost,
            Some(Decimal::from_str("185.52").unwrap())
        );

        let cash = list_cash(&conn, Some("U1234567"));
        assert_eq!(cash.len(), 1);
        assert_eq!(
            cash[0].cash,
            Decimal::from_str("100000").unwrap() - Decimal::from_str("18552").unwrap()
        );

        let fills = list_fills(&conn, "U1234567");
        assert_eq!(fills.len(), 1);
        assert_eq!(fills[0].price, Decimal::from_str("185.52").unwrap());
    }

    #[test]
    fn order_ids_allocate_per_account_without_a_row_lock() {
        let conn = Connection::open_in_memory().expect("in-memory db");
        conn.execute_batch("PRAGMA foreign_keys = ON;")
            .expect("pragma");
        init_schema(&conn);
        add_account(&conn, "U1", "MARGIN").expect("insert account");
        assert_eq!(next_order_id(&conn, "U1").unwrap(), 1);
        add_contract(
            &conn,
            &Contract {
                conid: 1,
                symbol: "AAPL".into(),
                sec_type: "STK".into(),
                exchange: "SMART".into(),
                currency: "USD".into(),
            },
        )
        .unwrap();
        place_order(
            &conn,
            &NewOrder {
                order_id: 1,
                account_id: "U1".into(),
                conid: 1,
                side: "BUY".into(),
                order_type: "MKT".into(),
                quantity: Decimal::from_str("10").unwrap(),
                lmt_price: None,
                aux_price: None,
            },
        )
        .unwrap();
        assert_eq!(next_order_id(&conn, "U1").unwrap(), 2);
    }
}

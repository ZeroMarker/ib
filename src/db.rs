use crate::models::*;
use oracle::{Connection, Statement};

fn exec(conn: &Connection, sql: &str, params: &[&dyn oracle::sql_type::ToSql]) {
    conn.execute(sql, params).unwrap_or_else(|e| panic!("SQL failed: {e}\n{sql}"));
}

fn rows(stmt: &mut Statement, mut f: impl FnMut(&oracle::Row)) {
    let mut iter = stmt.query(&[]).expect("query failed");
    while let Some(row) = iter.next() {
        f(&row.expect("row error"));
    }
}

pub fn init_schema(conn: &mut Connection) {
    let sql = include_str!("../migrations/001_ibkr_schema.sql");
    for part in sql.split(';') {
        // Strip leading comment lines, keep the actual statement text.
        let stmt: String = part
            .lines()
            .filter(|l| !l.trim_start().starts_with("--"))
            .collect::<Vec<_>>()
            .join("\n")
            .trim()
            .to_string();
        if !stmt.is_empty() {
            exec(conn, &stmt, &[]);
            println!("ok: {}", stmt.lines().next().unwrap_or("").trim());
        }
    }
    conn.commit().unwrap();
    println!("schema created");
}

pub fn drop_schema(conn: &mut Connection) {
    for t in ["CASH_BALANCES", "POSITIONS", "FILLS", "ORDERS", "CONTRACTS", "ACCOUNTS"] {
        let sql = format!("DROP TABLE {t}");
        match conn.execute(&sql, &[]) {
            Ok(_) => println!("dropped {t}"),
            Err(e) => println!("skip {t}: {}", e),
        }
    }
    conn.commit().unwrap();
}

pub fn add_account(conn: &Connection, account_id: &str, account_type: &str) {
    exec(
        conn,
        "INSERT INTO ACCOUNTS (ACCOUNT_ID, ACCOUNT_TYPE) VALUES (:1, :2)",
        &[&account_id, &account_type.to_uppercase().as_str()],
    );
    conn.commit().unwrap();
}

pub fn list_accounts(conn: &Connection) -> Vec<Account> {
    let mut stmt = conn
        .statement("SELECT ACCOUNT_ID, ACCOUNT_TYPE, CURRENCY, STATUS FROM ACCOUNTS ORDER BY ACCOUNT_ID")
        .build()
        .unwrap();
    let mut out = Vec::new();
    rows(&mut stmt, |r| out.push(Account {
        account_id: r.get(0).unwrap(),
        account_type: r.get(1).unwrap(),
        currency: r.get(2).unwrap(),
        status: r.get(3).unwrap(),
    }));
    out
}

pub fn add_contract(conn: &Connection, c: &Contract) {
    exec(
        conn,
        "INSERT INTO CONTRACTS (CONID, SYMBOL, SEC_TYPE, EXCHANGE, CURRENCY) \
         VALUES (:1, :2, :3, :4, :5)",
        &[&c.conid, &c.symbol, &c.sec_type.as_str(), &c.exchange.as_str(), &c.currency.as_str()],
    );
    conn.commit().unwrap();
}

pub fn list_contracts(conn: &Connection) -> Vec<Contract> {
    let mut stmt = conn
        .statement("SELECT CONID, SYMBOL, SEC_TYPE, EXCHANGE, CURRENCY FROM CONTRACTS ORDER BY CONID")
        .build()
        .unwrap();
    let mut out = Vec::new();
    rows(&mut stmt, |r| out.push(Contract {
        conid: r.get(0).unwrap(),
        symbol: r.get(1).unwrap(),
        sec_type: r.get(2).unwrap(),
        exchange: r.get(3).unwrap(),
        currency: r.get(4).unwrap(),
    }));
    out
}

pub fn place_order(conn: &Connection, o: &NewOrder) {
    // IBKR-style validation: LMT requires lmtPrice; STP requires auxPrice.
    match o.order_type.as_str() {
        "LMT" | "STP_LMT" => assert!(o.lmt_price.is_some(), "{o:?} requires LMT_PRICE"),
        _ => {}
    }
    if o.order_type == "STP" || o.order_type == "STP_LMT" {
        assert!(o.aux_price.is_some(), "{} requires AUX_PRICE", o.order_type);
    }
    assert!(["BUY", "SELL"].contains(&o.side.as_str()), "side must be BUY or SELL");

    exec(
        conn,
        "INSERT INTO ORDERS (ORDER_ID, ACCOUNT_ID, CONID, SIDE, ORDER_TYPE, \
                             LMT_PRICE, AUX_PRICE, TOTAL_QUANTITY, STATUS) \
         VALUES (:1, :2, :3, :4, :5, :6, :7, :8, 'Submitted')",
        &[
            &o.order_id, &o.account_id.as_str(), &o.conid,
            &o.side.as_str(), &o.order_type.as_str(),
            &o.lmt_price, &o.aux_price, &o.quantity,
        ],
    );
    conn.commit().unwrap();
}

const ORDER_SELECT: &str = "SELECT ORDER_ID, PERM_ID, ACCOUNT_ID, CONID, SIDE, ORDER_TYPE, \
     TOTAL_QUANTITY, FILLED_QUANTITY, LMT_PRICE, AUX_PRICE, STATUS FROM ORDERS";

pub fn list_orders(conn: &Connection, status: Option<&str>) -> Vec<Order> {
    let (sql, has_filter) = match status {
        Some(_) => (
            format!("{ORDER_SELECT} WHERE STATUS = :1 ORDER BY ORDER_ID"),
            true,
        ),
        None => (format!("{ORDER_SELECT} ORDER BY ORDER_ID"), false),
    };
    let mut stmt = conn.statement(&sql).build().unwrap();
    if has_filter {
        stmt.execute(&[&status]).expect("bind failed");
    }
    let mut out = Vec::new();
    rows(&mut stmt, |r| out.push(Order {
        order_id: r.get(0).unwrap(),
        perm_id: r.get(1).unwrap(),
        account_id: r.get(2).unwrap(),
        conid: r.get(3).unwrap(),
        side: r.get(4).unwrap(),
        order_type: r.get(5).unwrap(),
        total_quantity: r.get::<usize, f64>(6).unwrap_or_default(),
        filled_quantity: r.get::<usize, f64>(7).unwrap_or_default(),
        lmt_price: r.get(8).unwrap(),
        aux_price: r.get(9).unwrap(),
        status: r.get(10).unwrap(),
    }));
    out
}

pub fn cancel_order(conn: &Connection, order_id: i64, account_id: &str) {
    let n = conn
        .execute(
            "UPDATE ORDERS SET STATUS = 'Cancelled', UPDATED_AT = SYSTIMESTAMP \
             WHERE ORDER_ID = :1 AND ACCOUNT_ID = :2 AND STATUS NOT IN ('Filled','Cancelled')",
            &[&order_id, &account_id],
        )
        .expect("cancel failed")
        .row_count()
        .unwrap_or(0);
    conn.commit().unwrap();
    if n == 0 {
        panic!("order {order_id} not cancellable in account {account_id}");
    }
}

/// Record a fill and roll up order + position + cash like a simplified matching engine.
pub fn record_fill(conn: &Connection, f: &NewFill) {
    let tx = || -> Result<(), Box<dyn std::error::Error>> {
        // Lock the order row.
        let mut stmt = conn
            .statement(
                "SELECT CONID, SIDE, TOTAL_QUANTITY, FILLED_QUANTITY FROM ORDERS \
                 WHERE ORDER_ID = :1 AND ACCOUNT_ID = :2 FOR UPDATE",
            )
            .build()?;
        stmt.query(&[&f.order_id, &f.account_id.as_str()])?;
        let row = stmt.query(&[]).expect("order not found").next().expect("order not found")?;
        let conid: i64 = row.get(0)?;
        let side: String = row.get(1)?;
        let total: f64 = row.get::<usize, f64>(2).unwrap_or_default();
        let filled: f64 = row.get::<usize, f64>(3).unwrap_or_default();

        // Remaining quantity is the fill size.
        let remaining = total - filled;
        assert!(remaining > 0.0, "order already fully filled");

        exec(
            conn,
            "INSERT INTO FILLS (EXEC_ID, ORDER_ID, ACCOUNT_ID, CONID, SIDE, QUANTITY, PRICE) \
             VALUES (:1, :2, :3, :4, :5, :6, :7)",
            &[&f.exec_id, &f.order_id, &f.account_id.as_str(), &conid, &side.as_str(), &remaining, &f.price],
        );

        let new_filled = total;
        let new_status = "Filled";
        exec(
            conn,
            "UPDATE ORDERS SET FILLED_QUANTITY = :1, AVG_FILL_PRICE = :2, \
                    STATUS = :3, UPDATED_AT = SYSTIMESTAMP \
             WHERE ORDER_ID = :4 AND ACCOUNT_ID = :5",
            &[&new_filled, &f.price, &new_status, &f.order_id, &f.account_id.as_str()],
        );

        // Update position (signed).
        let signed_qty = if side == "BUY" { remaining } else { -remaining };
        exec(
            conn,
            "MERGE INTO POSITIONS P USING (SELECT :1 A, :2 C, :3 Q FROM dual) S \
               ON (P.ACCOUNT_ID = S.A AND P.CONID = S.C) \
             WHEN MATCHED THEN UPDATE SET P.POSITION = P.POSITION + S.Q, \
                P.AVG_COST = :4, P.UPDATED_AT = SYSTIMESTAMP \
             WHEN NOT MATCHED THEN INSERT (ACCOUNT_ID, CONID, POSITION, AVG_COST) \
                VALUES (S.A, S.C, S.Q, :4)",
            &[&f.account_id.as_str(), &conid, &signed_qty, &f.price],
        );

        // Update cash: BUY reduces cash, SELL increases cash (ignoring multiplier).
        let currency = contract_currency(conn, conid);
        let cash_delta = if side == "BUY" { -(remaining * f.price) } else { remaining * f.price };
        exec(
            conn,
            "MERGE INTO CASH_BALANCES B USING (SELECT :1 A, :2 CUR, :3 AMT FROM dual) S \
               ON (B.ACCOUNT_ID = S.A AND B.CURRENCY = S.CUR) \
             WHEN MATCHED THEN UPDATE SET B.CASH = B.CASH + S.AMT, B.UPDATED_AT = SYSTIMESTAMP \
             WHEN NOT MATCHED THEN INSERT (ACCOUNT_ID, CURRENCY, CASH) VALUES (S.A, S.CUR, S.AMT)",
            &[&f.account_id.as_str(), &currency, &cash_delta],
        );

        Ok(())
    };
    tx().expect("fill failed");
    conn.commit().unwrap();
}

fn contract_currency(conn: &Connection, conid: i64) -> String {
    let mut stmt = conn
        .statement("SELECT CURRENCY FROM CONTRACTS WHERE CONID = :1")
        .build()
        .unwrap();
    let mut it = stmt.query(&[&conid]).unwrap();
    match it.next() {
        Some(row) => row.unwrap().get(0).unwrap(),
        None => "USD".to_string(),
    }
}

pub fn set_position(
    conn: &Connection,
    account_id: &str,
    conid: i64,
    position: f64,
    avg_cost: Option<f64>,
) {
    exec(
        conn,
        "MERGE INTO POSITIONS P USING (SELECT :1 A, :2 C, :3 Q, :4 AC FROM dual) S \
           ON (P.ACCOUNT_ID = S.A AND P.CONID = S.C) \
         WHEN MATCHED THEN UPDATE SET P.POSITION = S.Q, P.AVG_COST = S.AC, \
            P.UPDATED_AT = SYSTIMESTAMP \
         WHEN NOT MATCHED THEN INSERT (ACCOUNT_ID, CONID, POSITION, AVG_COST) \
            VALUES (S.A, S.C, S.Q, S.AC)",
        &[&account_id, &conid, &position, &avg_cost],
    );
    conn.commit().unwrap();
}

pub fn set_cash(conn: &Connection, account_id: &str, currency: &str, amount: f64) {
    exec(
        conn,
        "MERGE INTO CASH_BALANCES B USING (SELECT :1 A, :2 CUR, :3 AMT FROM dual) S \
           ON (B.ACCOUNT_ID = S.A AND B.CURRENCY = S.CUR) \
         WHEN MATCHED THEN UPDATE SET B.CASH = S.AMT, B.UPDATED_AT = SYSTIMESTAMP \
         WHEN NOT MATCHED THEN INSERT (ACCOUNT_ID, CURRENCY, CASH) VALUES (S.A, S.CUR, S.AMT)",
        &[&account_id, &currency.to_uppercase().as_str(), &amount],
    );
    conn.commit().unwrap();
}

pub fn list_positions(conn: &Connection, account_id: Option<&str>) -> Vec<Position> {
    let (sql, filtered) = match account_id {
        Some(_) => (
            "SELECT ACCOUNT_ID, CONID, POSITION, AVG_COST FROM POSITIONS WHERE ACCOUNT_ID = :1 ORDER BY CONID"
                .to_string(),
            true,
        ),
        None => (
            "SELECT ACCOUNT_ID, CONID, POSITION, AVG_COST FROM POSITIONS ORDER BY ACCOUNT_ID, CONID"
                .to_string(),
            false,
        ),
    };
    let mut stmt = conn.statement(&sql).build().unwrap();
    if filtered {
        stmt.execute(&[&account_id]).unwrap();
    }
    let mut out = Vec::new();
    rows(&mut stmt, |r| out.push(Position {
        account_id: r.get(0).unwrap(),
        conid: r.get(1).unwrap(),
        position: r.get::<usize, f64>(2).unwrap_or_default(),
        avg_cost: r.get(3).unwrap(),
    }));
    out
}

pub fn list_cash(conn: &Connection, account_id: Option<&str>) -> Vec<CashBalance> {
    let (sql, filtered) = match account_id {
        Some(_) => (
            "SELECT ACCOUNT_ID, CURRENCY, CASH FROM CASH_BALANCES WHERE ACCOUNT_ID = :1 ORDER BY CURRENCY"
                .to_string(),
            true,
        ),
        None => (
            "SELECT ACCOUNT_ID, CURRENCY, CASH FROM CASH_BALANCES ORDER BY ACCOUNT_ID, CURRENCY"
                .to_string(),
            false,
        ),
    };
    let mut stmt = conn.statement(&sql).build().unwrap();
    if filtered {
        stmt.execute(&[&account_id]).unwrap();
    }
    let mut out = Vec::new();
    rows(&mut stmt, |r| out.push(CashBalance {
        account_id: r.get(0).unwrap(),
        currency: r.get(1).unwrap(),
        cash: r.get::<usize, f64>(2).unwrap_or_default(),
    }));
    out
}

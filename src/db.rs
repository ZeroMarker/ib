use crate::models::*;
use oracle::{Connection, Statement};
use rust_decimal::prelude::ToPrimitive;
use rust_decimal::Decimal;
use std::error::Error;

const DECIMAL_SCALE: i64 = 1_000_000;

fn exec(conn: &Connection, sql: &str, params: &[&dyn oracle::sql_type::ToSql]) {
    conn.execute(sql, params)
        .unwrap_or_else(|e| panic!("SQL failed: {e}\n{sql}"));
}

fn rows(
    stmt: &mut Statement,
    params: &[&dyn oracle::sql_type::ToSql],
    mut f: impl FnMut(&oracle::Row),
) {
    let iter = stmt.query(params).expect("query failed");
    for row in iter {
        f(&row.expect("row error"));
    }
}

fn decimal_at(row: &oracle::Row, index: usize) -> Decimal {
    let value: String = row.get(index).expect("invalid NUMBER value");
    Decimal::from_str_exact(&value).expect("invalid decimal value")
}

fn optional_decimal_at(row: &oracle::Row, index: usize) -> Option<Decimal> {
    let value: Option<String> = row.get(index).expect("invalid nullable NUMBER value");
    value.map(|value| Decimal::from_str_exact(&value).expect("invalid decimal value"))
}

/// Convert a value to the six-decimal fixed-point representation used by the schema.
fn scaled(value: &Decimal) -> Result<i64, Box<dyn Error>> {
    if value.round_dp(6) != *value {
        return Err(format!("value {value} has more than 6 decimal places").into());
    }
    (value * Decimal::from(DECIMAL_SCALE))
        .to_i64()
        .ok_or_else(|| format!("value {value} is outside NUMBER(18,6) range").into())
}

fn scaled_round(value: &Decimal) -> Result<i64, Box<dyn Error>> {
    (value.round_dp(6) * Decimal::from(DECIMAL_SCALE))
        .to_i64()
        .ok_or_else(|| format!("value {value} is outside NUMBER(18,6) range").into())
}

pub fn init_schema(conn: &mut Connection) {
    let sql = include_str!("../migrations/001_ibkr_schema.sql");
    for part in sql.split(';') {
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
    for table in [
        "CASH_BALANCES",
        "POSITIONS",
        "FILLS",
        "ORDERS",
        "CONTRACTS",
        "ACCOUNTS",
    ] {
        let sql = format!("DROP TABLE {table}");
        match conn.execute(&sql, &[]) {
            Ok(_) => println!("dropped {table}"),
            Err(e) => println!("skip {table}: {e}"),
        }
    }
    conn.commit().unwrap();
}

pub fn add_account(conn: &Connection, account_id: &str, account_type: &str) {
    let account_type = account_type.to_uppercase();
    exec(
        conn,
        "INSERT INTO ACCOUNTS (ACCOUNT_ID, ACCOUNT_TYPE) VALUES (:1, :2)",
        &[&account_id, &account_type.as_str()],
    );
    conn.commit().unwrap();
}

pub fn list_accounts(conn: &Connection) -> Vec<Account> {
    let mut stmt = conn
        .statement(
            "SELECT ACCOUNT_ID, ACCOUNT_TYPE, CURRENCY, STATUS FROM ACCOUNTS ORDER BY ACCOUNT_ID",
        )
        .build()
        .unwrap();
    let mut out = Vec::new();
    rows(&mut stmt, &[], |r| {
        out.push(Account {
            account_id: r.get(0).unwrap(),
            account_type: r.get(1).unwrap(),
            currency: r.get(2).unwrap(),
            status: r.get(3).unwrap(),
        });
    });
    out
}

pub fn add_contract(conn: &Connection, c: &Contract) {
    let sec_type = c.sec_type.to_uppercase();
    let exchange = c.exchange.to_uppercase();
    let currency = c.currency.to_uppercase();
    exec(
        conn,
        "INSERT INTO CONTRACTS (CONID, SYMBOL, SEC_TYPE, EXCHANGE, CURRENCY) \
         VALUES (:1, :2, :3, :4, :5)",
        &[
            &c.conid,
            &c.symbol,
            &sec_type.as_str(),
            &exchange.as_str(),
            &currency.as_str(),
        ],
    );
    conn.commit().unwrap();
}

pub fn list_contracts(conn: &Connection) -> Vec<Contract> {
    let mut stmt = conn
        .statement(
            "SELECT CONID, SYMBOL, SEC_TYPE, EXCHANGE, CURRENCY FROM CONTRACTS ORDER BY CONID",
        )
        .build()
        .unwrap();
    let mut out = Vec::new();
    rows(&mut stmt, &[], |r| {
        out.push(Contract {
            conid: r.get(0).unwrap(),
            symbol: r.get(1).unwrap(),
            sec_type: r.get(2).unwrap(),
            exchange: r.get(3).unwrap(),
            currency: r.get(4).unwrap(),
        });
    });
    out
}

pub fn place_order(conn: &Connection, o: &NewOrder) {
    assert!(
        ["MKT", "LMT", "STP", "STP_LMT"].contains(&o.order_type.as_str()),
        "unsupported order type: {}",
        o.order_type
    );
    assert!(o.quantity > Decimal::ZERO, "quantity must be positive");
    match o.order_type.as_str() {
        "LMT" | "STP_LMT" => assert!(o.lmt_price.is_some(), "{o:?} requires LMT_PRICE"),
        _ => {}
    }
    if o.order_type == "STP" || o.order_type == "STP_LMT" {
        assert!(o.aux_price.is_some(), "{} requires AUX_PRICE", o.order_type);
    }
    assert!(
        ["BUY", "SELL"].contains(&o.side.as_str()),
        "side must be BUY or SELL"
    );
    assert!(
        o.lmt_price.is_none_or(|price| price > Decimal::ZERO),
        "limit price must be positive"
    );
    assert!(
        o.aux_price.is_none_or(|price| price > Decimal::ZERO),
        "aux price must be positive"
    );

    let quantity = scaled(&o.quantity).expect("invalid quantity");
    let lmt_price = o
        .lmt_price
        .as_ref()
        .map(|value| scaled(value).expect("invalid limit price"));
    let aux_price = o
        .aux_price
        .as_ref()
        .map(|value| scaled(value).expect("invalid aux price"));

    exec(
        conn,
        "INSERT INTO ORDERS (ORDER_ID, ACCOUNT_ID, CONID, SIDE, ORDER_TYPE, \
                             LMT_PRICE, AUX_PRICE, TOTAL_QUANTITY, STATUS) \
         VALUES (:1, :2, :3, :4, :5, :6 / 1000000, :7 / 1000000, \
                 :8 / 1000000, 'Submitted')",
        &[
            &o.order_id,
            &o.account_id.as_str(),
            &o.conid,
            &o.side.as_str(),
            &o.order_type.as_str(),
            &lmt_price,
            &aux_price,
            &quantity,
        ],
    );
    conn.commit().unwrap();
}

const ORDER_SELECT: &str = "SELECT ORDER_ID, PERM_ID, ACCOUNT_ID, CONID, SIDE, ORDER_TYPE, \
     TOTAL_QUANTITY, FILLED_QUANTITY, LMT_PRICE, AUX_PRICE, STATUS FROM ORDERS";

pub fn list_orders(conn: &Connection, status: Option<&str>) -> Vec<Order> {
    let sql = match status {
        Some(_) => format!("{ORDER_SELECT} WHERE STATUS = :1 ORDER BY ORDER_ID"),
        None => format!("{ORDER_SELECT} ORDER BY ORDER_ID"),
    };
    let mut stmt = conn.statement(&sql).build().unwrap();
    let status_param = status.map(str::to_owned);
    let params: Vec<&dyn oracle::sql_type::ToSql> = status_param
        .as_ref()
        .map(|value| vec![value as &dyn oracle::sql_type::ToSql])
        .unwrap_or_default();
    let mut out = Vec::new();
    rows(&mut stmt, &params, |r| {
        out.push(Order {
            order_id: r.get(0).unwrap(),
            perm_id: r.get(1).unwrap(),
            account_id: r.get(2).unwrap(),
            conid: r.get(3).unwrap(),
            side: r.get(4).unwrap(),
            order_type: r.get(5).unwrap(),
            total_quantity: decimal_at(r, 6),
            filled_quantity: decimal_at(r, 7),
            lmt_price: optional_decimal_at(r, 8),
            aux_price: optional_decimal_at(r, 9),
            status: r.get(10).unwrap(),
        });
    });
    out
}

pub fn cancel_order(conn: &Connection, order_id: i64, account_id: &str) {
    let n = conn
        .execute(
            "UPDATE ORDERS SET STATUS = 'Cancelled', UPDATED_AT = SYSTIMESTAMP \
             WHERE ORDER_ID = :1 AND ACCOUNT_ID = :2 \
               AND STATUS NOT IN ('Filled','Cancelled')",
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

/// Record a full remaining fill and roll up order, position and cash atomically.
pub fn record_fill(conn: &Connection, f: &NewFill) {
    let transaction = || -> Result<(), Box<dyn Error>> {
        if f.price <= Decimal::ZERO {
            return Err("fill price must be positive".into());
        }
        if f.exec_id.is_empty() || f.exec_id.len() > 24 {
            return Err("execution ID must contain 1 to 24 bytes".into());
        }
        let price_scaled = scaled(&f.price)?;

        // A retry with the same execution ID is a no-op. Reusing it for another fill is rejected.
        let same_fill: i64 = conn.query_row_as(
            "SELECT COUNT(*) FROM FILLS \
             WHERE EXEC_ID = :1 AND ORDER_ID = :2 AND ACCOUNT_ID = :3 \
               AND PRICE = :4 / 1000000",
            &[
                &f.exec_id,
                &f.order_id,
                &f.account_id.as_str(),
                &price_scaled,
            ],
        )?;
        if same_fill > 0 {
            return Ok(());
        }
        let used_exec_id: i64 = conn.query_row_as(
            "SELECT COUNT(*) FROM FILLS WHERE EXEC_ID = :1",
            &[&f.exec_id],
        )?;
        if used_exec_id > 0 {
            return Err(format!("execution ID {} belongs to another fill", f.exec_id).into());
        }

        let mut stmt = conn
            .statement(
                "SELECT CONID, SIDE, TOTAL_QUANTITY, FILLED_QUANTITY FROM ORDERS \
                 WHERE ORDER_ID = :1 AND ACCOUNT_ID = :2 \
                   AND STATUS IN ('Submitted', 'PreSubmitted') FOR UPDATE",
            )
            .build()?;
        let row = stmt.query_row(&[&f.order_id, &f.account_id.as_str()])?;
        let conid: i64 = row.get(0)?;
        let side: String = row.get(1)?;
        let total = decimal_at(&row, 2);
        let filled = decimal_at(&row, 3);
        let remaining = total - filled;
        if remaining <= Decimal::ZERO {
            return Err("order already fully filled".into());
        }
        let remaining_scaled = scaled(&remaining)?;
        let total_scaled = scaled(&total)?;

        conn.execute(
            "INSERT INTO FILLS (EXEC_ID, ORDER_ID, ACCOUNT_ID, CONID, SIDE, QUANTITY, PRICE) \
             VALUES (:1, :2, :3, :4, :5, :6 / 1000000, :7 / 1000000)",
            &[
                &f.exec_id,
                &f.order_id,
                &f.account_id.as_str(),
                &conid,
                &side.as_str(),
                &remaining_scaled,
                &price_scaled,
            ],
        )?;

        conn.execute(
            "UPDATE ORDERS SET FILLED_QUANTITY = :1 / 1000000, \
                    AVG_FILL_PRICE = :2 / 1000000, STATUS = 'Filled', \
                    UPDATED_AT = SYSTIMESTAMP \
             WHERE ORDER_ID = :3 AND ACCOUNT_ID = :4 \
               AND STATUS IN ('Submitted', 'PreSubmitted')",
            &[
                &total_scaled,
                &price_scaled,
                &f.order_id,
                &f.account_id.as_str(),
            ],
        )?;

        let signed_qty = if side == "BUY" {
            remaining_scaled
        } else {
            -remaining_scaled
        };
        conn.execute(
            "MERGE INTO POSITIONS P USING (SELECT :1 A, :2 C, \
                                                   :3 / 1000000 Q, :4 / 1000000 PRICE FROM dual) S \
               ON (P.ACCOUNT_ID = S.A AND P.CONID = S.C) \
             WHEN MATCHED THEN UPDATE SET P.POSITION = P.POSITION + S.Q, \
                P.AVG_COST = CASE \
                    WHEN P.POSITION + S.Q = 0 THEN NULL \
                    WHEN P.POSITION = 0 \
                      OR (SIGN(P.POSITION) <> SIGN(S.Q) \
                          AND SIGN(P.POSITION + S.Q) <> SIGN(P.POSITION)) THEN S.PRICE \
                    WHEN SIGN(P.POSITION) = SIGN(S.Q) THEN \
                      (ABS(P.POSITION) * NVL(P.AVG_COST, S.PRICE) \
                       + ABS(S.Q) * S.PRICE) / (ABS(P.POSITION) + ABS(S.Q)) \
                    ELSE P.AVG_COST END, \
                P.UPDATED_AT = SYSTIMESTAMP \
             WHEN NOT MATCHED THEN INSERT (ACCOUNT_ID, CONID, POSITION, AVG_COST) \
                VALUES (S.A, S.C, S.Q, S.PRICE)",
            &[&f.account_id.as_str(), &conid, &signed_qty, &price_scaled],
        )?;

        let (currency, multiplier) = contract_details(conn, conid)?;
        let cash_delta = if side == "BUY" {
            -(remaining * f.price * multiplier)
        } else {
            remaining * f.price * multiplier
        };
        let cash_delta_scaled = scaled_round(&cash_delta)?;
        conn.execute(
            "MERGE INTO CASH_BALANCES B USING (SELECT :1 A, :2 CUR, \
                                                   :3 / 1000000 AMT FROM dual) S \
               ON (B.ACCOUNT_ID = S.A AND B.CURRENCY = S.CUR) \
             WHEN MATCHED THEN UPDATE SET B.CASH = B.CASH + S.AMT, \
                B.UPDATED_AT = SYSTIMESTAMP \
             WHEN NOT MATCHED THEN INSERT (ACCOUNT_ID, CURRENCY, CASH) \
                VALUES (S.A, S.CUR, S.AMT)",
            &[&f.account_id.as_str(), &currency, &cash_delta_scaled],
        )?;

        Ok(())
    };

    if let Err(error) = transaction() {
        let _ = conn.rollback();
        panic!("fill failed: {error}");
    }
    if let Err(error) = conn.commit() {
        let _ = conn.rollback();
        panic!("fill commit failed: {error}");
    }
}

fn contract_details(conn: &Connection, conid: i64) -> Result<(String, Decimal), Box<dyn Error>> {
    let (currency, multiplier): (String, String) = conn.query_row_as(
        "SELECT CURRENCY, MULTIPLIER FROM CONTRACTS WHERE CONID = :1",
        &[&conid],
    )?;
    Ok((currency, Decimal::from_str_exact(&multiplier)?))
}

pub fn set_position(
    conn: &Connection,
    account_id: &str,
    conid: i64,
    position: Decimal,
    avg_cost: Option<Decimal>,
) {
    let position = scaled(&position).expect("invalid position");
    let avg_cost = avg_cost.map(|value| scaled(&value).expect("invalid average cost"));
    exec(
        conn,
        "MERGE INTO POSITIONS P USING (SELECT :1 A, :2 C, \
                                              :3 / 1000000 Q, :4 / 1000000 AC FROM dual) S \
           ON (P.ACCOUNT_ID = S.A AND P.CONID = S.C) \
         WHEN MATCHED THEN UPDATE SET P.POSITION = S.Q, P.AVG_COST = S.AC, \
            P.UPDATED_AT = SYSTIMESTAMP \
         WHEN NOT MATCHED THEN INSERT (ACCOUNT_ID, CONID, POSITION, AVG_COST) \
            VALUES (S.A, S.C, S.Q, S.AC)",
        &[&account_id, &conid, &position, &avg_cost],
    );
    conn.commit().unwrap();
}

pub fn set_cash(conn: &Connection, account_id: &str, currency: &str, amount: Decimal) {
    let amount = scaled(&amount).expect("invalid cash amount");
    let currency = currency.to_uppercase();
    exec(
        conn,
        "MERGE INTO CASH_BALANCES B USING (SELECT :1 A, :2 CUR, \
                                               :3 / 1000000 AMT FROM dual) S \
           ON (B.ACCOUNT_ID = S.A AND B.CURRENCY = S.CUR) \
         WHEN MATCHED THEN UPDATE SET B.CASH = S.AMT, B.UPDATED_AT = SYSTIMESTAMP \
         WHEN NOT MATCHED THEN INSERT (ACCOUNT_ID, CURRENCY, CASH) VALUES (S.A, S.CUR, S.AMT)",
        &[&account_id, &currency.as_str(), &amount],
    );
    conn.commit().unwrap();
}

pub fn list_positions(conn: &Connection, account_id: Option<&str>) -> Vec<Position> {
    let sql = match account_id {
        Some(_) => {
            "SELECT ACCOUNT_ID, CONID, POSITION, AVG_COST FROM POSITIONS \
                    WHERE ACCOUNT_ID = :1 ORDER BY CONID"
        }
        None => {
            "SELECT ACCOUNT_ID, CONID, POSITION, AVG_COST FROM POSITIONS \
                 ORDER BY ACCOUNT_ID, CONID"
        }
    };
    let mut stmt = conn.statement(sql).build().unwrap();
    let account_param = account_id.map(str::to_owned);
    let params: Vec<&dyn oracle::sql_type::ToSql> = account_param
        .as_ref()
        .map(|value| vec![value as &dyn oracle::sql_type::ToSql])
        .unwrap_or_default();
    let mut out = Vec::new();
    rows(&mut stmt, &params, |r| {
        out.push(Position {
            account_id: r.get(0).unwrap(),
            conid: r.get(1).unwrap(),
            position: decimal_at(r, 2),
            avg_cost: optional_decimal_at(r, 3),
        });
    });
    out
}

pub fn list_cash(conn: &Connection, account_id: Option<&str>) -> Vec<CashBalance> {
    let sql = match account_id {
        Some(_) => {
            "SELECT ACCOUNT_ID, CURRENCY, CASH FROM CASH_BALANCES \
                    WHERE ACCOUNT_ID = :1 ORDER BY CURRENCY"
        }
        None => {
            "SELECT ACCOUNT_ID, CURRENCY, CASH FROM CASH_BALANCES \
                 ORDER BY ACCOUNT_ID, CURRENCY"
        }
    };
    let mut stmt = conn.statement(sql).build().unwrap();
    let account_param = account_id.map(str::to_owned);
    let params: Vec<&dyn oracle::sql_type::ToSql> = account_param
        .as_ref()
        .map(|value| vec![value as &dyn oracle::sql_type::ToSql])
        .unwrap_or_default();
    let mut out = Vec::new();
    rows(&mut stmt, &params, |r| {
        out.push(CashBalance {
            account_id: r.get(0).unwrap(),
            currency: r.get(1).unwrap(),
            cash: decimal_at(r, 2),
        });
    });
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixed_point_values_are_scaled_without_binary_rounding() {
        assert_eq!(
            scaled(&Decimal::from_str_exact("185.50").unwrap()).unwrap(),
            185_500_000
        );
        assert_eq!(
            scaled(&Decimal::from_str_exact("0.000001").unwrap()).unwrap(),
            1
        );
    }

    #[test]
    fn values_with_more_than_six_places_are_rejected_for_input() {
        assert!(scaled(&Decimal::from_str_exact("1.0000001").unwrap()).is_err());
        assert_eq!(
            scaled_round(&Decimal::from_str_exact("1.0000006").unwrap()).unwrap(),
            1_000_001
        );
    }
}

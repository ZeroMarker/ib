mod auth;
mod db;
mod models;
mod trading;
mod web;

use models::*;
use oracle::{pool::PoolBuilder, Connection, InitParams};
use rust_decimal::Decimal;
use std::env;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::time::Duration;

fn usage() -> ! {
    eprintln!(
        "usage: ib <command> [args]

commands:
  ping                                test Oracle connection
  serve [ADDR]                        run web frontend and auth API (default 127.0.0.1:8081)
  init-db                             create simulation trading schema
  init-auth                           add user and session tables to an existing database
  drop-db                             drop all schema tables
  account add <ACCOUNT_ID> [TYPE]     TYPE: CASH|MARGIN|IRA
  account list
  contract add <CONID> <SYMBOL> <SEC_TYPE> [EXCHANGE] [CURRENCY]
  contract list
  order place <ORDER_ID> <ACCOUNT_ID> <CONID> <BUY|SELL> <MKT|LMT|STP|STP_LMT> \
<QTY> [LMT_PRICE] [AUX_PRICE]
  order list [STATUS]
  order cancel <ORDER_ID> <ACCOUNT_ID>
  fill add <ORDER_ID> <ACCOUNT_ID> <PRICE> [EXEC_ID]
  position set <ACCOUNT_ID> <CONID> <POSITION> [AVG_COST]
  position list [ACCOUNT_ID]
  cash set <ACCOUNT_ID> <CURRENCY> <AMOUNT>
  cash list [ACCOUNT_ID]"
    );
    std::process::exit(2);
}

fn decimal_arg(value: &str) -> Decimal {
    Decimal::from_str(value).unwrap_or_else(|_| usage())
}

fn database_config() -> (String, String, String, PathBuf) {
    (
        env::var("DB_USER").expect("DB_USER not set"),
        env::var("DB_PASSWORD").expect("DB_PASSWORD not set"),
        env::var("DB_DSN").expect("DB_DSN not set"),
        PathBuf::from(env::var("DB_WALLET_DIR").expect("DB_WALLET_DIR not set")),
    )
}

fn init_oracle_client(wallet_dir: &Path) {
    assert!(
        wallet_dir.join("tnsnames.ora").is_file(),
        "wallet file missing: {}",
        wallet_dir.join("tnsnames.ora").display()
    );
    assert!(
        ["cwallet.sso", "ewallet.pem", "ewallet.p12"]
            .iter()
            .any(|name| wallet_dir.join(name).is_file()),
        "wallet must contain cwallet.sso, ewallet.pem, or ewallet.p12"
    );

    // Point the Oracle client at the wallet directory: tnsnames.ora is resolved
    // from there and the mTLS wallet is loaded automatically (config dir doubles
    // as wallet location). DB_DSN must be the TNS alias in tnsnames.ora.
    InitParams::new()
        .oracle_client_config_dir(wallet_dir.to_str().expect("wallet dir not UTF-8"))
        .expect("invalid wallet dir")
        .init()
        .expect("Oracle client initialization failed");
}

fn connect() -> Connection {
    let (user, password, dsn, wallet_dir) = database_config();
    init_oracle_client(&wallet_dir);
    Connection::connect(&user, &password, &dsn).expect("Oracle connection failed")
}

fn connect_pool() -> oracle::pool::Pool {
    let (user, password, dsn, wallet_dir) = database_config();
    init_oracle_client(&wallet_dir);
    let max_connections = env::var("DB_POOL_MAX")
        .unwrap_or_else(|_| "8".into())
        .parse::<u32>()
        .expect("DB_POOL_MAX must be a positive integer");
    assert!(
        max_connections > 0,
        "DB_POOL_MAX must be a positive integer"
    );

    let mut builder = PoolBuilder::new(user, password, dsn);
    builder
        .min_connections(1)
        .max_connections(max_connections)
        .connection_increment(1)
        .ping_interval(Some(Duration::from_secs(60)))
        .expect("invalid Oracle pool ping interval")
        .ping_timeout(Duration::from_secs(5))
        .expect("invalid Oracle pool ping timeout")
        .timeout(Duration::from_secs(30))
        .expect("invalid Oracle pool timeout")
        .max_lifetime_connection(Duration::from_secs(3600))
        .expect("invalid Oracle pool connection lifetime")
        .stmt_cache_size(50);
    builder.build().expect("Oracle pool initialization failed")
}

#[tokio::main]
async fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    if args.is_empty() {
        usage();
    }
    if args[0] == "serve" {
        let addr = args
            .get(1)
            .cloned()
            .or_else(|| env::var("SERVER_ADDR").ok())
            .unwrap_or_else(|| "127.0.0.1:8081".into());
        auth::serve(connect_pool(), &addr).await;
        return;
    }

    let mut conn = connect();
    match args[0].as_str() {
        "ping" => {
            let (db_name, schema): (String, String) = conn
                .query_row_as(
                    "SELECT SYS_CONTEXT('USERENV','DB_NAME'), \
                        SYS_CONTEXT('USERENV','CURRENT_SCHEMA') FROM dual",
                    &[],
                )
                .expect("query failed");
            println!(
                "simulation database connection ok: db={} schema={}",
                db_name, schema
            );
        }
        "init-db" => db::init_schema(&mut conn),
        "init-auth" => db::init_auth_schema(&mut conn),
        "drop-db" => db::drop_schema(&mut conn),
        "account" => match args.get(1).map(String::as_str) {
            Some("add") => {
                let id = args.get(2).unwrap_or_else(|| usage());
                let typ = args.get(3).cloned().unwrap_or_else(|| "MARGIN".into());
                db::add_account(&conn, id, &typ);
                println!("account {} ({}) created", id, typ);
            }
            Some("list") => {
                for a in db::list_accounts(&conn) {
                    println!(
                        "{} {:8} {} {}",
                        a.account_id, a.account_type, a.currency, a.status
                    );
                }
            }
            _ => usage(),
        },
        "contract" => match args.get(1).map(String::as_str) {
            Some("add") => {
                let c = Contract {
                    conid: args
                        .get(2)
                        .unwrap_or_else(|| usage())
                        .parse()
                        .unwrap_or_else(|_| usage()),
                    symbol: args.get(3).cloned().unwrap_or_else(|| usage()),
                    sec_type: args.get(4).cloned().unwrap_or_else(|| "STK".into()),
                    exchange: args.get(5).cloned().unwrap_or_else(|| "SMART".into()),
                    currency: args.get(6).cloned().unwrap_or_else(|| "USD".into()),
                };
                db::add_contract(&conn, &c)
                    .unwrap_or_else(|error| panic!("contract failed: {error}"));
                println!("contract {} {} added", c.conid, c.symbol);
            }
            Some("list") => {
                for c in db::list_contracts(&conn) {
                    println!(
                        "{} {:6} {:4} {:8} {}",
                        c.conid, c.symbol, c.sec_type, c.exchange, c.currency
                    );
                }
            }
            _ => usage(),
        },
        "order" => match args.get(1).map(String::as_str) {
            Some("place") => {
                if args.len() < 8 {
                    usage();
                }
                let o = NewOrder {
                    order_id: args[2].parse().unwrap_or_else(|_| usage()),
                    account_id: args[3].clone(),
                    conid: args[4].parse().unwrap_or_else(|_| usage()),
                    side: args[5].to_uppercase(),
                    order_type: args[6].to_uppercase(),
                    quantity: decimal_arg(&args[7]),
                    lmt_price: args.get(8).map(|s| decimal_arg(s)),
                    aux_price: args.get(9).map(|s| decimal_arg(s)),
                };
                db::place_order(&conn, &o).unwrap_or_else(|error| panic!("order failed: {error}"));
                println!("order {} submitted", o.order_id);
            }
            Some("list") => {
                let status = args.get(2).cloned();
                for o in db::list_orders(&conn, status.as_deref()) {
                    println!(
                        "#{} perm={} {} conid={} {:4} {:7} qty={}/{} lmt={:?} aux={:?} {}",
                        o.order_id,
                        o.perm_id
                            .map(|p| p.to_string())
                            .unwrap_or_else(|| "-".into()),
                        o.account_id,
                        o.conid,
                        o.side,
                        o.order_type,
                        o.total_quantity,
                        o.filled_quantity,
                        o.lmt_price,
                        o.aux_price,
                        o.status,
                    );
                }
            }
            Some("cancel") => {
                let order_id: i64 = args
                    .get(2)
                    .unwrap_or_else(|| usage())
                    .parse()
                    .unwrap_or_else(|_| usage());
                let account_id = args.get(3).cloned().unwrap_or_else(|| usage());
                db::cancel_order(&conn, order_id, &account_id)
                    .unwrap_or_else(|error| panic!("cancel failed: {error}"));
                println!("order {} cancelled", order_id);
            }
            _ => usage(),
        },
        "fill" => match args.get(1).map(String::as_str) {
            Some("add") => {
                if args.len() < 5 {
                    usage();
                }
                let f = NewFill {
                    exec_id: args.get(5).cloned().unwrap_or_else(|| {
                        format!(
                            "EX{:016X}",
                            chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
                        )
                    }),
                    order_id: args[2].parse().unwrap_or_else(|_| usage()),
                    account_id: args[3].clone(),
                    price: decimal_arg(&args[4]),
                };
                db::record_fill(&conn, &f).unwrap_or_else(|error| panic!("{error}"));
                println!("fill recorded on order {}", f.order_id);
            }
            _ => usage(),
        },
        "position" => match args.get(1).map(String::as_str) {
            Some("set") => {
                if args.len() < 5 {
                    usage();
                }
                db::set_position(
                    &conn,
                    &args[2],
                    args[3].parse().unwrap_or_else(|_| usage()),
                    decimal_arg(&args[4]),
                    args.get(5).map(|s| decimal_arg(s)),
                );
                println!("position updated");
            }
            Some("list") => {
                for p in db::list_positions(&conn, args.get(2).map(String::as_str)) {
                    println!(
                        "{} conid={} pos={} avg_cost={:?}",
                        p.account_id, p.conid, p.position, p.avg_cost
                    );
                }
            }
            _ => usage(),
        },
        "cash" => match args.get(1).map(String::as_str) {
            Some("set") => {
                if args.len() < 5 {
                    usage();
                }
                db::set_cash(&conn, &args[2], &args[3], decimal_arg(&args[4]))
                    .unwrap_or_else(|error| panic!("cash update failed: {error}"));
                println!("balance updated");
            }
            Some("list") => {
                for b in db::list_cash(&conn, args.get(2).map(String::as_str)) {
                    println!("{} {} {:.2}", b.account_id, b.currency, b.cash);
                }
            }
            _ => usage(),
        },
        _ => usage(),
    }
}

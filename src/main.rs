//! CLI entry point for the simulation trading platform.
//!
//! Command dispatch is a flat `match` over argv; each arm is a thin wrapper
//! that parses arguments and calls into `db`. Argument access goes through
//! `Args` so a missing operand prints usage instead of panicking on an
//! out-of-bounds index.

use ib::db;
use ib::models::*;
use ib::serve;
use rust_decimal::Decimal;
use std::env;
use std::path::PathBuf;
use std::str::FromStr;

/// Positional command-line arguments with usage-exiting accessors.
///
/// The previous dispatch used `args.get(n).unwrap_or_else(|| usage())` and
/// `args[n]` at roughly 30 sites; centralizing it here keeps the parse
/// rules in one place and guarantees every operand is bounds-checked.
struct Args(Vec<String>);

impl Args {
    fn from_env() -> Self {
        Self(env::args().skip(1).collect())
    }

    /// The subcommand keyword, e.g. `order` in `ib order place ...`.
    fn command(&self) -> &str {
        self.0.first().map_or_else(|| usage(), String::as_str)
    }

    fn sub(&self) -> Option<&str> {
        self.0.get(1).map(String::as_str)
    }

    fn opt(&self, index: usize) -> Option<&str> {
        self.0.get(index).map(String::as_str)
    }

    /// A required operand, or usage plus exit.
    fn required(&self, index: usize) -> &str {
        self.opt(index).unwrap_or_else(|| usage())
    }

    /// A required integer operand, or usage plus exit.
    fn int(&self, index: usize) -> i64 {
        self.required(index).parse().unwrap_or_else(|_| usage())
    }

    /// A required decimal operand, or usage plus exit.
    fn decimal(&self, index: usize) -> Decimal {
        decimal_arg(self.required(index))
    }

    /// An optional decimal operand.
    fn optional_decimal(&self, index: usize) -> Option<Decimal> {
        self.opt(index).map(decimal_arg)
    }
}

fn usage() -> ! {
    eprintln!(
        "usage: ib <command> [args]

commands:
  ping                                test SQLite connection
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

fn database_config() -> PathBuf {
    env::var("DB_PATH")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("ib.sqlite3"))
}

fn connect() -> rusqlite::Connection {
    db::open(&database_config())
}

fn connect_pool() -> db::Pool {
    db::Pool::new(connect())
}

#[tokio::main]
async fn main() {
    let args = Args::from_env();

    if args.command() == "serve" {
        let addr = args
            .opt(1)
            .map(str::to_owned)
            .or_else(|| env::var("SERVER_ADDR").ok())
            .unwrap_or_else(|| "127.0.0.1:8081".into());
        serve(connect_pool(), &addr).await;
        return;
    }

    let conn = connect();
    match args.command() {
        "ping" => {
            let version: String = conn
                .query_row("SELECT sqlite_version()", [], |row| row.get(0))
                .expect("query failed");
            println!(
                "simulation database ok: sqlite={} path={}",
                version,
                database_config().display()
            );
        }
        "init-db" => db::init_schema(&conn),
        "init-auth" => db::init_auth_schema(&conn),
        "drop-db" => db::drop_schema(&conn),
        "account" => match args.sub() {
            Some("add") => {
                let id = args.required(2);
                let account_type = args.opt(3).unwrap_or("MARGIN");
                db::add_account(&conn, id, account_type);
                println!("account {} ({}) created", id, account_type);
            }
            Some("list") => {
                for account in db::list_accounts(&conn) {
                    println!(
                        "{} {:8} {} {}",
                        account.account_id, account.account_type, account.currency, account.status
                    );
                }
            }
            _ => usage(),
        },
        "contract" => match args.sub() {
            Some("add") => {
                let contract = Contract {
                    conid: args.int(2),
                    symbol: args.required(3).to_owned(),
                    sec_type: args.opt(4).unwrap_or("STK").to_owned(),
                    exchange: args.opt(5).unwrap_or("SMART").to_owned(),
                    currency: args.opt(6).unwrap_or("USD").to_owned(),
                };
                db::add_contract(&conn, &contract)
                    .unwrap_or_else(|error| panic!("contract failed: {error}"));
                println!("contract {} {} added", contract.conid, contract.symbol);
            }
            Some("list") => {
                for contract in db::list_contracts(&conn) {
                    println!(
                        "{} {:6} {:4} {:8} {}",
                        contract.conid,
                        contract.symbol,
                        contract.sec_type,
                        contract.exchange,
                        contract.currency
                    );
                }
            }
            _ => usage(),
        },
        "order" => match args.sub() {
            Some("place") => {
                let order = NewOrder {
                    order_id: args.int(2),
                    account_id: args.required(3).to_owned(),
                    conid: args.int(4),
                    side: args.required(5).to_uppercase(),
                    order_type: args.required(6).to_uppercase(),
                    quantity: args.decimal(7),
                    lmt_price: args.optional_decimal(8),
                    aux_price: args.optional_decimal(9),
                };
                db::place_order(&conn, &order)
                    .unwrap_or_else(|error| panic!("order failed: {error}"));
                println!("order {} submitted", order.order_id);
            }
            Some("list") => {
                for order in db::list_orders(&conn, args.opt(2)) {
                    println!(
                        "#{} perm={} {} conid={} {:4} {:7} qty={}/{} lmt={:?} aux={:?} {}",
                        order.order_id,
                        order
                            .perm_id
                            .map(|p| p.to_string())
                            .unwrap_or_else(|| "-".into()),
                        order.account_id,
                        order.conid,
                        order.side,
                        order.order_type,
                        order.total_quantity,
                        order.filled_quantity,
                        order.lmt_price,
                        order.aux_price,
                        order.status,
                    );
                }
            }
            Some("cancel") => {
                let order_id = args.int(2);
                db::cancel_order(&conn, order_id, args.required(3))
                    .unwrap_or_else(|error| panic!("cancel failed: {error}"));
                println!("order {order_id} cancelled");
            }
            _ => usage(),
        },
        "fill" => match args.sub() {
            Some("add") => {
                let fill = NewFill {
                    exec_id: args
                        .opt(5)
                        .map(str::to_owned)
                        .unwrap_or_else(default_exec_id),
                    order_id: args.int(2),
                    account_id: args.required(3).to_owned(),
                    price: args.decimal(4),
                };
                db::record_fill(&conn, &fill).unwrap_or_else(|error| panic!("{error}"));
                println!("fill recorded on order {}", fill.order_id);
            }
            _ => usage(),
        },
        "position" => match args.sub() {
            Some("set") => {
                db::set_position(
                    &conn,
                    args.required(2),
                    args.int(3),
                    args.decimal(4),
                    args.optional_decimal(5),
                );
                println!("position updated");
            }
            Some("list") => {
                for position in db::list_positions(&conn, args.opt(2)) {
                    println!(
                        "{} conid={} pos={} avg_cost={:?}",
                        position.account_id, position.conid, position.position, position.avg_cost
                    );
                }
            }
            _ => usage(),
        },
        "cash" => match args.sub() {
            Some("set") => {
                db::set_cash(&conn, args.required(2), args.required(3), args.decimal(4))
                    .unwrap_or_else(|error| panic!("cash update failed: {error}"));
                println!("balance updated");
            }
            Some("list") => {
                for balance in db::list_cash(&conn, args.opt(2)) {
                    println!(
                        "{} {} {:.2}",
                        balance.account_id, balance.currency, balance.cash
                    );
                }
            }
            _ => usage(),
        },
        _ => usage(),
    }
}

/// Fallback execution ID for `ib fill add` when the caller supplies none.
/// The 24-byte cap in `db::record_fill` matches the 18 characters produced here.
fn default_exec_id() -> String {
    format!(
        "EX{:016X}",
        chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
    )
}

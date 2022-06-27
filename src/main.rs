#[macro_use]
extern crate log;

use crate::monitoring::monitor::Monitor;
use crate::notifications::mail;
use crate::utils::config::Config;

use ahash::AHashMap;
use clap::Parser;
use diesel::{prelude::PgConnection, r2d2::ConnectionManager};
use sproot::models::AlertsConfig;
use sproot::{prog, Pool};
use std::sync::RwLock;
use std::{thread, time::Duration};
use websockets::ws_handler::WsHandler;
use websockets::ws_message::{msg_err_handler, msg_ok_database};

mod monitoring;
mod notifications;
mod utils;
mod websockets;

#[derive(Parser, Debug)]
#[clap(author, version, about)]
struct Args {
    #[clap(short = 'c', long = "config")]
    config_path: Option<String>,

    #[clap(flatten)]
    verbose: clap_verbosity_flag::Verbosity,
}

lazy_static::lazy_static! {
    // Lazy static of the Config which is loaded from the config file
    static ref CONFIG: Config = match Config::new() {
        Ok(config) => config,
        Err(e) => {
            error!("Cannot build the Config: {}", e);
            std::process::exit(1);
        }
    };

    // Be warned that it is not guarantee that the task is currently running.
    // The task could have been aborted sooner due to the sanity check of the query.
    static ref RUNNING_ALERT: RwLock<AHashMap<String, tokio::task::JoinHandle<()>>> = RwLock::new(AHashMap::new());
    // List of the AlertsConfig (to be used in the WSS)
    static ref ALERTS_CONFIG: RwLock<Vec<AlertsConfig>> = RwLock::new(Vec::new());
}

fn init_pool() -> Pool {
    // Init the connection to the postgresql
    let manager = ConnectionManager::<PgConnection>::new(&CONFIG.database_url);
    // This step might spam for error CONFIG.database_max_connection of times, this is normal.
    match r2d2::Pool::builder()
        .max_size(CONFIG.database_max_connection)
        .min_idle(Some((10 * CONFIG.database_max_connection) / 100))
        .build(manager)
    {
        Ok(pool) => {
            info!("R2D2 PostgreSQL pool created");
            pool
        }
        Err(e) => {
            error!("Failed to create db pool: {}", e);
            std::process::exit(1);
        }
    }
}

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let args = Args::parse();

    // Init logger
    env_logger::Builder::new()
        .filter_module(
            &prog().map_or_else(|| "speculare_alerts".to_owned(), |f| f.replace('-', "_")),
            args.verbose.log_level_filter(),
        )
        .filter_module("sproot", args.verbose.log_level_filter())
        .init();

    // Initialize the connections'pool (r2d2 sync)
    let pool = init_pool();

    // Build the Ws handler to listen for the alerts table
    let ws_handler = WsHandler {
        query: "*",
        table: "alerts",
        msg_error: msg_err_handler,
        msg_ok: msg_ok_database,
        pool: &pool,
    };

    // Check if the SMTP server host is "ok"
    mail::test_smtp_transport();

    // Build the default monitor struct
    let monitor = Monitor::default(&pool);
    // Run the foreach loop over each alarms and start monitoring them.
    monitor.oneshot();

    let mut count = 0u8;
    loop {
        // Create and start listening on the Websocket
        if let Err(err) = ws_handler.listen().await {
            error!("AlertSource::Database: stream error: {}", err);
        }

        error!("Main loop: attempt to recover the CDC connection, waiting 5s");
        // Avoid spamming CPU in case of crash loop
        thread::sleep(Duration::from_secs(5));

        count += 1;
        if count >= 3 {
            error!("Main loop: boot loop, stopping...");
            std::process::exit(1);
        }
    }
}

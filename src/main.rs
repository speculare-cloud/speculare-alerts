#[macro_use]
extern crate log;

use std::collections::HashMap;
use std::sync::RwLock;
use std::{thread, time::Duration};

use bastion::prelude::ChildrenRef;
use bastion::supervisor::{ActorRestartStrategy, RestartStrategy, SupervisorRef};
use bastion::Bastion;
use clap::Parser;
use diesel::{prelude::PgConnection, r2d2::ConnectionManager};
use once_cell::sync::Lazy;
use sproot::{prog, Pool};
use websockets::ws_handler::WsHandler;
use websockets::ws_message::{msg_err_handler, msg_ok_database};

use crate::monitoring::monitor::Monitor;
use crate::notifications::mail;
use crate::utils::config::Config;

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

// Lazy static of the Config which is loaded from the config file
static CONFIG: Lazy<Config> = Lazy::new(|| match Config::new() {
    Ok(config) => config,
    Err(e) => {
        error!("Cannot build the Config: {}", e);
        std::process::exit(1);
    }
});

static RUNNING_CHILDREN: Lazy<RwLock<HashMap<i64, ChildrenRef>>> =
    Lazy::new(|| RwLock::new(HashMap::new()));

static SUPERVISOR: Lazy<SupervisorRef> = Lazy::new(|| {
    match Bastion::supervisor(|sp| {
        sp.with_restart_strategy(RestartStrategy::default().with_actor_restart_strategy(
            ActorRestartStrategy::LinearBackOff {
                timeout: Duration::from_secs(3),
            },
        ))
    }) {
        Ok(sp) => sp,
        Err(err) => {
            error!("Cannot create the Bastion supervisor: {:?}", err);
            std::process::exit(1);
        }
    }
});

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

    // Define log level
    if std::env::var("RUST_LOG").is_err() {
        std::env::set_var(
            "RUST_LOG",
            format!(
                "{}={level},sproot={level}",
                &prog().map_or_else(|| "speculare_alerts".to_owned(), |f| f.replace('-', "_")),
                level = args.verbose.log_level_filter()
            ),
        )
    }

    // Init logger/tracing
    tracing_subscriber::fmt::init();

    // Initialize the connections'pool (r2d2 sync)
    let pool = init_pool();

    // Init Bastion supervisor
    Bastion::init();
    Bastion::start();

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

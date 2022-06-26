use std::{thread, time::Duration};

use crate::{
    utils::{
        mail::test_smtp_transport,
        monitoring::launch_monitoring,
        websocket::{msg_err_handler, WsHandler},
        ws_alerts::msg_ok_database,
        ws_hosts::msg_ok_files,
    },
    CONFIG,
};

use sproot::{
    models::{AlertSource, Alerts},
    Pool,
};

/// Will start the program normally (launch alerts, ...)
pub async fn flow_run_start(pool: Pool) -> std::io::Result<()> {
    // Check if the SMTP server host is "ok"
    test_smtp_transport();

    // If alerts_source is == Files, we should delete all
    // alerts in the database because they'll be recreated.
    // Deleting and recreating allow us to avoid computing the
    // diff between database and actual to remove the old alerts.
    if CONFIG.alerts_source == AlertSource::Files {
        // Get a connection from the R2D2 pool
        let mut pooled_conn = match pool.get() {
            Ok(pooled) => pooled,
            Err(e) => {
                error!("Cannot get a connection from the pool: {}", e);
                std::process::exit(1);
            }
        };

        match Alerts::delete_all(&mut pooled_conn) {
            Ok(_) => {}
            Err(e) => {
                error!("Cannot delete the alerts from the database: {}", e);
                std::process::exit(1);
            }
        };
    }

    // Launch the monitoring of each alarms
    launch_monitoring(&pool);

    let ws_handler = if CONFIG.alerts_source == AlertSource::Files {
        WsHandler {
            query: "insert,delete",
            table: "hosts",
            msg_error: msg_err_handler,
            msg_ok: msg_ok_files,
            pool: &pool,
        }
    } else {
        WsHandler {
            query: "*",
            table: "alerts",
            msg_error: msg_err_handler,
            msg_ok: msg_ok_database,
            pool: &pool,
        }
    };
    // Creating a loop to attempt to reconnect to CDC if a crash of CDC/network happens.
    // but apply a hard limit of 3 tries
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

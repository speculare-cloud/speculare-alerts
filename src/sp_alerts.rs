use crate::utils::mail::test_smtp_transport;
use crate::utils::monitor::Monitor;
use crate::utils::websocket::{msg_err_handler, WsHandler};
use crate::utils::ws_utils::{msg_ok_database, msg_ok_files};
use crate::CONFIG;

use sproot::models::{AlertSource, Alerts};
use sproot::{ConnType, Pool};
use std::{thread, time::Duration};

pub struct SpAlerts<'a> {
    pub ws_handler: WsHandler<'a>,
    pub pool: Pool,
}

impl<'a> SpAlerts<'a> {
    pub fn default(pool: &'a Pool) -> Self {
        let ws_handler = if CONFIG.alerts_source == AlertSource::Files {
            WsHandler {
                query: "insert,delete",
                table: "hosts",
                msg_error: msg_err_handler,
                msg_ok: msg_ok_files,
                pool,
            }
        } else {
            WsHandler {
                query: "*",
                table: "alerts",
                msg_error: msg_err_handler,
                msg_ok: msg_ok_database,
                pool,
            }
        };

        Self {
            ws_handler,
            pool: pool.to_owned(),
        }
    }
}

impl SpAlerts<'_> {
    fn get_conn(&self) -> ConnType {
        match self.pool.get() {
            Ok(pooled) => pooled,
            Err(err) => {
                error!("Cannot get a connection from the pool: {}", err);
                std::process::exit(1);
            }
        }
    }

    async fn delete_leftover(&self) {
        if let Err(err) = Alerts::delete_all(&mut self.get_conn()) {
            error!("Cannot delete the alerts from the database: {}", err);
            std::process::exit(1);
        }
    }

    pub async fn prepare(&self) {
        // Check if the SMTP server host is "ok"
        test_smtp_transport();

        // If alerts_source is == Files, we should delete all
        // alerts in the database because they'll be recreated.
        // Deleting and recreating allow us to avoid computing the
        // diff between database and actual to remove the old alerts.
        if CONFIG.alerts_source == AlertSource::Files {
            self.delete_leftover().await;
        }
    }

    pub async fn serve(&self) -> std::io::Result<()> {
        let monitor = Monitor::default(&self.pool);
        // Run the foreach loop over each alarms and start monitoring them.
        monitor.oneshot();

        let mut count = 0u8;
        loop {
            // Create and start listening on the Websocket
            if let Err(err) = self.ws_handler.listen().await {
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
}

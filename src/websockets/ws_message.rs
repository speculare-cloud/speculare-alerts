use crate::{
    monitoring::alerts::{AlertsQuery, WholeAlert},
    utils::{CdcChange, CdcKind},
    RUNNING_ALERT,
};

use sproot::Pool;
use std::io::{Error, ErrorKind};
use tokio_tungstenite::tungstenite::Error::{AlreadyClosed, ConnectionClosed, Io as TIo};
use tokio_tungstenite::tungstenite::{Error as TError, Message};

pub fn msg_err_handler(err: TError) -> std::io::Result<()> {
    match err {
        // Consider those kind of error as fatal
        ConnectionClosed | AlreadyClosed | TIo(_) => {
            error!("WebSocket: error is fatal: {}", err);
            Err(Error::new(ErrorKind::Other, err))
        }
        _ => {
            debug!("WebSocket: error is non-fatal: {}", err);
            Ok(())
        }
    }
}

pub fn msg_ok_database(msg: Message, pool: &Pool) {
    // Convert msg into String
    let mut msg = msg.into_text().expect("Cannot convert message to text");
    trace!("Websocket: Message received: \"{}\"", msg);

    // Construct data from str using Serde
    let data: CdcChange = simd_json::from_str(&mut msg).unwrap();

    // Construct alert from CdcChange (using columnname and columnvalues)
    let alert: WholeAlert = match (&data).into() {
        Ok(alert) => {
            let (query, qtype) = alert.get_query();

            WholeAlert {
                inner: alert,
                query,
                qtype,
            }
        }
        Err(err) => {
            error!(
                "Cannot construct the alert with the data from the WS: {}",
                err
            );
            return;
        }
    };

    match data.kind {
        CdcKind::Insert => {
            info!("Websocket: running CdcKind::Insert");
            alert.start_monitoring(pool.clone());
        }
        CdcKind::Update | CdcKind::Delete => {
            info!("Websocket: running CdcKind::Update or CdcKind::Delete");
            {
                // Stop the task's "thread" in it's own scope to drop the lock asap
                let mut running = RUNNING_ALERT.write().unwrap();
                let task = running.remove(&alert.inner.id);
                if let Some(task) = task {
                    task.abort();
                }
            }

            // If it's an Update, start the task again
            if data.kind == CdcKind::Update {
                alert.start_monitoring(pool.clone());
            }
        }
    }
}

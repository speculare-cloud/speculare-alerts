use std::io::{Error, ErrorKind};

use sproot::Pool;
use tokio_tungstenite::tungstenite::Error::{AlreadyClosed, ConnectionClosed, Io as TIo};
use tokio_tungstenite::tungstenite::{Error as TError, Message};

use crate::{
    monitoring::{alerts::WholeAlert, query::AlertsQuery},
    utils::{CdcChange, CdcKind},
};

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
    let data: CdcChange = match simd_json::from_str(&mut msg) {
        Ok(val) => val,
        Err(err) => {
            error!("Failed to parse CdcChange: {} from: {}", err, msg);
            return;
        }
    };

    // Construct alert from CdcChange (using columnname and columnvalues)
    let alert: WholeAlert = match (&data).into() {
        Ok(alert) => {
            let (query, qtype) = match alert.construct_query() {
                Ok((q, t)) => (q, t),
                Err(err) => {
                    error!(
                        "cannot construct the query related to alert {}: {}",
                        alert.id, err
                    );
                    return;
                }
            };

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
            alert.stop_monitoring();

            // If it's an Update, start the task again
            if data.kind == CdcKind::Update {
                alert.start_monitoring(pool.clone());
            }
        }
    }
}

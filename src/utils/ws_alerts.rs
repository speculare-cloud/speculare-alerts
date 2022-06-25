use crate::{
    utils::{alerts::start_alert_task, CdcKind},
    RUNNING_ALERT,
};

use super::CdcChange;

use sproot::{models::Alerts, Pool};
use tokio_tungstenite::tungstenite::Message;

pub fn msg_ok_database(msg: Message, pool: &Pool) {
    // Convert msg into String
    let mut msg = msg.into_text().expect("Cannot convert message to text");
    trace!("Websocket: Message received: \"{}\"", msg);

    // Construct data from str using Serde
    let data: CdcChange = simd_json::from_str(&mut msg).unwrap();

    // Construct alert from CdcChange (using columnname and columnvalues)
    let alert: Alerts = match (&data).into() {
        Ok(alert) => alert,
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
            start_alert_task(alert, pool.clone())
        }
        CdcKind::Update | CdcKind::Delete => {
            info!("Websocket: running CdcKind::Update or CdcKind::Delete");
            {
                // Stop the task's "thread" in it's own scope to drop the lock asap
                let mut running = RUNNING_ALERT.write().unwrap();
                let task = running.remove(&alert.id);
                if let Some(task) = task {
                    task.abort();
                }
            }

            // If it's an Update, start the task again
            if data.kind == CdcKind::Update {
                start_alert_task(alert, pool.clone());
            }
        }
    }
}

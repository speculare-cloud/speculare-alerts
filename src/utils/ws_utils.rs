use crate::{
    utils::{
        alerts::{AlertsQuery, WholeAlert},
        CdcKind,
    },
    ALERTS_CONFIG, RUNNING_ALERT,
};

use super::CdcChange;

use sproot::{
    models::{Alerts, AlertsConfig, HostTargeted},
    Pool,
};
use tokio_tungstenite::tungstenite::Message;

pub fn msg_ok_files(msg: Message, pool: &Pool) {
    // Convert msg into String
    let mut msg = msg.into_text().expect("Cannot convert message to text");
    trace!("Websocket: Message received: \"{}\"", msg);
    // Construct data from str using Serde
    let data: CdcChange = simd_json::from_str(&mut msg).unwrap();
    // Get the host_uuid that received the change
    let host_uuid_idx = data.columnnames.iter().position(|item| item == "uuid");
    if host_uuid_idx.is_none() {
        error!("WebSocket: host_uuid is not present in the CdcChange");
        return;
    }
    let host_uuid = &data.columnnames[host_uuid_idx.unwrap()];
    // Get the hostname that received the change
    let hostname_idx = data.columnnames.iter().position(|item| item == "hostname");
    if hostname_idx.is_none() {
        error!("WebSocket: hostname is not present in the CdcChange");
        return;
    }
    let hostname = &data.columnnames[hostname_idx.unwrap()];

    match data.kind {
        CdcKind::Insert => {
            info!("Websocket: running CdcKind::Insert for {:.6}", host_uuid);
            // Get the ALERTS_CONFIG (read) and filter those with ALL or SPECIFIC(host_uuid);
            // The READ lock will be held for the whole scope
            let alerts_config = &*ALERTS_CONFIG.read().unwrap();
            let matched_config: Vec<&AlertsConfig> = alerts_config
                .iter()
                .filter(|e| match e.host_targeted.as_ref().unwrap() {
                    HostTargeted::ALL => true,
                    HostTargeted::SPECIFIC(uuid) => uuid == host_uuid,
                })
                .collect();

            for config in matched_config {
                // Build the Alerts from the config & hostname & host_uuid
                let alert = Alerts::build_from_config(
                    config.to_owned(),
                    host_uuid.to_owned(),
                    hostname.to_owned(),
                    Alerts::generate_id_from(host_uuid, &config.name),
                );
                let (query, qtype) = alert.get_query();

                let alert = WholeAlert {
                    inner: alert,
                    query,
                    qtype,
                };
                // Start the analysis
                alert.start_monitoring(pool.clone());
            }
        }
        CdcKind::Delete => {
            // TODO
        }
        _ => trace!("WebSocket: CdcKind not supported"),
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

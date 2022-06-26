use crate::{ALERTS_CONFIG, CONFIG, RUNNING_ALERT};

use super::analysis::execute_analysis;
use super::query::construct_query;
use super::QueryType;

use sproot::apierrors::ApiError;
use sproot::models::{AlertSource, Host, HostTargeted};
use sproot::{models::Alerts, ConnType, Pool};
use std::time::Duration;

pub trait AlertsQuery {
    fn get_query(&self) -> (String, QueryType);
}

impl AlertsQuery for Alerts {
    fn get_query(&self) -> (String, QueryType) {
        match construct_query(self) {
            Ok((query, qtype)) => (query, qtype),
            Err(err) => {
                error!(
                    "Alert {} for host_uuid {:.6} cannot build the query: {}",
                    self.name, self.host_uuid, err
                );
                std::process::exit(1);
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct WholeAlert {
    pub inner: Alerts,
    pub query: String,
    pub qtype: QueryType,
}

impl WholeAlert {
    fn get_conn(&self, pool: &Pool) -> ConnType {
        match pool.get() {
            Ok(pooled) => pooled,
            Err(err) => {
                error!("Cannot get a connection from the pool: {}", err);
                std::process::exit(1);
            }
        }
    }

    /// Create the task for a particular alert and add it to the RUNNING_ALERT.
    pub fn start_monitoring(self, pool: Pool) {
        let calert = self.clone();
        let cpool = pool.clone();
        // Spawn a new task which will do the check for that particular alerts
        // Save the JoinHandle so we can abort if needed later
        let alert_task: tokio::task::JoinHandle<()> = tokio::spawn(async move {
            // Construct the interval corresponding to this alert
            let mut interval = tokio::time::interval(Duration::from_secs(self.inner.timing as u64));

            // Start the real "forever" loop
            loop {
                // Wait for the next tick of our interval
                interval.tick().await;
                trace!(
                    "Alert {} for host_uuid {:.6} running every {:?}",
                    self.inner.name,
                    self.inner.host_uuid,
                    interval.period()
                );

                // Execute the query and the analysis
                execute_analysis(&self, &mut self.get_conn(&pool));
            }
        });
        // Add the task into our AHashMap protected by RwLock (multiple readers, one write at most)
        RUNNING_ALERT
            .write()
            .unwrap()
            .insert(calert.inner.id.clone(), alert_task);

        // Add the Alert to the database if we're in Files mode
        if CONFIG.alerts_source == AlertSource::Files {
            let alert_id = calert.inner.id.clone();
            match Alerts::insert(&mut calert.get_conn(&cpool), &[calert.inner]) {
                Ok(_) => info!("Alert {} added to the database", alert_id),
                Err(e) => {
                    error!("Cannot add the alerts to the database: {}", e);
                    std::process::exit(1);
                }
            };
        }
    }
}

pub fn alerts_from_config(conn: &mut ConnType) -> Result<Vec<Alerts>, ApiError> {
    // TODO - If more than 50 hosts, get them too (paging).
    let hosts = &Host::list_hosts(conn, 50, 0)?;

    let mut alerts: Vec<Alerts> = Vec::new();
    // For each alerts config, create the Alerts corresponding
    // with the host & host_uuid & id defined.
    for aconfig in &*ALERTS_CONFIG.read().unwrap() {
        let cloned_config = aconfig.clone();
        match aconfig.host_targeted.as_ref().unwrap() {
            HostTargeted::SPECIFIC(val) => {
                let thosts: Vec<&Host> = hosts.iter().filter(|h| &h.uuid == val).collect();
                if thosts.len() != 1 {
                    return Err(ApiError::ServerError(format!(
                        "The host {} in the AlertConfig {} does not exists.",
                        &val, &aconfig.name
                    )));
                }
                let id = Alerts::generate_id_from(&thosts[0].uuid, &aconfig.name);

                info!(
                    "Created the alert {} for {:.6} with id {}",
                    &aconfig.name, thosts[0].uuid, id
                );

                alerts.push(Alerts::build_from_config(
                    cloned_config,
                    thosts[0].uuid.to_owned(),
                    thosts[0].hostname.to_owned(),
                    id,
                ));
            }
            HostTargeted::ALL => {
                for host in hosts {
                    let id = Alerts::generate_id_from(&host.uuid, &aconfig.name);

                    info!(
                        "Created the alert {} for {:.6} with id {}",
                        &aconfig.name, host.uuid, id
                    );

                    alerts.push(Alerts::build_from_config(
                        cloned_config.clone(),
                        host.uuid.to_owned(),
                        host.hostname.to_owned(),
                        id,
                    ));
                }
            }
        }
    }

    Ok(alerts)
}

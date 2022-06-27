use crate::RUNNING_ALERT;

use super::analysis::execute_analysis;
use super::query::construct_query;
use super::QueryType;

use sproot::{apierrors::ApiError, models::Alerts, ConnType, Pool};
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
        let cid = self.inner.id.clone();
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
        RUNNING_ALERT.write().unwrap().insert(cid, alert_task);
    }
}

pub fn alerts_from_database(pool: &Pool) -> Result<Vec<Alerts>, ApiError> {
    // Get the alerts from the database
    Alerts::get_list(&mut pool.get()?)
}

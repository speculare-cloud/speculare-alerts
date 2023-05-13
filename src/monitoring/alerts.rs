use std::time::Duration;

use bastion::context::BastionContext;
use diesel::{sql_types::Text, *};
use sproot::models::QueryType;
use sproot::{apierrors::ApiError, models::Alerts, ConnType, Pool};
use tokio::time::interval;

use super::analysis::execute_analysis;
use super::qtype::pct;
use crate::utils::{AbsDTORaw, PctDTORaw};
use crate::{RUNNING_CHILDREN, SUPERVISOR};

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

    pub fn stop_monitoring(&self) {
        let mut run = RUNNING_CHILDREN.write().unwrap();
        let child = run.remove(&self.inner.id);
        if let Some(child) = child {
            _ = child.kill();
        }
    }

    /// Create the task for a particular alert and add it to the RUNNING_ALERT.
    pub fn start_monitoring(self, pool: Pool) {
        // Cloning the id of the inner alert to the RUNNING_CHILDREN
        let cid = self.inner.id;

        // Create/Add a new children into the Bastion "supervisor"
        // This children will be restarted if it fails and can be killed
        // thanks to the ChildrenRef added inside RUNNING_CHILDREN.
        let children_ref = SUPERVISOR
            .children(|child| {
                child.with_exec(move |_ctx: BastionContext| {
                    let alert = self.clone();
                    let mut conn = self.get_conn(&pool);
                    async move {
                        // Construct the interval corresponding to this alert
                        let mut interval = interval(Duration::from_secs(alert.inner.timing as u64));

                        trace!(
                            "Alert {} for host_uuid {:.6} running every {:?}",
                            alert.inner.name,
                            alert.inner.host_uuid,
                            interval.period()
                        );

                        // Start the real "forever" loop
                        loop {
                            // Wait for the next tick of our interval
                            interval.tick().await;

                            // Execute the query and the analysis
                            execute_analysis(&alert, &mut conn);
                        }
                    }
                })
            })
            .expect("Cannot create the Children for Bastion");

        // Add the children_ref into the global AHashMap RUNNING_CHILDREN
        RUNNING_CHILDREN.write().unwrap().insert(cid, children_ref);
    }

    /// This function execute the query based on the QueryType,
    /// because all type does not wait for the same result.
    pub fn execute_query(&self, conn: &mut ConnType) -> Result<String, ApiError> {
        // Each qtype type has their own return structure and conversion method (from struct to String).
        match self.qtype {
            QueryType::Pct => {
                let results = sql_query(&self.query)
                    .bind::<Text, _>(&self.inner.host_uuid)
                    .load::<PctDTORaw>(conn)?;
                Ok(pct::compute_pct(&results).to_string())
            }
            QueryType::Abs => {
                let results = sql_query(&self.query)
                    .bind::<Text, _>(&self.inner.host_uuid)
                    .load::<AbsDTORaw>(conn)?;
                trace!("result abs is {:?}", &results);
                if results.is_empty() {
                    Err(ApiError::NotFoundError(Some(String::from(
                        "the result of the query (abs) is empty",
                    ))))
                } else {
                    Ok(results[0].value.to_string())
                }
            }
        }
    }
}

pub fn alerts_from_database(pool: &Pool) -> Result<Vec<Alerts>, ApiError> {
    // Get the alerts from the database
    Alerts::get_all(&mut pool.get()?)
}

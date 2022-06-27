use crate::RUNNING_CHILDREN;

use super::analysis::execute_analysis;
use super::query::construct_query;
use super::QueryType;

use bastion::{
    context::BastionContext,
    supervisor::{ActorRestartStrategy, RestartStrategy, SupervisorRef},
    Bastion,
};
use sproot::{apierrors::ApiError, models::Alerts, ConnType, Pool};
use std::time::Duration;

lazy_static::lazy_static! {
    static ref SUPERVISOR: SupervisorRef = match Bastion::supervisor(|sp| {
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
    };
}

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

    pub fn stop_monitoring(&self) {
        let mut run = RUNNING_CHILDREN.write().unwrap();
        let child = run.remove(&self.inner.id);
        if let Some(child) = child {
            _ = child.kill();
        }
    }

    /// Create the task for a particular alert and add it to the RUNNING_ALERT.
    /// TODO - Get rid of most of those clone (beurk)
    pub fn start_monitoring(&self, pool: Pool) {
        let cid = self.inner.id.clone();
        let alert = self.clone();
        let children_ref = SUPERVISOR
            .children(|child| {
                child.with_exec(move |_ctx: BastionContext| {
                    let alert = alert.clone();
                    let pool = pool.clone();
                    async move {
                        // Construct the interval corresponding to this alert
                        let mut interval =
                            tokio::time::interval(Duration::from_secs(alert.inner.timing as u64));

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
                            execute_analysis(&alert, &mut alert.get_conn(&pool));
                        }
                    }
                })
            })
            .expect("Cannot create the Children for Bastion");

        // let cid = self.inner.id.clone();
        // let alert = self.clone();
        // let children_ref = Bastion::spawn(move |_ctx: BastionContext| {
        //     let alert = alert.clone();
        //     let pool = pool.clone();
        //     async move {
        //         debug!("{:?}", alert);

        //         // Construct the interval corresponding to this alert
        //         let mut interval =
        //             tokio::time::interval(Duration::from_secs(alert.inner.timing as u64));

        //         // Start the real "forever" loop
        //         loop {
        //             // Wait for the next tick of our interval
        //             interval.tick().await;
        //             trace!(
        //                 "Alert {} for host_uuid {:.6} running every {:?}",
        //                 alert.inner.name,
        //                 alert.inner.host_uuid,
        //                 interval.period()
        //             );

        //             // Execute the query and the analysis
        //             execute_analysis(&alert, &mut alert.get_conn(&pool));
        //         }
        //     }
        // })
        // .expect("Cannot create the Children for Bastion");

        RUNNING_CHILDREN.write().unwrap().insert(cid, children_ref);
    }
}

pub fn alerts_from_database(pool: &Pool) -> Result<Vec<Alerts>, ApiError> {
    // Get the alerts from the database
    Alerts::get_list(&mut pool.get()?)
}

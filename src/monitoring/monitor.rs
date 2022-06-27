use super::alerts::{alerts_from_database, AlertsQuery, WholeAlert};

use sproot::Pool;

pub struct Monitor {
    alerts: Vec<WholeAlert>,
    pool: Pool,
}

impl Monitor {
    pub fn default(pool: &Pool) -> Self {
        // Get the Alerts and convert them into WholeAlert
        let alerts = match alerts_from_database(pool) {
            Ok(alerts) => alerts
                .into_iter()
                .map(|alert| {
                    let (query, qtype) = alert.get_query();

                    WholeAlert {
                        inner: alert,
                        query,
                        qtype,
                    }
                })
                .collect(),
            Err(err) => {
                error!("monitoring: fatal error: {}", err);
                std::process::exit(1);
            }
        };

        Self {
            alerts,
            pool: pool.to_owned(),
        }
    }

    pub fn oneshot(self) {
        for alert in self.alerts {
            alert.start_monitoring(self.pool.clone());
        }
    }
}

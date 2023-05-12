use sproot::Pool;

use super::{
    alerts::{alerts_from_database, WholeAlert},
    query::AlertsQuery,
};

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
                .map(|alert| match alert.construct_query() {
                    Ok((query, qtype)) => Ok(WholeAlert {
                        inner: alert,
                        query,
                        qtype,
                    }),
                    Err(err) => {
                        error!(
                            "cannot construct the query related to alert {}: {}",
                            alert.id, err
                        );
                        Err(err)
                    }
                })
                .take_while(Result::is_ok)
                .map(Result::unwrap)
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
            if !alert.inner.active {
                continue;
            }
            alert.start_monitoring(self.pool.clone());
        }
    }
}

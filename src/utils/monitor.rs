use crate::{utils::alerts::alerts_from_config, ALERTS_CONFIG, CONFIG};

use sproot::{
    apierrors::ApiError,
    models::AlertSource,
    models::{Alerts, AlertsConfig},
    Pool,
};

use super::alerts::{AlertsQuery, WholeAlert};

pub struct Monitor {
    alerts: Vec<WholeAlert>,
    pool: Pool,
}

impl Monitor {
    pub fn default(pool: &Pool) -> Self {
        // Get the Alerts and convert them into WholeAlert
        let alerts = match if CONFIG.alerts_source == AlertSource::Files {
            alerts_from_files(pool)
        } else {
            alerts_from_database(pool)
        } {
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

fn alerts_from_files(pool: &Pool) -> Result<Vec<Alerts>, ApiError> {
    // Get the AlertsConfig from the ALERTS_PATH folder
    let alerts_config: Vec<AlertsConfig> = AlertsConfig::from_configs_path(&CONFIG.alerts_path)?;
    // New scope: Drop the lock as soon as it's not needed anymore
    {
        // Move the local alerts_config Vec to the global ALERTS_CONFIG
        let mut x = ALERTS_CONFIG.write().unwrap();
        let _ = std::mem::replace(&mut *x, alerts_config);
    }
    trace!("alerts_from_files: read from the alerts_path");

    // Convert the AlertsConfig to alerts
    alerts_from_config(&mut pool.get()?)
}

fn alerts_from_database(pool: &Pool) -> Result<Vec<Alerts>, ApiError> {
    // Get the alerts from the database
    Alerts::get_list(&mut pool.get()?)
}

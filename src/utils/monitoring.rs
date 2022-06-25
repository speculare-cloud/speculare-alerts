use crate::{ALERTS_CONFIG, CONFIG};

use super::alerts::start_alert_task;

use sproot::{
    errors::AppError,
    models::AlertSource,
    models::{Alerts, AlertsConfig, Host, HostTargeted},
    ConnType, Pool,
};

pub fn alerts_from_config(conn: &mut ConnType) -> Result<Vec<Alerts>, AppError> {
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
                    return Err(AppError {
                        message: format!(
                            "The host {} in the AlertConfig {} does not exists.",
                            &val, &aconfig.name
                        ),
                        error_type: sproot::errors::AppErrorType::NotFound,
                    });
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

fn alerts_from_files(pool: &Pool) -> Result<Vec<Alerts>, AppError> {
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

fn alerts_from_database(pool: &Pool) -> Result<Vec<Alerts>, AppError> {
    // Get the alerts from the database
    Alerts::get_list(&mut pool.get()?)
}

/// Start the monitoring tasks for each alarms
pub fn launch_monitoring(pool: &Pool) -> Result<(), AppError> {
    let alerts = match if CONFIG.alerts_source == AlertSource::Files {
        alerts_from_files(pool)
    } else {
        alerts_from_database(pool)
    } {
        Ok(alerts) => alerts,
        Err(err) => {
            error!("monitoring: fatal error: {}", err);
            std::process::exit(1);
        }
    };

    // Start the alerts monitoring for real
    for alert in alerts {
        start_alert_task(alert, pool.clone())
    }

    Ok(())
}

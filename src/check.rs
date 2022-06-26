use crate::{
    utils::{
        alerts::{alerts_from_config, AlertsQuery, WholeAlert},
        analysis::execute_analysis,
        mail::test_smtp_transport,
    },
    ALERTS_CONFIG, CONFIG,
};

use sproot::{
    models::{AlertSource, AlertsConfig},
    Pool,
};

/// Will check the AlertsConfig & SMTP syntax for potential errors
pub fn dry_run(pool: Pool) {
    // Check if the SMTP server host is "ok"
    test_smtp_transport();

    // If the AlertSource is database, we don't need to check the Alerts
    // as they are created using the API and already checked at creation
    if CONFIG.alerts_source == AlertSource::Database {
        println!("\nEverything went well, no errors found !");
        return;
    }

    // Need to get the Alerts
    let alerts_config = match AlertsConfig::from_configs_path(&CONFIG.alerts_path) {
        Ok(alerts_config) => alerts_config,
        Err(_) => {
            println!("\nFailed to get AlertsConfig, check tbe logs for more info.\n> If you can't see the logs, try settings RUST_LOG to trace in the config.toml");
            std::process::exit(1)
        }
    };

    // New scope: Drop the lock as soon as it's not needed anymore
    {
        // Move the local alerts_config Vec to the global ALERTS_CONFIG
        let mut x = ALERTS_CONFIG.write().unwrap();
        let _ = std::mem::replace(&mut *x, alerts_config);
    }

    let mut conn = match pool.get() {
        Ok(conn) => conn,
        Err(e) => {
            error!("Cannot get a conn for the alerts_from_config: {}", e);
            std::process::exit(1);
        }
    };

    // Convert the AlertsConfig to alert
    let alerts: Vec<WholeAlert> = match alerts_from_config(&mut conn) {
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
        Err(e) => {
            error!("Failed to launch monitoring: {}", e);
            std::process::exit(1);
        }
    };

    // Dry run for the alerts and exit in case of error
    for alert in alerts {
        execute_analysis(&alert, &mut conn);
    }

    println!("\nEverything went well, no errors found !");
}

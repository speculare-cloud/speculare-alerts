use crate::notifications::mail;

use super::{alerts::WholeAlert, IncidentStatus, Severity};

use chrono::prelude::Utc;
use evalexpr::*;
use sproot::{
    apierrors::ApiError,
    models::{Alerts, Incidents, IncidentsDTO, IncidentsDTOUpdate},
    ConnType,
};

/// Determine if we are in a Warn or Crit level of incidents
fn check_threshold(walert: &WholeAlert, result: &str) -> (bool, bool) {
    let should_warn = eval_boolean(&walert.inner.warn.replace("$this", result)).unwrap_or_else(|e| {
        error!(
            "[{}] alert {} for host_uuid {:.6} failed to parse the String to an expression (warn: {}): {}",
            walert.inner.id, walert.inner.name, walert.inner.host_uuid, walert.inner.warn, e
        );
        panic!();
    });
    let should_crit = eval_boolean(&walert.inner.crit.replace("$this", result)).unwrap_or_else(|e| {
        error!(
            "[{}] alert {} for host_uuid {:.6} failed to parse the String to an expression (crit: {}): {}",
            walert.inner.id, walert.inner.name, walert.inner.host_uuid, walert.inner.crit, e
        );
        panic!();
    });

    (should_warn, should_crit)
}

/// This function is the core of the monitoring, this is where we:
/// - Execute the query and get the result
/// - Evaluate if we need to trigger an incidents or not
pub fn execute_analysis(walert: &WholeAlert, conn: &mut ConnType) {
    info!(
        "[{}] Executing {} analysis for {:.6}",
        walert.inner.id, walert.inner.name, walert.inner.host_uuid
    );

    // Execute the query passed as arguement (this query was build previously)
    let result = match walert.execute_query(conn) {
        Ok(result) => result,
        Err(err) => match err {
            ApiError::NotFoundError(_) => return,
            _ => {
                error!(
                    "[{}] Analysis: alert {} for host_uuid {:.6} execute_query failed: {}",
                    walert.inner.id, walert.inner.name, walert.inner.host_uuid, err
                );
                panic!();
            }
        },
    };

    // This call to check_threshold panics if the evals fails
    let (should_warn, should_crit) = check_threshold(walert, &result);

    // Check if an active incident already exist for this alarm (err == not found).
    let prev_incident: Option<Incidents> = match Incidents::find_active(conn, &walert.inner.id) {
        Ok(res) => Some(res),
        Err(_) => None,
    };

    // Assert that we do not create an incident for nothing
    if !(should_warn || should_crit) {
        // Check if an incident was active
        if let Some(prev_incident) = prev_incident {
            info!(
                ">[{}] We need to resolve the previous incident however",
                walert.inner.id
            );
            let incident_id = prev_incident.id;
            let incident_dto = IncidentsDTOUpdate {
                status: Some(IncidentStatus::Resolved as i32),
                updated_at: Some(Utc::now().naive_local()),
                resolved_at: Some(Utc::now().naive_local()),
                ..Default::default()
            };
            // TODO - Handle error
            let incident = incident_dto
                .gupdate(conn, incident_id)
                .expect("Failed to update (resolve) the incidents");
            // TODO - Handle error
            mail::send_information_mail(&incident, false);
        }
        return;
    }

    // Determine the incident severity based on the (should_warn, should_crit)
    let severity = match (should_warn, should_crit) {
        (true, false) => Severity::Warning,
        (false, true) => Severity::Critical,
        (true, true) => Severity::Critical,
        (false, false) => {
            panic!("should_warn && should_crit are both false, this should never happens.")
        }
    };

    // If prev_incident exists:
    // - We need to update the severity of the incidents
    // - The result of the query changed
    // - Update the updated_at field
    // If prev_incident does not exists:
    // - Create a new incident
    match prev_incident {
        Some(prev_incident) => {
            trace!(
                ">[{}] Update the previous incident using the new values",
                walert.inner.id
            );
            let incident_id = prev_incident.id;
            // We won't downgrade an incident because it's been a "critical" incident in the past.
            let curr_severity = severity as i32;
            // Check if we should update the severity and thus sending an escalation alert
            let mut should_alert = false;
            let mut incident_severity = None;
            if prev_incident.severity < curr_severity {
                should_alert = true;
                incident_severity = Some(curr_severity);
            };
            // Update the previous incident
            let incident_dto = IncidentsDTOUpdate {
                result: Some(result),
                updated_at: Some(Utc::now().naive_local()),
                severity: incident_severity,
                ..Default::default()
            };
            // TODO - Handle error
            let incident = incident_dto
                .gupdate(conn, incident_id)
                .expect("Failed to update the incidents");

            // We should_alert if the prev severity is lower than the current one
            if should_alert {
                // TODO - Handle error
                mail::send_information_mail(&incident, true);
            }
        }
        None => {
            info!(
                ">[{}] Create a new incident based on the current values",
                walert.inner.id
            );
            // Clone the alert to allow us to own it in the IncidentsDTO
            let calert: Alerts = walert.inner.clone();
            let incident = IncidentsDTO {
                result,
                started_at: Utc::now().naive_local(),
                updated_at: Utc::now().naive_local(),
                resolved_at: None,
                host_uuid: calert.host_uuid,
                hostname: calert.hostname,
                status: IncidentStatus::Active as i32,
                severity: severity as i32,
                alerts_id: calert.id,
                alerts_name: calert.name,
                alerts_table: calert.table,
                alerts_lookup: calert.lookup,
                alerts_warn: calert.warn,
                alerts_crit: calert.crit,
                alerts_info: calert.info,
                alerts_where_clause: calert.where_clause,
            };
            // TODO - Handle error
            let incident = incident
                .ginsert(conn)
                .expect("Failed to insert a new incident");

            // TODO - Handle error
            mail::send_information_mail(&incident, false);
        }
    }
}

use crate::{
    monitoring::{IncidentStatus, Severity},
    CONFIG,
};

use lettre::transport::smtp::authentication::Credentials;
use lettre::transport::smtp::client::{Tls, TlsParameters};
use lettre::transport::smtp::PoolConfig;
use lettre::SmtpTransport;
use lettre::{
    message::{header, MultiPart, SinglePart},
    Message, Transport,
};
use once_cell::sync::Lazy;
use sailfish::TemplateOnce;
use sproot::models::Incidents;

const DATE_SMALL_FORMAT: &str = "%d %b %Y at %H:%M";
const DATE_FORMAT: &str = "%Y-%m-%d %H:%M:%S";

// Lazy static for SmtpTransport used to send mails
// Build it using rustls and a pool of 16 items.
static MAILER: Lazy<SmtpTransport> = Lazy::new(|| match get_smtp_transport() {
    Ok(smtp) => smtp,
    Err(e) => {
        error!("MAILER: cannot get the smtp_transport: {}", e);
        std::process::exit(1);
    }
});

pub fn test_smtp_transport() {
    // Check if the SMTP server host is "ok"
    match MAILER.test_connection() {
        Ok(result) => {
            info!("MAILER: No fatal error, connect is: {}", result);
        }
        Err(e) => {
            error!("MAILER: test of the smtp_transport failed: {}", e);
            std::process::exit(1);
        }
    }
}

/// Structure representing the incident (created) template html sent by mail
#[derive(TemplateOnce)]
#[template(path = "incident.stpl")]
struct IncidentTemplate<'a> {
    alert_name: &'a str,
    hostname: &'a str,
    severity: &'a str,
    started_at: &'a str,
    lookup: &'a str,
    result: &'a str,
    warn: &'a str,
    crit: &'a str,
}

/// Structure representing the incident (escalated) template html sent by mail
#[derive(TemplateOnce)]
#[template(path = "escalate.stpl")]
struct EscalateTemplate<'a> {
    hostname: &'a str,
    severity: &'a str,
    updated_at: &'a str,
    lookup: &'a str,
    result: &'a str,
    warn: &'a str,
    crit: &'a str,
}

/// Structure representing the incident (resolved) template html sent by mail
#[derive(TemplateOnce)]
#[template(path = "resolved.stpl")]
struct ResolvedTemplate<'a> {
    alert_name: &'a str,
    hostname: &'a str,
    resolved_at: &'a str,
    lookup: &'a str,
    result: &'a str,
    warn: &'a str,
    crit: &'a str,
}

/// Send an email alerting on the status (new/escalated/resolved) of an incident.
pub fn send_information_mail(incident: &Incidents, escalate: bool) {
    // SAFETY: render_once() can never fails except if called from the template itself.
    let mail_content = match (escalate, IncidentStatus::from(incident.status)) {
        (true, _) => EscalateTemplate {
            hostname: &incident.hostname,
            severity: &Severity::from(incident.severity).to_string(),
            updated_at: &incident.updated_at.format(DATE_FORMAT).to_string(),
            lookup: &incident.alerts_lookup,
            result: &incident.result,
            warn: &incident.alerts_warn,
            crit: &incident.alerts_crit,
        }
        .render_once()
        .unwrap(),
        (false, IncidentStatus::Active) => IncidentTemplate {
            alert_name: &incident.alerts_name,
            hostname: &incident.hostname,
            severity: &Severity::from(incident.severity).to_string(),
            started_at: &incident.started_at.format(DATE_FORMAT).to_string(),
            lookup: &incident.alerts_lookup,
            result: &incident.result,
            warn: &incident.alerts_warn,
            crit: &incident.alerts_crit,
        }
        .render_once()
        .unwrap(),
        (false, IncidentStatus::Resolved) => ResolvedTemplate {
            alert_name: &incident.alerts_name,
            hostname: &incident.hostname,
            resolved_at: &incident.updated_at.format(DATE_FORMAT).to_string(),
            lookup: &incident.alerts_lookup,
            result: &incident.result,
            warn: &incident.alerts_warn,
            crit: &incident.alerts_crit,
        }
        .render_once()
        .unwrap(),
    };

    send_mail(incident, mail_content);
}

fn send_mail(incident: &Incidents, template: String) {
    // Build the email with all params
    let email = match Message::builder()
        // Sender is the email of the sender, which is used by the SMTP
        // if the sender is not equals to the smtp server account, the mail will ends in the spam.
        .from(CONFIG.smtp_email_sender.clone())
        // Receiver is the person who should get the email
        .to(CONFIG.smtp_email_receiver.clone())
        // Subject will looks like: "Hostname [alert_name] - 23 Jul 2021 at 17:51"
        .subject(format!("{} [{}] - {}", incident.hostname, incident.alerts_name, incident.started_at.format(DATE_SMALL_FORMAT)))
        .multipart(
                // Use multipart to have a fallback
            MultiPart::alternative()
                    // This singlepart is the fallback for the html code
                    .singlepart(
                        SinglePart::builder()
                        .header(header::ContentType::TEXT_PLAIN)
                        .body(String::from("There's a new error being reported by Speculare.\nAllow this mail to be displayed as HTML or go to your dashboard."))
                    )
                    // This singlepart is the html design with all fields replaced
                    .singlepart(
                        SinglePart::builder()
                        .header(header::ContentType::TEXT_HTML)
                        .body(template)
                    )
        ) {
			Ok(mail) => mail,
			Err(err) => {
				error!("Could not construct the email: {}", err);
				return;
			},
		};

    // Send the email
    match MAILER.send(&email) {
        Ok(_) => info!(
            "Email for alert {} with host {:.6} sent successfully!",
            incident.alerts_name, incident.host_uuid
        ),
        Err(err) => error!("Could not send email: {}", err),
    }
}

fn get_smtp_transport() -> Result<SmtpTransport, lettre::transport::smtp::Error> {
    let creds = Credentials::new(CONFIG.smtp_user.to_owned(), CONFIG.smtp_password.to_owned());

    let transport = if CONFIG.smtp_tls {
        SmtpTransport::builder_dangerous(&CONFIG.smtp_host).tls(Tls::Required(TlsParameters::new(
            (&CONFIG.smtp_host).to_owned(),
        )?))
    } else {
        SmtpTransport::builder_dangerous(&CONFIG.smtp_host)
    };

    // Open a remote connection to gmail
    Ok(transport
        .port(CONFIG.smtp_port)
        .credentials(creds)
        .pool_config(PoolConfig::new().max_size(16))
        .build())
}

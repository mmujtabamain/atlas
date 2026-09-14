//! Failure reporting to the team (CLAUDE.md "Monitoring & failure alerting").
//!
//! Production failures — panics and engine errors surfaced by the UI — are
//! posted to DevBench's notify endpoint, which relays them into the project
//! chat and the mobile app:
//!
//! ```text
//! POST $DEVBENCH_NOTIFY_URL/api/notify/
//! Authorization: Bearer $DEVBENCH_NOTIFY_TOKEN
//! {"text": "...", "level": "info|warning|error", "source": "atlas-app"}
//! ```
//!
//! Both values come from the environment only; the token is a server-side
//! secret and is never written to a log or a screen. Without them the module
//! degrades to local logging and says so once.

use std::sync::OnceLock;
use std::time::Duration;

/// Severity of an alert, as the endpoint expects it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Level {
    Info,
    Warning,
    Error,
}

impl Level {
    pub fn as_str(self) -> &'static str {
        match self {
            Level::Info => "info",
            Level::Warning => "warning",
            Level::Error => "error",
        }
    }
}

/// Where alerts go.
#[derive(Clone, Debug)]
pub struct AlertConfig {
    base_url: String,
    token: String,
    source: String,
}

impl AlertConfig {
    /// Reads `DEVBENCH_NOTIFY_URL` and `DEVBENCH_NOTIFY_TOKEN`; `None` when
    /// either is missing or empty.
    pub fn from_env() -> Option<AlertConfig> {
        let base_url = std::env::var("DEVBENCH_NOTIFY_URL").ok().filter(|v| !v.trim().is_empty())?;
        let token = std::env::var("DEVBENCH_NOTIFY_TOKEN").ok().filter(|v| !v.trim().is_empty())?;
        Some(AlertConfig::new(base_url, token))
    }

    pub fn new(base_url: impl Into<String>, token: impl Into<String>) -> AlertConfig {
        AlertConfig {
            base_url: base_url.into().trim_end_matches('/').to_string(),
            token: token.into(),
            source: "atlas-app".into(),
        }
    }

    /// The full endpoint URL.
    pub fn endpoint(&self) -> String {
        format!("{}/api/notify/", self.base_url)
    }

    /// The JSON body for one alert.
    pub fn body(&self, level: Level, text: &str) -> serde_json::Value {
        serde_json::json!({ "text": text, "level": level.as_str(), "source": self.source })
    }

    /// Posts one alert synchronously. Returns the HTTP status, or the transport
    /// error as text. Never panics — this runs inside the panic hook.
    pub fn send(&self, level: Level, text: &str) -> Result<u16, String> {
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(5)))
            .http_status_as_error(false)
            .build()
            .into();
        let response = agent
            .post(self.endpoint())
            .header("Authorization", format!("Bearer {}", self.token))
            .send_json(self.body(level, text))
            .map_err(|err| err.to_string())?;
        Ok(response.status().as_u16())
    }
}

static CONFIG: OnceLock<Option<AlertConfig>> = OnceLock::new();

/// Reads the configuration once. Call early in `main`.
pub fn init() {
    let config = CONFIG.get_or_init(AlertConfig::from_env);
    match config {
        Some(config) => log::info!("production alerts → {}", config.endpoint()),
        None => log::warn!("production alerts are log-only: set DEVBENCH_NOTIFY_URL and DEVBENCH_NOTIFY_TOKEN in the deployment environment"),
    }
}

/// Whether alerts reach DevBench, which Settings states under Diagnostics.
pub fn is_configured() -> bool {
    CONFIG.get().map(|c| c.is_some()).unwrap_or(false)
}

/// Logs the failure and, when configured, posts it to the team from a
/// background thread so the UI never waits on the network.
pub fn report(level: Level, text: impl Into<String>) {
    let text = text.into();
    match level {
        Level::Error => log::error!("{text}"),
        Level::Warning => log::warn!("{text}"),
        Level::Info => log::info!("{text}"),
    }
    if let Some(Some(config)) = CONFIG.get() {
        let config = config.clone();
        std::thread::Builder::new()
            .name("atlas-alert".into())
            .spawn(move || match config.send(level, &text) {
                Ok(status) if (200..300).contains(&status) => log::debug!("alert delivered ({status})"),
                Ok(status) => log::warn!("alert endpoint answered {status}"),
                Err(err) => log::warn!("alert not delivered: {err}"),
            })
            .ok();
    }
}

/// Installs a panic hook that reports the panic before the default hook runs.
/// The report is synchronous here: the process may be about to die.
pub fn install_panic_hook() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let location = info
            .location()
            .map(|l| format!("{}:{}", l.file(), l.line()))
            .unwrap_or_else(|| "unknown location".into());
        let message = info
            .payload()
            .downcast_ref::<&str>()
            .map(|s| s.to_string())
            .or_else(|| info.payload().downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "panic".into());
        let text = format!("atlas-app panicked at {location}: {message}");
        log::error!("{text}");
        if let Some(Some(config)) = CONFIG.get()
            && let Err(err) = config.send(Level::Error, &text)
        {
            log::warn!("panic alert not delivered: {err}");
        }
        previous(info);
    }));
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader, Read, Write};
    use std::net::TcpListener;

    /// A one-request HTTP stub that records what it received.
    fn stub_server(status_line: &'static str) -> (String, std::thread::JoinHandle<(String, String)>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let handle = std::thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            let mut reader = BufReader::new(stream);
            let mut headers = String::new();
            let mut content_length = 0usize;
            loop {
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                if line == "\r\n" || line.is_empty() {
                    break;
                }
                if let Some(value) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                    content_length = value.trim().parse().unwrap();
                }
                headers.push_str(&line);
            }
            let mut body = vec![0u8; content_length];
            reader.read_exact(&mut body).unwrap();
            let mut stream = reader.into_inner();
            write!(stream, "HTTP/1.1 {status_line}\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok").unwrap();
            stream.flush().unwrap();
            (headers, String::from_utf8(body).unwrap())
        });
        (format!("http://127.0.0.1:{port}/"), handle)
    }

    #[test]
    fn posts_the_expected_request() {
        let (url, server) = stub_server("200 OK");
        let config = AlertConfig::new(url, "secret-token");
        assert!(config.endpoint().ends_with("/api/notify/"));
        assert!(!config.endpoint().contains("//api"));
        let status = config.send(Level::Error, "forecast failed: unknown account account-9").unwrap();
        assert_eq!(status, 200);
        let (headers, body) = server.join().unwrap();
        assert!(headers.to_ascii_lowercase().contains("authorization: bearer secret-token"), "{headers}");
        assert!(headers.to_ascii_lowercase().contains("content-type: application/json"), "{headers}");
        let json: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(json["level"], "error");
        assert_eq!(json["source"], "atlas-app");
        assert_eq!(json["text"], "forecast failed: unknown account account-9");
    }

    #[test]
    fn reports_non_success_status_without_panicking() {
        let (url, server) = stub_server("503 Service Unavailable");
        let config = AlertConfig::new(url, "t");
        assert_eq!(config.send(Level::Warning, "x").unwrap(), 503);
        server.join().unwrap();
    }

    #[test]
    fn unreachable_endpoint_is_an_error_value() {
        let config = AlertConfig::new("http://127.0.0.1:9/", "t");
        assert!(config.send(Level::Info, "x").is_err());
    }
}

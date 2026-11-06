use std::env;

/// Runtime configuration, sourced from environment variables (see `.env.example`).
#[derive(Debug, Clone)]
pub struct AppConfig {
    pub addr: String,
    pub port: u16,
}

impl AppConfig {
    pub fn from_env() -> Self {
        let addr = env::var("ARSY_ADDR").unwrap_or_else(|_| "0.0.0.0".to_string());
        let port = env::var("ARSY_PORT")
            .ok()
            .and_then(|raw| raw.parse().ok())
            .unwrap_or(8080);
        Self { addr, port }
    }
}

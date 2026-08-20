use std::{env, net::IpAddr, str::FromStr};

use thiserror::Error;

pub const DEFAULT_HOST: &str = "127.0.0.1";
pub const DEFAULT_PORT: u16 = 8787;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    pub host: IpAddr,
    pub port: u16,
}

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("CODEX_QUOTA_HOST is not a valid IP address: {0}")]
    InvalidHost(String),
    #[error("CODEX_QUOTA_PORT is not a valid port: {0}")]
    InvalidPort(String),
}

impl Config {
    pub fn from_env() -> Result<Self, ConfigError> {
        let host_text = env::var("CODEX_QUOTA_HOST").unwrap_or_else(|_| DEFAULT_HOST.to_owned());
        let host = IpAddr::from_str(&host_text)
            .map_err(|_| ConfigError::InvalidHost(host_text.clone()))?;

        let port_text = env::var("CODEX_QUOTA_PORT").unwrap_or_else(|_| DEFAULT_PORT.to_string());
        let port = port_text
            .parse::<u16>()
            .map_err(|_| ConfigError::InvalidPort(port_text.clone()))?;
        if port == 0 {
            return Err(ConfigError::InvalidPort(port_text));
        }

        Ok(Self { host, port })
    }
}

#[cfg(test)]
mod tests {
    use super::{DEFAULT_HOST, DEFAULT_PORT};

    #[test]
    fn defaults_match_python_runtime() {
        assert_eq!(DEFAULT_HOST, "127.0.0.1");
        assert_eq!(DEFAULT_PORT, 8787);
    }
}

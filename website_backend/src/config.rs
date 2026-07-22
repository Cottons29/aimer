use std::net::{IpAddr, SocketAddr};
use std::path::{Path, PathBuf};

use axum::http::{HeaderValue, header::InvalidHeaderValue};
use serde::Deserialize;
use thiserror::Error;

/// The runtime configuration loaded from an Aimer manifest.
///
/// Use [`Config::load`] during process startup to read and validate the
/// `[server]` table from `aimer.toml`. Invalid values are rejected before the
/// network listener is created.
#[derive(Debug)]
pub struct Config {
    server: ServerConfig,
}

/// Network and cross-origin settings for the website backend.
#[derive(Debug)]
pub struct ServerConfig {
    address: SocketAddr,
    cors_origins: Vec<HeaderValue>,
}

#[derive(Debug, Deserialize)]
struct RawConfig {
    server: RawServerConfig,
}

#[derive(Debug, Deserialize)]
struct RawServerConfig {
    ip: IpAddr,
    port: u16,
    cors: Vec<String>,
}

/// An error encountered while reading or validating `aimer.toml`.
#[derive(Debug, Error)]
pub enum ConfigError {
    /// The configuration file could not be read.
    #[error("reading configuration file {}: {source}", path.display())]
    Read {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    /// The configuration file is not valid TOML or has invalid field values.
    #[error("parsing configuration file {}: {source}", path.display())]
    Parse {
        path: PathBuf,
        #[source]
        source: toml::de::Error,
    },
    /// A configured CORS origin cannot be represented as an HTTP header value.
    #[error("invalid CORS origin {origin:?}: {source}")]
    InvalidCorsOrigin {
        origin: String,
        #[source]
        source: InvalidHeaderValue,
    },
}

impl Config {
    /// Reads and validates a runtime configuration file.
    ///
    /// The file must contain a `[server]` table with an IP address, a `u16`
    /// port, and a list of CORS origins. This method does not supply defaults:
    /// missing or malformed settings return an error so deployment mistakes are
    /// visible at startup.
    pub fn load(path: impl AsRef<Path>) -> Result<Self, ConfigError> {
        let path = path.as_ref();
        let contents = std::fs::read_to_string(path).map_err(|source| ConfigError::Read {
            path: path.to_owned(),
            source,
        })?;
        let raw: RawConfig = toml::from_str(&contents).map_err(|source| ConfigError::Parse {
            path: path.to_owned(),
            source,
        })?;
        let cors_origins = raw
            .server
            .cors
            .into_iter()
            .map(|origin| {
                HeaderValue::from_str(&origin)
                    .map_err(|source| ConfigError::InvalidCorsOrigin { origin, source })
            })
            .collect::<Result<Vec<_>, _>>()?;

        Ok(Self {
            server: ServerConfig {
                address: SocketAddr::new(raw.server.ip, raw.server.port),
                cors_origins,
            },
        })
    }

    /// Returns the validated server settings.
    pub fn server(&self) -> &ServerConfig {
        &self.server
    }
}

impl ServerConfig {
    /// Returns the socket address formed from the configured IP and port.
    pub fn address(&self) -> SocketAddr {
        self.address
    }

    /// Returns the HTTP header values accepted by the CORS policy.
    pub fn cors_origins(&self) -> &[HeaderValue] {
        &self.cors_origins
    }
}

use std::collections::HashMap;
use std::fmt;
use std::net::{IpAddr, SocketAddr};
use std::path::{Path, PathBuf};

use axum::http::HeaderValue;
use axum::http::header::InvalidHeaderValue;
use thiserror::Error;

/// The environment variable holding the IP address the server binds to.
const SERVER_IP: &str = "SERVER_IP";
/// The environment variable holding the TCP port the server binds to.
const SERVER_PORT: &str = "SERVER_PORT";
/// The environment variable holding the comma separated CORS origins.
const SERVER_CORS: &str = "SERVER_CORS";

/// The runtime configuration of the website backend.
///
/// Use [`Config::resolve`] during process startup to read and validate the
/// server settings from a standard `.env` file, falling back to the process
/// environment when the file is absent. Invalid values are rejected before the
/// network listener is created.
///
/// The two sources exist for the two ways the backend is deployed: a developer
/// checkout keeps its settings in `.env`, while container platforms such as
/// Cloudflare Containers inject them as ordinary environment variables.
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

/// The place a [`Config`] was read from.
///
/// It is carried by [`ConfigError`] so a failure points at the file or at the
/// process environment the offending value came from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ConfigSource {
    /// A dotenv document at the given path.
    File(PathBuf),
    /// The environment variables of the running process.
    Environment,
}

impl fmt::Display for ConfigSource {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::File(path) => write!(formatter, "configuration file {}", path.display()),
            Self::Environment => formatter.write_str("the process environment"),
        }
    }
}

/// An error encountered while reading or validating the configuration.
#[derive(Debug, Error)]
pub enum ConfigError {
    /// The configuration file could not be read.
    #[error("reading configuration file {}: {source}", path.display())]
    Read {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    /// The configuration file is not a valid dotenv document.
    #[error("parsing configuration file {}: {source}", path.display())]
    Parse {
        path: PathBuf,
        #[source]
        source: dotenvy::Error,
    },
    /// A required variable is absent from the configuration source.
    #[error("missing configuration variable {key} in {location}")]
    Missing {
        location: ConfigSource,
        key: &'static str,
    },
    /// A variable is present but cannot be parsed into the expected type.
    #[error("invalid value {value:?} for {key}: {source}")]
    InvalidValue {
        key: &'static str,
        value: String,
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
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
    /// Reads and validates the configuration, preferring a dotenv file.
    ///
    /// When `path` names an existing file it is read with [`Config::load`];
    /// otherwise the settings are taken from the process environment with
    /// [`Config::from_env`]. This is what process startup should call: a
    /// developer checkout keeps a `.env` next to the sources, while a container
    /// runtime supplies the same variables through the environment.
    ///
    /// # Errors
    ///
    /// Returns the errors of whichever source was selected. A missing file is
    /// not an error by itself, but the environment must then be complete.
    pub fn resolve(path: impl AsRef<Path>) -> Result<Self, ConfigError> {
        let path = path.as_ref();
        if path.is_file() {
            Self::load(path)
        } else {
            Self::from_env()
        }
    }

    /// Reads and validates a runtime configuration file.
    ///
    /// The file is a standard `.env` document and must define `SERVER_IP`,
    /// `SERVER_PORT`, and `SERVER_CORS`. `SERVER_CORS` is a comma separated
    /// list of origins and may be empty to disable cross-origin requests. This
    /// method does not supply defaults: missing or malformed settings return an
    /// error so deployment mistakes are visible at startup.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use website_backend::Config;
    ///
    /// // SERVER_IP=0.0.0.0
    /// // SERVER_PORT=3200
    /// // SERVER_CORS=http://localhost:3000
    /// let config = Config::load(".env")?;
    ///
    /// assert_eq!(config.server().address().port(), 3200);
    /// # Ok::<(), website_backend::ConfigError>(())
    /// ```
    ///
    /// # Errors
    ///
    /// Returns [`ConfigError::Read`] when the file cannot be read,
    /// [`ConfigError::Parse`] when it is not valid dotenv syntax,
    /// [`ConfigError::Missing`] when a required variable is absent,
    /// [`ConfigError::InvalidValue`] when a variable cannot be parsed, and
    /// [`ConfigError::InvalidCorsOrigin`] when an origin is not a legal HTTP
    /// header value.
    pub fn load(path: impl AsRef<Path>) -> Result<Self, ConfigError> {
        let path = path.as_ref();
        let contents = std::fs::read_to_string(path).map_err(|source| ConfigError::Read {
            path: path.to_owned(),
            source,
        })?;
        let variables = parse_dotenv(path, &contents)?;

        Self::from_variables(&variables, &ConfigSource::File(path.to_owned()))
    }

    /// Reads and validates the configuration from the process environment.
    ///
    /// The same variables as in [`Config::load`] are expected. Container
    /// platforms — Cloudflare Containers, Docker, Kubernetes — inject them
    /// directly, so no file has to be baked into the image.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigError::Missing`], [`ConfigError::InvalidValue`], or
    /// [`ConfigError::InvalidCorsOrigin`] exactly like [`Config::load`].
    pub fn from_env() -> Result<Self, ConfigError> {
        Self::from_vars(std::env::vars())
    }

    /// Reads and validates the configuration from arbitrary variables.
    ///
    /// This is the source-agnostic entry point behind [`Config::from_env`]; it
    /// is useful for tests and for embedding the backend in a process that
    /// keeps its settings elsewhere.
    ///
    /// # Examples
    ///
    /// ```
    /// use website_backend::Config;
    ///
    /// let config = Config::from_vars([
    ///     ("SERVER_IP".to_owned(), "0.0.0.0".to_owned()),
    ///     ("SERVER_PORT".to_owned(), "3200".to_owned()),
    ///     ("SERVER_CORS".to_owned(), String::new()),
    /// ])?;
    ///
    /// assert_eq!(config.server().address().to_string(), "0.0.0.0:3200");
    /// # Ok::<(), website_backend::ConfigError>(())
    /// ```
    ///
    /// # Errors
    ///
    /// Returns [`ConfigError::Missing`], [`ConfigError::InvalidValue`], or
    /// [`ConfigError::InvalidCorsOrigin`] exactly like [`Config::load`].
    pub fn from_vars(
        variables: impl IntoIterator<Item = (String, String)>,
    ) -> Result<Self, ConfigError> {
        let variables: HashMap<String, String> = variables.into_iter().collect();

        Self::from_variables(&variables, &ConfigSource::Environment)
    }

    /// Validates the collected variables, blaming `location` on failure.
    fn from_variables(
        variables: &HashMap<String, String>,
        location: &ConfigSource,
    ) -> Result<Self, ConfigError> {
        let ip: IpAddr = parse(variables, location, SERVER_IP, |value| {
            value.parse::<IpAddr>()
        })?;
        let port: u16 = parse(variables, location, SERVER_PORT, |value| {
            value.parse::<u16>()
        })?;
        let cors_origins = get(variables, location, SERVER_CORS)?
            .split(',')
            .map(str::trim)
            .filter(|origin| !origin.is_empty())
            .map(|origin| {
                HeaderValue::from_str(origin).map_err(|source| ConfigError::InvalidCorsOrigin {
                    origin: origin.to_owned(),
                    source,
                })
            })
            .collect::<Result<Vec<_>, _>>()?;

        Ok(Self {
            server: ServerConfig {
                address: SocketAddr::new(ip, port),
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

/// Collects every key/value pair declared in a dotenv document.
fn parse_dotenv(path: &Path, contents: &str) -> Result<HashMap<String, String>, ConfigError> {
    dotenvy::from_read_iter(contents.as_bytes())
        .collect::<Result<HashMap<_, _>, _>>()
        .map_err(|source| ConfigError::Parse {
            path: path.to_owned(),
            source,
        })
}

/// Returns a required variable, reporting the source it was expected in.
fn get<'a>(
    variables: &'a HashMap<String, String>,
    location: &ConfigSource,
    key: &'static str,
) -> Result<&'a str, ConfigError> {
    variables
        .get(key)
        .map(String::as_str)
        .ok_or_else(|| ConfigError::Missing {
            location: location.clone(),
            key,
        })
}

/// Returns a required variable parsed into `T`, naming the key on failure.
fn parse<T, E>(
    variables: &HashMap<String, String>,
    location: &ConfigSource,
    key: &'static str,
    parser: impl FnOnce(&str) -> Result<T, E>,
) -> Result<T, ConfigError>
where
    E: std::error::Error + Send + Sync + 'static,
{
    let value = get(variables, location, key)?;
    parser(value).map_err(|source| ConfigError::InvalidValue {
        key,
        value: value.to_owned(),
        source: Box::new(source),
    })
}

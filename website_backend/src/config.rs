use axum::http::HeaderValue;
use axum::http::header::InvalidHeaderValue;
use thiserror::Error;

/// The environment variable holding the comma separated CORS origins.
pub const SERVER_CORS: &str = "SERVER_CORS";

/// The runtime configuration of the website backend.
///
/// A Worker binds no socket and reads no file, so the single thing left to
/// configure is which sites may call the API from a browser. The value comes
/// from the `SERVER_CORS` variable declared in the `[vars]` table of
/// `wrangler.toml`, and is validated before the router is built so a deployment
/// mistake surfaces on the first request instead of as a silent CORS failure in
/// the browser.
///
/// # Examples
///
/// ```
/// use website_backend::Config;
///
/// let config = Config::from_origins("https://aimers.dev,http://localhost:3000")?;
///
/// assert_eq!(config.cors_origins().len(), 2);
/// # Ok::<(), website_backend::ConfigError>(())
/// ```
#[derive(Debug)]
pub struct Config {
    cors_origins: Vec<HeaderValue>,
}

/// An error encountered while reading or validating the configuration.
#[derive(Debug, Error)]
pub enum ConfigError {
    /// A required variable is absent from the environment.
    #[error("missing configuration variable {key} in the Worker environment")]
    Missing {
        /// The name of the variable that was expected.
        key: &'static str,
    },
    /// A configured CORS origin cannot be represented as an HTTP header value.
    #[error("invalid CORS origin {origin:?}: {source}")]
    InvalidCorsOrigin {
        /// The origin as it was written in the configuration.
        origin: String,
        #[source]
        source: InvalidHeaderValue,
    },
}

impl Config {
    /// Reads and validates the configuration from a set of variables.
    ///
    /// `SERVER_CORS` must be present; every other variable is ignored. This is
    /// the entry point the Worker uses with the values of its `[vars]` table,
    /// and the one tests use to describe an environment inline.
    ///
    /// # Examples
    ///
    /// ```
    /// use website_backend::Config;
    ///
    /// let config = Config::from_vars([
    ///     ("SERVER_CORS".to_owned(), "https://aimers.dev".to_owned()),
    /// ])?;
    ///
    /// assert_eq!(config.cors_origins(), ["https://aimers.dev"]);
    /// # Ok::<(), website_backend::ConfigError>(())
    /// ```
    ///
    /// # Errors
    ///
    /// Returns [`ConfigError::Missing`] when `SERVER_CORS` is absent and
    /// [`ConfigError::InvalidCorsOrigin`] when one of its origins is not a legal
    /// HTTP header value.
    pub fn from_vars(
        variables: impl IntoIterator<Item = (String, String)>,
    ) -> Result<Self, ConfigError> {
        let origins = variables
            .into_iter()
            .find(|(key, _)| key == SERVER_CORS)
            .map(|(_, value)| value)
            .ok_or(ConfigError::Missing { key: SERVER_CORS })?;

        Self::from_origins(&origins)
    }

    /// Validates a comma separated list of allowed CORS origins.
    ///
    /// Surrounding whitespace is trimmed and empty entries are dropped, so an
    /// empty list — which allows no cross-origin request at all — is spelled as
    /// an empty string.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigError::InvalidCorsOrigin`] when an origin is not a legal
    /// HTTP header value.
    pub fn from_origins(origins: &str) -> Result<Self, ConfigError> {
        let cors_origins = origins
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

        Ok(Self { cors_origins })
    }

    /// Returns the HTTP header values accepted by the CORS policy.
    #[inline]
    pub fn cors_origins(&self) -> &[HeaderValue] {
        &self.cors_origins
    }
}

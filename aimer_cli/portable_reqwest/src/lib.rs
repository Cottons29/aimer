//! The bounded `reqwest` surface available to an interpreted hot-reload guest.
//!
//! Native application builds keep the real `reqwest` crate. The generated
//! guest replaces it with this small compatibility layer, which sends one
//! canonical HTTP GET request through Aimer's existing capability-call ABI.
//! No browser object, wasm-bindgen closure, or executor handle crosses the
//! guest boundary.

use std::fmt;

use aimer_anteros::CapabilityError;
#[cfg(any(target_arch = "wasm32", test))]
use aimer_anteros::{CapabilityDecoder, PORTABLE_HTTP_MAX_RESPONSE_BYTES};
#[cfg(target_arch = "wasm32")]
use aimer_anteros::{
    CapabilityCall, CapabilityEncoder, CapabilityTransport,
    PORTABLE_HTTP_CAPABILITY_ABI_MAJOR, PORTABLE_HTTP_MAX_REQUEST_BYTES, PORTABLE_HTTP_METHOD_GET,
    portable_http_capability_id,
};

/// An error returned by the portable HTTP capability.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Error(String);

impl Error {
    fn new(message: impl Into<String>) -> Self {
        Self(message.into())
    }
}

impl fmt::Display for Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for Error {}

impl From<CapabilityError> for Error {
    fn from(error: CapabilityError) -> Self {
        Self::new(format!("portable HTTP capability failed: {error:?}"))
    }
}

/// A compact HTTP status code compatible with the status methods used by the
/// website's request helpers.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct StatusCode(u16);

impl StatusCode {
    /// Returns the numeric status code.
    #[inline]
    pub const fn as_u16(self) -> u16 {
        self.0
    }

    /// Returns whether the status is in the successful 2xx range.
    #[inline]
    pub const fn is_success(self) -> bool {
        self.0 >= 200 && self.0 < 300
    }
}

impl fmt::Display for StatusCode {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}", self.0)
    }
}

/// The bounded response returned by [`get`].
pub struct Response {
    status: StatusCode,
    body: Vec<u8>,
}

impl Response {
    /// Returns the response status.
    #[inline]
    pub const fn status(&self) -> StatusCode {
        self.status
    }

    /// Converts non-successful responses into an error, preserving successful
    /// responses for the normal body path.
    pub fn error_for_status(self) -> Result<Self, Error> {
        if self.status.is_success() {
            Ok(self)
        } else {
            Err(Error::new(format!("HTTP request returned status {}", self.status)))
        }
    }

    /// Decodes the bounded response body as UTF-8.
    pub async fn text(self) -> Result<String, Error> {
        String::from_utf8(self.body).map_err(|_| Error::new("HTTP response body was not UTF-8"))
    }
}

/// Performs one bounded GET request through the host capability registry.
///
/// The current capability ABI is synchronous at the import boundary; the
/// async function shape preserves the `reqwest::get(...).await` call surface
/// while the host performs the bounded read during that poll. The request is
/// still owned by the generation-local `AsyncBuilder` task and cannot outlive
/// a cancelled guest generation.
#[cfg(target_arch = "wasm32")]
pub async fn get(url: &str) -> Result<Response, Error> {
    let mut encoder = CapabilityEncoder::new(PORTABLE_HTTP_MAX_REQUEST_BYTES);
    encoder.write_string(url)?;
    let request = encoder.into_bytes();
    let response = aimer_anteros::WasmCapabilityTransport::new().invoke(CapabilityCall::new(
        portable_http_capability_id(),
        PORTABLE_HTTP_CAPABILITY_ABI_MAJOR,
        PORTABLE_HTTP_METHOD_GET,
        &request,
        PORTABLE_HTTP_MAX_RESPONSE_BYTES,
    ))?;
    decode_response(&response)
}

#[cfg(not(target_arch = "wasm32"))]
pub async fn get(_url: &str) -> Result<Response, Error> {
    Err(Error::new(
        "aimer_portable_reqwest is available only inside a portable guest",
    ))
}

#[cfg(any(target_arch = "wasm32", test))]
fn decode_response(bytes: &[u8]) -> Result<Response, Error> {
    let mut decoder = CapabilityDecoder::new(bytes, PORTABLE_HTTP_MAX_RESPONSE_BYTES)?;
    let status = StatusCode(decoder.read_u16()?);
    let body = decoder.read_bytes()?;
    decoder.finish()?;
    Ok(Response { status, body })
}

#[cfg(test)]
mod tests {
    use super::{StatusCode, decode_response};
    use aimer_anteros::{CapabilityEncoder, PORTABLE_HTTP_MAX_RESPONSE_BYTES};

    #[test]
    fn response_budget_fits_the_cli_guest_memory_profile() {
        assert_eq!(PORTABLE_HTTP_MAX_RESPONSE_BYTES, 256 * 1024);
    }

    #[test]
    fn response_codec_preserves_status_and_body() {
        let mut encoder = CapabilityEncoder::new(PORTABLE_HTTP_MAX_RESPONSE_BYTES);
        encoder.write_u16(200).unwrap();
        encoder.write_bytes(b"hello").unwrap();

        let response = decode_response(&encoder.into_bytes()).unwrap();

        assert_eq!(response.status(), StatusCode(200));
        assert_eq!(response.text_now(), "hello");
    }

    #[test]
    fn status_success_matches_http_2xx_range() {
        assert!(!StatusCode(199).is_success());
        assert!(StatusCode(200).is_success());
        assert!(StatusCode(299).is_success());
        assert!(!StatusCode(300).is_success());
    }

    trait ResponseText {
        fn text_now(self) -> String;
    }

    impl ResponseText for super::Response {
        fn text_now(self) -> String {
            String::from_utf8(self.body).unwrap()
        }
    }
}

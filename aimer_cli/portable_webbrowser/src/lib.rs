//! Guest-only compatibility surface for applications that use `webbrowser`.
//!
//! A portable hot-reload guest has no direct browser or operating-system
//! handles. Native builds keep the application's real `webbrowser` crate;
//! this replacement keeps guest callbacks linkable and reports the unavailable
//! side effect when they are invoked.

use std::fmt;
use std::io::{Error, ErrorKind, Result};
use std::str::FromStr;

/// Browser selection accepted by the upstream `webbrowser` API.
#[derive(Debug, Default, Eq, PartialEq, Copy, Clone, Hash)]
pub enum Browser {
    /// The platform's default browser.
    #[default]
    Default,
    /// Mozilla Firefox.
    Firefox,
    /// Microsoft Internet Explorer.
    InternetExplorer,
    /// Google Chrome.
    Chrome,
    /// Opera.
    Opera,
    /// macOS Safari.
    Safari,
    /// Haiku WebPositive.
    WebPositive,
}

impl Browser {
    /// Reports that no browser is available inside a portable guest.
    #[inline]
    pub const fn is_available() -> bool {
        false
    }

    /// Reports that this browser is unavailable inside a portable guest.
    #[inline]
    pub const fn exists(&self) -> bool {
        false
    }
}

impl fmt::Display for Browser {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            Self::Default => "Default",
            Self::Firefox => "Firefox",
            Self::InternetExplorer => "Internet Explorer",
            Self::Chrome => "Chrome",
            Self::Opera => "Opera",
            Self::Safari => "Safari",
            Self::WebPositive => "WebPositive",
        };
        formatter.write_str(name)
    }
}

/// Error returned when a browser name cannot be parsed.
#[derive(Debug, Eq, PartialEq, Copy, Clone, Hash)]
pub struct ParseBrowserError;

impl fmt::Display for ParseBrowserError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("invalid browser")
    }
}

impl std::error::Error for ParseBrowserError {}

impl FromStr for Browser {
    type Err = ParseBrowserError;

    fn from_str(value: &str) -> std::result::Result<Self, Self::Err> {
        match value {
            "default" => Ok(Self::Default),
            "firefox" => Ok(Self::Firefox),
            "ie" | "internet explorer" | "internetexplorer" => Ok(Self::InternetExplorer),
            "chrome" => Ok(Self::Chrome),
            "opera" => Ok(Self::Opera),
            "safari" => Ok(Self::Safari),
            "webpositive" => Ok(Self::WebPositive),
            _ => Err(ParseBrowserError),
        }
    }
}

/// Options accepted by the upstream browser-opening API.
#[derive(Debug, Eq, PartialEq, Clone, Hash)]
pub struct BrowserOptions {
    suppress_output: bool,
    target_hint: String,
    dry_run: bool,
}

impl Default for BrowserOptions {
    fn default() -> Self {
        Self {
            suppress_output: true,
            target_hint: "_blank".to_owned(),
            dry_run: false,
        }
    }
}

impl BrowserOptions {
    /// Creates default browser options.
    #[inline]
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets whether browser output should be suppressed.
    #[inline]
    pub fn with_suppress_output(&mut self, suppress_output: bool) -> &mut Self {
        self.suppress_output = suppress_output;
        self
    }

    /// Sets the requested browser target hint.
    #[inline]
    pub fn with_target_hint(&mut self, target_hint: &str) -> &mut Self {
        self.target_hint = target_hint.to_owned();
        self
    }

    /// Sets dry-run mode. A portable guest still reports the unavailable
    /// browser because it cannot inspect a host browser window.
    #[inline]
    pub fn with_dry_run(&mut self, dry_run: bool) -> &mut Self {
        self.dry_run = dry_run;
        self
    }
}

impl fmt::Display for BrowserOptions {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.debug_struct("BrowserOptions")
            .field("suppress_output", &self.suppress_output)
            .field("target_hint", &self.target_hint)
            .field("dry_run", &self.dry_run)
            .finish()
    }
}

/// Reports that opening a browser is unavailable in the portable guest.
#[inline]
pub fn open(_url: &str) -> Result<()> {
    Err(unavailable_error())
}

/// Reports that opening a selected browser is unavailable in the portable
/// guest.
#[inline]
pub fn open_browser(_browser: Browser, _url: &str) -> Result<()> {
    Err(unavailable_error())
}

/// Reports that opening a selected browser is unavailable in the portable
/// guest, regardless of the supplied options.
#[inline]
pub fn open_browser_with_options(
    _browser: Browser,
    _url: &str,
    _options: &BrowserOptions,
) -> Result<()> {
    Err(unavailable_error())
}

fn unavailable_error() -> Error {
    Error::new(
        ErrorKind::Unsupported,
        "browser opening is unavailable in a portable hot-reload guest",
    )
}

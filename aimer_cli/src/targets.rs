use std::error::Error;
use std::fmt::Display;

#[derive(Debug, PartialEq, Eq, Copy, Clone)]
pub enum Targets {
    Macos,
    Windows,
    Linux,
    Android,
    AndroidSimulator,
    Ios,
    IosSimulator,
    Web,
    Terminated,
}

impl TryFrom<&str> for Targets {
    type Error = Box<dyn Error>;

    fn try_from(s: &str) -> Result<Self, Self::Error> {
        let res = match s {
            "macos" => Targets::Macos,
            "windows" => Targets::Windows,
            "linux" => Targets::Linux,
            "android" => Targets::Android,
            "ios" => Targets::Ios,
            "web" => Targets::Web,
            "terminated" => Targets::Terminated,
            "ios-simulator" => Targets::IosSimulator,
            "android-simulator" => Targets::AndroidSimulator,
            _ => return Err(format!("Invalid argument : {s}").into()),
        };
        Ok(res)
    }
}

impl TryFrom<String> for Targets {
    type Error = Box<dyn Error>;
    fn try_from(s: String) -> Result<Self, Self::Error> {
        Targets::try_from(s.as_str())
    }
}

impl Display for Targets {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Targets::Macos => write!(f, "macos"),
            Targets::Windows => write!(f, "windows"),
            Targets::Linux => write!(f, "linux"),
            Targets::Android => write!(f, "android"),
            Targets::Ios => write!(f, "ios"),
            Targets::Web => write!(f, "web"),
            Targets::Terminated => write!(f, "terminated"),
            Targets::IosSimulator => write!(f, "ios-simulator"),
            Targets::AndroidSimulator => write!(f, "android-simulator"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── TryFrom<&str> ────────────────────────────────────────────────

    #[test]
    fn try_from_str_valid_variants() {
        let cases = [
            ("macos", Targets::Macos),
            ("windows", Targets::Windows),
            ("linux", Targets::Linux),
            ("android", Targets::Android),
            ("ios", Targets::Ios),
            ("web", Targets::Web),
            ("terminated", Targets::Terminated),
            ("ios-simulator", Targets::IosSimulator),
            ("android-simulator", Targets::AndroidSimulator),
        ];
        for (input, expected) in cases {
            let result = Targets::try_from(input);
            assert!(result.is_ok(), "Failed for input: {input}");
            assert_eq!(result.unwrap(), expected);
        }
    }

    #[test]
    fn try_from_str_invalid_input() {
        let invalid = ["", "MacOS", "WINDOWS", "unknown", "ios_sim", "androidsim"];
        for input in invalid {
            let result = Targets::try_from(input);
            assert!(result.is_err(), "Expected error for input: {input}");
        }
    }

    // ── TryFrom<String> ──────────────────────────────────────────────

    #[test]
    fn try_from_string_valid() {
        let result = Targets::try_from(String::from("web"));
        assert_eq!(result.unwrap(), Targets::Web);
    }

    #[test]
    fn try_from_string_invalid() {
        let result = Targets::try_from(String::from("nope"));
        assert!(result.is_err());
    }

    // ── Display round-trip ───────────────────────────────────────────

    #[test]
    fn display_round_trip() {
        let variants = [
            Targets::Macos,
            Targets::Windows,
            Targets::Linux,
            Targets::Android,
            Targets::Ios,
            Targets::Web,
            Targets::Terminated,
            Targets::IosSimulator,
            Targets::AndroidSimulator,
        ];
        for variant in variants {
            let s = variant.to_string();
            let round_tripped = Targets::try_from(s.as_str()).unwrap();
            assert_eq!(variant, round_tripped);
        }
    }

    #[test]
    fn display_strings() {
        assert_eq!(Targets::Macos.to_string(), "macos");
        assert_eq!(Targets::Windows.to_string(), "windows");
        assert_eq!(Targets::Linux.to_string(), "linux");
        assert_eq!(Targets::Android.to_string(), "android");
        assert_eq!(Targets::Ios.to_string(), "ios");
        assert_eq!(Targets::Web.to_string(), "web");
        assert_eq!(Targets::Terminated.to_string(), "terminated");
        assert_eq!(Targets::IosSimulator.to_string(), "ios-simulator");
        assert_eq!(Targets::AndroidSimulator.to_string(), "android-simulator");
    }

    // ── Copy / Clone / PartialEq / Debug ─────────────────────────────

    #[test]
    fn targets_copy_and_clone() {
        let a = Targets::Macos;
        let b = a; // Copy
        let c = a; // Copy
        assert_eq!(a, b);
        assert_eq!(a, c);
    }

    #[test]
    fn targets_debug_format() {
        assert_eq!(format!("{:?}", Targets::Web), "Web");
        assert_eq!(format!("{:?}", Targets::AndroidSimulator), "AndroidSimulator");
    }

    #[test]
    fn targets_partial_eq() {
        assert_eq!(Targets::Ios, Targets::Ios);
        assert_ne!(Targets::Ios, Targets::IosSimulator);
        assert_ne!(Targets::Macos, Targets::Windows);
    }
}

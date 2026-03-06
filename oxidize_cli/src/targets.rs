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
    Terminated
}

impl TryFrom<&str> for Targets {

    type Error = Box<dyn Error>;

    fn try_from(s: &str) -> Result<Self, Self::Error> {
        let res = match s.as_ref() {
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
        Ok(Targets::try_from(s.as_ref())?)
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
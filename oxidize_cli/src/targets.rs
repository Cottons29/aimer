use std::error::Error;
use std::fmt::Display;

pub enum Targets {
    Macos,
    Windows,
    Linux,
    Android,
    Ios
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
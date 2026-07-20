use std::backtrace::Backtrace;
use std::fmt;

#[cfg(panic = "unwind")]
use std::ffi::OsStr;
#[cfg(panic = "unwind")]
use std::panic::{AssertUnwindSafe, catch_unwind};

use crate::Element;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum BuildPhase {
    CreateState,
    InitState,
    ApplyStateMutation,
    Build,
    ToElement,
    AdoptConfig,
    KeyedState,
}

impl fmt::Display for BuildPhase {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::CreateState => f.write_str("create_state"),
            Self::InitState => f.write_str("init_state"),
            Self::ApplyStateMutation => f.write_str("queued state mutation"),
            Self::Build => f.write_str("build"),
            Self::ToElement => f.write_str("child to_element"),
            Self::AdoptConfig => f.write_str("adopt_config_from"),
            Self::KeyedState => f.write_str("keyed state construction"),
        }
    }
}

#[derive(Debug)]
pub(crate) struct PanicDiagnostic {
    widget_name: &'static str,
    phase: BuildPhase,
    payload: String,
    backtrace: Option<Backtrace>,
}

impl fmt::Display for PanicDiagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Widget `{}` panicked during {}: {}",
            self.widget_name, self.phase, self.payload
        )?;
        if let Some(backtrace) = &self.backtrace {
            write!(f, "\n\nBacktrace:\n{backtrace}")?;
        }
        Ok(())
    }
}

impl PanicDiagnostic {
    pub(crate) fn into_error_element(self) -> Box<dyn Element> {
        let message = self.to_string();
        aimer_utils::log::error(&message);
        Box::new(crate::ErrorElement::new(message))
    }
}

pub(crate) fn recover_operation<T>(
    widget_name: &'static str,
    phase: BuildPhase,
    operation: impl FnOnce() -> T,
) -> Result<T, PanicDiagnostic> {
    #[cfg(panic = "unwind")]
    {
        catch_unwind(AssertUnwindSafe(operation)).map_err(|payload| PanicDiagnostic {
            widget_name,
            phase,
            payload: panic_payload(payload.as_ref()),
            backtrace: capture_backtrace(),
        })
    }

    #[cfg(not(panic = "unwind"))]
    {
        let _ = (widget_name, phase);
        Ok(operation())
    }
}

#[cfg(panic = "unwind")]
fn capture_backtrace() -> Option<Backtrace> {
    backtrace_enabled(std::env::var_os("RUST_BACKTRACE").as_deref()).then(Backtrace::force_capture)
}

#[cfg(panic = "unwind")]
fn backtrace_enabled(setting: Option<&OsStr>) -> bool {
    setting.is_some_and(|value| value != "0")
}

#[cfg(panic = "unwind")]
fn panic_payload(payload: &(dyn std::any::Any + Send)) -> String {
    if let Some(message) = payload.downcast_ref::<&str>() {
        (*message).to_owned()
    } else if let Some(message) = payload.downcast_ref::<String>() {
        message.clone()
    } else {
        "non-string panic payload".to_owned()
    }
}

pub(crate) fn build_or_error(
    widget_name: &'static str,
    phase: BuildPhase,
    operation: impl FnOnce() -> Box<dyn Element>,
) -> Box<dyn Element> {
    recover_operation(widget_name, phase, operation)
        .unwrap_or_else(|diagnostic| diagnostic.into_error_element())
}

#[cfg(all(test, panic = "unwind"))]
mod tests {
    use std::ffi::OsStr;
    use std::panic::panic_any;

    use super::*;

    #[test]
    fn recovers_borrowed_string_panic_with_widget_and_phase() {
        let diagnostic = recover_operation("MissingProviderWidget", BuildPhase::Build, || {
            panic!("No provider found")
        })
        .expect_err("panic should be recovered");

        let message = diagnostic.to_string();
        assert!(message.contains("MissingProviderWidget"));
        assert!(message.contains("build"));
        assert!(message.contains("No provider found"));
    }

    #[test]
    fn rust_backtrace_setting_controls_capture() {
        assert!(!backtrace_enabled(None));
        assert!(!backtrace_enabled(Some(OsStr::new("0"))));
        assert!(backtrace_enabled(Some(OsStr::new("1"))));
        assert!(backtrace_enabled(Some(OsStr::new("full"))));
    }

    #[test]
    fn recovered_diagnostic_respects_process_backtrace_setting() {
        let diagnostic = recover_operation("MissingProviderWidget", BuildPhase::Build, || {
            panic!("No provider found")
        })
        .expect_err("panic should be recovered");

        assert_eq!(
            diagnostic
                .backtrace
                .is_some(),
            backtrace_enabled(std::env::var_os("RUST_BACKTRACE").as_deref())
        );
    }

    #[test]
    fn diagnostic_omits_backtrace_when_not_enabled() {
        let diagnostic = PanicDiagnostic {
            widget_name: "MissingProviderWidget",
            phase: BuildPhase::Build,
            payload: "No provider found".to_owned(),
            backtrace: None,
        };

        assert!(
            !diagnostic
                .to_string()
                .contains("Backtrace:")
        );
    }

    #[test]
    fn diagnostic_prints_backtrace_when_enabled() {
        let diagnostic = PanicDiagnostic {
            widget_name: "MissingProviderWidget",
            phase: BuildPhase::Build,
            payload: "No provider found".to_owned(),
            backtrace: Some(Backtrace::disabled()),
        };

        assert!(
            diagnostic
                .to_string()
                .contains("Backtrace:")
        );
    }

    #[test]
    fn recovers_owned_string_panic_payload() {
        let diagnostic = recover_operation("OwnedPayloadWidget", BuildPhase::Build, || {
            panic_any(String::from("owned panic message"))
        })
        .expect_err("panic should be recovered");

        assert!(
            diagnostic
                .to_string()
                .contains("owned panic message")
        );
    }

    #[test]
    fn recovers_non_string_panic_payload_safely() {
        let diagnostic =
            recover_operation("OpaquePayloadWidget", BuildPhase::Build, || panic_any(42_u32))
                .expect_err("panic should be recovered");

        assert!(
            diagnostic
                .to_string()
                .contains("non-string panic payload")
        );
    }
}

use std::fmt;

use zeroize::Zeroizing;

use super::route::CommandSpec;

/// One process invocation of a development launch sequence.
///
/// A step separates public invocation data from private session data. The
/// program and arguments are visible to every user on the machine through
/// ordinary process listings, so they never carry the session token. Private
/// data travels either through the child environment of a platform tool that
/// documents a private child channel or through the standard input of the tool,
/// and both are zeroized when the step is dropped.
pub struct LaunchStep {
    command: CommandSpec,
    environment: Vec<(String, Zeroizing<String>)>,
    stdin: Option<Zeroizing<String>>,
}

impl LaunchStep {
    /// Creates a step that carries no private session data yet.
    #[inline]
    pub const fn new(command: CommandSpec) -> Self {
        Self {
            command,
            environment: Vec::new(),
            stdin: None,
        }
    }

    /// Adds one private child-environment variable.
    #[inline]
    pub fn private_environment(mut self, name: impl Into<String>, value: Zeroizing<String>) -> Self {
        self.environment.push((name.into(), value));
        self
    }

    /// Sets the private standard-input payload of this step.
    #[inline]
    pub fn private_stdin(mut self, payload: Zeroizing<String>) -> Self {
        self.stdin = Some(payload);
        self
    }

    /// Returns the public invocation of this step.
    #[inline]
    pub const fn command(&self) -> &CommandSpec {
        &self.command
    }

    /// Returns the private child-environment variables of this step.
    #[inline]
    pub fn environment(&self) -> &[(String, Zeroizing<String>)] {
        &self.environment
    }

    /// Returns the private standard-input payload of this step.
    #[inline]
    pub fn stdin(&self) -> Option<&str> {
        self.stdin.as_deref().map(String::as_str)
    }
}

impl fmt::Debug for LaunchStep {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("LaunchStep")
            .field("command", &self.command)
            .field(
                "environment",
                &self
                    .environment
                    .iter()
                    .map(|(name, _)| (name.as_str(), "[REDACTED]"))
                    .collect::<Vec<_>>(),
            )
            .field("stdin", &self.stdin.as_ref().map(|_| "[REDACTED]"))
            .finish()
    }
}

/// The ordered process invocations that start one development app.
///
/// Targets that provision private session data before starting the app own more
/// than one step; the app itself is always the final step, so a pipeline can
/// stream its console output without inspecting the target family.
#[derive(Debug)]
pub struct LaunchConfiguration {
    steps: Vec<LaunchStep>,
}

impl LaunchConfiguration {
    /// Creates a configuration whose last step starts the app.
    ///
    /// # Panics
    ///
    /// Panics when `steps` is empty, because a launch always starts one app.
    #[inline]
    pub fn new(steps: Vec<LaunchStep>) -> Self {
        assert!(!steps.is_empty(), "a launch configuration starts one app");
        Self { steps }
    }

    /// Returns every step in execution order.
    #[inline]
    pub fn steps(&self) -> &[LaunchStep] {
        &self.steps
    }

    /// Returns the step that starts the app and streams its console output.
    #[inline]
    pub fn app(&self) -> &LaunchStep {
        self.steps.last().expect("a launch starts one app")
    }

    /// Returns every text another local user can observe in a process listing.
    ///
    /// Adapter tests assert that no session secret ever appears here, which is
    /// the exact boundary the threat model cares about: private child
    /// environments and standard input are not part of this iterator.
    pub fn public_text(&self) -> impl Iterator<Item = &str> {
        self.steps.iter().flat_map(|step| {
            std::iter::once(step.command().program())
                .chain(step.command().arguments().iter().map(String::as_str))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn step() -> LaunchStep {
        LaunchStep::new(CommandSpec::new(
            "xcrun",
            vec!["simctl".into(), "launch".into()],
        ))
        .private_environment("SECRET_TOKEN", Zeroizing::new("cafebabe".to_owned()))
        .private_stdin(Zeroizing::new("deadbeef".to_owned()))
    }

    #[test]
    fn a_launch_keeps_private_data_out_of_every_observable_argument() {
        let configuration = LaunchConfiguration::new(vec![
            LaunchStep::new(CommandSpec::new("adb", vec!["push".into()])),
            step(),
        ]);

        let public: Vec<_> = configuration.public_text().collect();

        assert_eq!(public, ["adb", "push", "xcrun", "simctl", "launch"]);
        assert!(!public.iter().any(|text| text.contains("cafebabe")));
        assert!(!public.iter().any(|text| text.contains("deadbeef")));
        assert_eq!(configuration.app().stdin(), Some("deadbeef"));
        assert_eq!(
            configuration.app().environment()[0].1.as_str(),
            "cafebabe"
        );
    }

    #[test]
    fn launch_diagnostics_name_private_channels_without_their_values() {
        let diagnostic = format!("{:?}", step());

        assert!(diagnostic.contains("SECRET_TOKEN"));
        assert!(diagnostic.contains("[REDACTED]"));
        assert!(!diagnostic.contains("cafebabe"));
        assert!(!diagnostic.contains("deadbeef"));
    }

    #[test]
    #[should_panic(expected = "a launch configuration starts one app")]
    fn a_launch_without_an_app_step_is_rejected() {
        LaunchConfiguration::new(Vec::new());
    }
}

use std::fmt;

/// The deterministic startup stages for one development hot-reload session.
///
/// The order mirrors the hot-reload pipeline contract: every stage observes the
/// already resolved execution policy instead of re-deriving it, and no stage
/// may run before the stage that produces its inputs. The initial guest stage
/// starts the guest branch; [`PipelineStage::PushInitialModule`] is the join
/// barrier that makes the completed guest available to the host. Shutdown is
/// not a stage because it must also run for every failed startup;
/// [`PipelineDriver::cleanup`] owns it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PipelineStage {
    /// Validate policy, target, toolchains, device selection, and prerequisites.
    Resolve,
    /// Generate the ephemeral session secret and its private launch injection.
    CreateSession,
    /// Compile the permanent native host with its development-only features.
    BuildHost,
    /// Start compilation and local validation of the first portable guest module.
    BuildInitialGuest,
    /// Package the native host without the mutable guest artifact.
    Assemble,
    /// Reserve the target-specific loopback, forward, or discovery path.
    PrepareRoute,
    /// Launch the debug host with the injected session data.
    LaunchApp,
    /// Connect, mutually authenticate, and verify runtime compatibility.
    DiscoverAndAuthenticate,
    /// Install the first complete guest generation and await its commit.
    PushInitialModule,
    /// Process subsequent source notifications until shutdown.
    Watch,
}

const STAGES: [PipelineStage; 10] = [
    PipelineStage::Resolve,
    PipelineStage::CreateSession,
    PipelineStage::BuildInitialGuest,
    PipelineStage::BuildHost,
    PipelineStage::Assemble,
    PipelineStage::PrepareRoute,
    PipelineStage::LaunchApp,
    PipelineStage::DiscoverAndAuthenticate,
    PipelineStage::PushInitialModule,
    PipelineStage::Watch,
];

impl PipelineStage {
    /// Returns the stable lowercase diagnostic name of this stage.
    pub const fn name(self) -> &'static str {
        match self {
            Self::Resolve => "resolve",
            Self::CreateSession => "create session",
            Self::BuildHost => "build host",
            Self::BuildInitialGuest => "build initial guest",
            Self::Assemble => "assemble",
            Self::PrepareRoute => "prepare route",
            Self::LaunchApp => "launch app",
            Self::DiscoverAndAuthenticate => "discover and authenticate",
            Self::PushInitialModule => "push initial module",
            Self::Watch => "watch",
        }
    }
}

/// Successful output of an individual pipeline stage.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StageOutput {
    /// The stage completed without installing a generation.
    Complete,
    /// The initial guest generation committed at the app safe point.
    InitialCommit,
}

/// Production operations required by one complete hot-reload session.
///
/// Implementations own every resource created by the workflow, including
/// compiler processes, the launched application, target routes, filesystem
/// watchers, and session credentials. Each method is called at most once during
/// startup and in the order documented by [`PipelineStage`]. [`Self::cleanup`]
/// is called exactly once after either success or failure and must release only
/// resources created by this implementation.
pub trait PipelineOperations {
    /// Resolves and validates project, target, and toolchain inputs.
    fn resolve(&mut self) -> Result<(), String>;

    /// Creates the ephemeral authenticated development session.
    fn create_session(&mut self) -> Result<(), String>;

    /// Builds the permanent native host with reload listener support.
    fn build_host(&mut self) -> Result<(), String>;

    /// Starts generating, building, and validating the first portable guest module.
    ///
    /// Implementations may return before the module is ready. The initial
    /// push operation is the completion barrier and must not publish a module
    /// until this branch has joined successfully.
    fn build_initial_guest(&mut self) -> Result<(), String>;

    /// Assembles the native host without embedding the mutable guest module.
    fn assemble(&mut self) -> Result<(), String>;

    /// Reserves the target-specific transport route owned by this run.
    fn prepare_route(&mut self) -> Result<(), String>;

    /// Launches the native application through the target adapter.
    fn launch_app(&mut self) -> Result<(), String>;

    /// Awaits readiness and authenticates a compatible listener.
    fn discover_and_authenticate(&mut self) -> Result<(), String>;

    /// Joins the initial guest branch, installs its module, and waits for its safe-point commit.
    fn push_initial_module(&mut self) -> Result<(), String>;

    /// Watches sources, rebuilds guests, and reconnects until shutdown.
    fn watch(&mut self) -> Result<(), String>;

    /// Releases every process, route, watcher, and secret owned by this run.
    fn cleanup(&mut self);
}

/// Concrete pipeline driver that joins production operations to stage ordering.
///
/// The driver deliberately contains no platform branching. A production
/// [`PipelineOperations`] implementation owns those details, while deterministic
/// tests can inject the same public boundary without spawning toolchains or
/// devices.
pub struct ProductionPipelineDriver<O> {
    operations: O,
}

impl<O> ProductionPipelineDriver<O> {
    /// Creates a driver that owns all operations and their session resources.
    #[inline]
    pub const fn new(operations: O) -> Self {
        Self { operations }
    }

    /// Returns the operations owned by this driver.
    #[inline]
    pub const fn operations(&self) -> &O {
        &self.operations
    }
}

impl<O> PipelineDriver for ProductionPipelineDriver<O>
where
    O: PipelineOperations,
{
    fn run_stage(&mut self, stage: PipelineStage) -> Result<StageOutput, String> {
        match stage {
            PipelineStage::Resolve => self.operations.resolve()?,
            PipelineStage::CreateSession => self.operations.create_session()?,
            PipelineStage::BuildHost => self.operations.build_host()?,
            PipelineStage::BuildInitialGuest => self.operations.build_initial_guest()?,
            PipelineStage::Assemble => self.operations.assemble()?,
            PipelineStage::PrepareRoute => self.operations.prepare_route()?,
            PipelineStage::LaunchApp => self.operations.launch_app()?,
            PipelineStage::DiscoverAndAuthenticate => {
                self.operations.discover_and_authenticate()?
            }
            PipelineStage::PushInitialModule => {
                self.operations.push_initial_module()?;
                return Ok(StageOutput::InitialCommit);
            }
            PipelineStage::Watch => self.operations.watch()?,
        }
        Ok(StageOutput::Complete)
    }

    fn cleanup(&mut self) {
        self.operations.cleanup();
    }
}

/// Side-effecting operations owned by the CLI runner rather than the pipeline.
pub trait PipelineDriver {
    /// Executes one stage in the exact order supplied by the pipeline.
    fn run_stage(&mut self, stage: PipelineStage) -> Result<StageOutput, String>;

    /// Performs bounded shutdown of routes, processes, watchers, and secrets.
    ///
    /// The pipeline calls this exactly once for both successful and failed
    /// startups, so a driver must release only resources this run created.
    fn cleanup(&mut self);
}

/// A startup or watch-loop failure with its stable stage boundary.
#[derive(Debug, Eq, PartialEq)]
pub struct PipelineError {
    stage: PipelineStage,
    diagnostic: String,
}

impl PipelineError {
    /// Returns the stage that failed or violated its contract.
    #[inline]
    pub const fn stage(&self) -> PipelineStage {
        self.stage
    }
}

impl fmt::Display for PipelineError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "hot-reload {} failed: {}",
            self.stage.name(),
            self.diagnostic
        )
    }
}

impl std::error::Error for PipelineError {}

/// Runs the complete startup workflow and always releases owned resources.
pub fn run_pipeline(driver: &mut impl PipelineDriver) -> Result<(), PipelineError> {
    let result = run_stages(driver);
    driver.cleanup();
    result
}

fn run_stages(driver: &mut impl PipelineDriver) -> Result<(), PipelineError> {
    for stage in STAGES {
        let output = driver
            .run_stage(stage)
            .map_err(|diagnostic| PipelineError { stage, diagnostic })?;
        if stage == PipelineStage::PushInitialModule && output != StageOutput::InitialCommit {
            return Err(PipelineError {
                stage,
                diagnostic: "the app did not commit the initial guest generation".to_owned(),
            });
        }
        if stage != PipelineStage::PushInitialModule && output == StageOutput::InitialCommit {
            return Err(PipelineError {
                stage,
                diagnostic: "a non-push stage reported an initial commit".to_owned(),
            });
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct RecordingOperations {
        calls: Vec<&'static str>,
    }

    impl PipelineOperations for RecordingOperations {
        fn resolve(&mut self) -> Result<(), String> {
            self.calls.push("resolve");
            Ok(())
        }

        fn create_session(&mut self) -> Result<(), String> {
            self.calls.push("create session");
            Ok(())
        }

        fn build_host(&mut self) -> Result<(), String> {
            self.calls.push("build host");
            Ok(())
        }

        fn build_initial_guest(&mut self) -> Result<(), String> {
            self.calls.push("build initial guest");
            Ok(())
        }

        fn assemble(&mut self) -> Result<(), String> {
            self.calls.push("assemble");
            Ok(())
        }

        fn prepare_route(&mut self) -> Result<(), String> {
            self.calls.push("prepare route");
            Ok(())
        }

        fn launch_app(&mut self) -> Result<(), String> {
            self.calls.push("launch app");
            Ok(())
        }

        fn discover_and_authenticate(&mut self) -> Result<(), String> {
            self.calls.push("discover and authenticate");
            Ok(())
        }

        fn push_initial_module(&mut self) -> Result<(), String> {
            self.calls.push("push initial module");
            Ok(())
        }

        fn watch(&mut self) -> Result<(), String> {
            self.calls.push("watch");
            Ok(())
        }

        fn cleanup(&mut self) {
            self.calls.push("cleanup");
        }
    }

    #[test]
    fn production_driver_joins_every_operation_and_owned_cleanup() {
        let mut driver = ProductionPipelineDriver::new(RecordingOperations::default());

        assert_eq!(run_pipeline(&mut driver), Ok(()));

        assert_eq!(
            driver.operations().calls,
            [
                "resolve",
                "create session",
                "build initial guest",
                "build host",
                "assemble",
                "prepare route",
                "launch app",
                "discover and authenticate",
                "push initial module",
                "watch",
                "cleanup",
            ]
        );
    }

    #[derive(Default)]
    struct RecordingDriver {
        stages: Vec<PipelineStage>,
        fail_at: Option<PipelineStage>,
        initial_committed: bool,
        cleanup_count: usize,
    }

    impl PipelineDriver for RecordingDriver {
        fn run_stage(&mut self, stage: PipelineStage) -> Result<StageOutput, String> {
            self.stages.push(stage);
            if self.fail_at == Some(stage) {
                return Err("injected failure".to_owned());
            }
            if stage == PipelineStage::PushInitialModule && self.initial_committed {
                Ok(StageOutput::InitialCommit)
            } else {
                Ok(StageOutput::Complete)
            }
        }

        fn cleanup(&mut self) {
            self.cleanup_count += 1;
        }
    }

    #[test]
    fn pipeline_runs_the_contract_stage_order_before_watching() {
        let mut successful = RecordingDriver {
            initial_committed: true,
            ..Default::default()
        };

        assert_eq!(run_pipeline(&mut successful), Ok(()));

        assert_eq!(
            successful.stages,
            [
                PipelineStage::Resolve,
                PipelineStage::CreateSession,
                PipelineStage::BuildInitialGuest,
                PipelineStage::BuildHost,
                PipelineStage::Assemble,
                PipelineStage::PrepareRoute,
                PipelineStage::LaunchApp,
                PipelineStage::DiscoverAndAuthenticate,
                PipelineStage::PushInitialModule,
                PipelineStage::Watch,
            ]
        );
        assert_eq!(successful.cleanup_count, 1);
    }

    #[test]
    fn pipeline_requires_the_initial_commit_and_cleans_up_after_every_stage_failure() {
        let mut uncommitted = RecordingDriver::default();

        let error = run_pipeline(&mut uncommitted).unwrap_err();

        assert_eq!(error.stage(), PipelineStage::PushInitialModule);
        assert_eq!(uncommitted.cleanup_count, 1);
        assert!(!uncommitted.stages.contains(&PipelineStage::Watch));

        for stage in STAGES {
            let mut failing = RecordingDriver {
                fail_at: Some(stage),
                initial_committed: true,
                ..Default::default()
            };

            let error = run_pipeline(&mut failing).unwrap_err();

            assert_eq!(error.stage(), stage);
            assert_eq!(failing.cleanup_count, 1);
            assert_eq!(*failing.stages.last().unwrap(), stage);
            assert!(error.to_string().starts_with(&format!(
                "hot-reload {} failed: injected failure",
                stage.name()
            )));
        }
    }
}

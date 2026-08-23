use std::fmt;
use std::sync::Arc;

/// A bounded collection of deterministic Phase 10 measurement samples.
///
/// The campaign chooses its sample capacity before recording begins. Recording
/// never grows the allocation beyond that capacity, and finalization consumes
/// the collection so percentile calculation sorts in place without copying the
/// potentially large sample set.
#[derive(Debug)]
pub struct MeasurementSeries {
    samples: Vec<u64>,
    maximum: usize,
}

impl MeasurementSeries {
    /// Creates an empty series with an explicit maximum sample count.
    pub fn new(maximum: usize) -> Result<Self, MeasurementError> {
        let mut samples = Vec::new();
        samples
            .try_reserve_exact(maximum)
            .map_err(|_| MeasurementError::SampleCapacityUnavailable { maximum })?;
        Ok(Self { samples, maximum })
    }

    /// Records one integer sample without growing beyond the campaign bound.
    #[inline]
    pub fn record(&mut self, sample: u64) -> Result<(), MeasurementError> {
        if self.samples.len() == self.maximum {
            return Err(MeasurementError::SampleLimitExceeded {
                maximum: self.maximum,
            });
        }
        self.samples.push(sample);
        Ok(())
    }

    /// Returns the number of samples recorded so far.
    #[inline]
    pub const fn len(&self) -> usize {
        self.samples.len()
    }

    /// Returns whether the series has no samples.
    #[inline]
    pub const fn is_empty(&self) -> bool {
        self.samples.is_empty()
    }

    /// Consumes the series and calculates its nearest-rank distribution.
    pub fn into_distribution(mut self) -> Result<MeasurementDistribution, MeasurementError> {
        if self.samples.is_empty() {
            return Err(MeasurementError::EmptySeries);
        }
        self.samples.sort_unstable();
        Ok(MeasurementDistribution {
            sample_count: self.samples.len(),
            median: percentile(&self.samples, 50),
            p95: percentile(&self.samples, 95),
            p99: percentile(&self.samples, 99),
            worst: *self.samples.last().expect("a measured series is non-empty"),
        })
    }
}

fn percentile(sorted: &[u64], percentile: usize) -> u64 {
    let quotient = sorted.len() / 100;
    let remainder = sorted.len() % 100;
    let rank = quotient * percentile + (remainder * percentile).div_ceil(100);
    sorted[rank.saturating_sub(1)]
}

/// Deterministic nearest-rank statistics for one measured metric.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MeasurementDistribution {
    sample_count: usize,
    median: u64,
    p95: u64,
    p99: u64,
    worst: u64,
}

impl MeasurementDistribution {
    /// Returns the number of observations represented by this distribution.
    #[inline]
    pub const fn sample_count(self) -> usize {
        self.sample_count
    }

    /// Returns the nearest-rank median observation.
    #[inline]
    pub const fn median(self) -> u64 {
        self.median
    }

    /// Returns the nearest-rank 95th-percentile observation.
    #[inline]
    pub const fn p95(self) -> u64 {
        self.p95
    }

    /// Returns the nearest-rank 99th-percentile observation.
    #[inline]
    pub const fn p99(self) -> u64 {
        self.p99
    }

    /// Returns the largest observed value.
    #[inline]
    pub const fn worst(self) -> u64 {
        self.worst
    }
}

/// Failure to allocate, record, or summarize a bounded measurement campaign.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MeasurementError {
    /// The declared bounded sample allocation could not be reserved.
    SampleCapacityUnavailable { maximum: usize },
    /// Recording would exceed the declared campaign sample count.
    SampleLimitExceeded { maximum: usize },
    /// A distribution cannot be calculated without observations.
    EmptySeries,
}

impl fmt::Display for MeasurementError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SampleCapacityUnavailable { maximum } => write!(
                formatter,
                "cannot reserve the bounded measurement capacity of {maximum} samples"
            ),
            Self::SampleLimitExceeded { maximum } => write!(
                formatter,
                "measurement campaign exceeds its {maximum}-sample limit"
            ),
            Self::EmptySeries => {
                formatter.write_str("measurement campaign contains no observations")
            }
        }
    }
}

impl std::error::Error for MeasurementError {}

/// One application complexity class required by the Phase 10 baseline matrix.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReferenceApp {
    /// A minimal reference application measuring fixed startup overhead.
    Small,
    /// A representative application with callbacks and persistent state.
    Stateful,
    /// An adversarial application whose inputs approach approved hard limits.
    NearLimit,
}

/// One hardware class required before numeric budgets can be approved.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TargetClass {
    /// A resource-constrained physical mobile target.
    LowResourceMobile,
    /// A representative native desktop target.
    Desktop,
}

/// Pinned metadata for one completed baseline run.
#[derive(Debug)]
pub struct BaselineRun {
    metric: PerformanceMetric,
    application: ReferenceApp,
    target_class: TargetClass,
    hardware: String,
    toolchain: String,
    configuration: String,
    sample_count: usize,
}

impl BaselineRun {
    /// Creates one run whose hardware, toolchain, and configuration are pinned.
    pub fn new(
        metric: PerformanceMetric,
        application: ReferenceApp,
        target_class: TargetClass,
        hardware: impl Into<String>,
        toolchain: impl Into<String>,
        configuration: impl Into<String>,
        sample_count: usize,
    ) -> Result<Self, BaselineError> {
        let hardware = hardware.into();
        let toolchain = toolchain.into();
        let configuration = configuration.into();
        if hardware.trim().is_empty() {
            return Err(BaselineError::InvalidRun("hardware must be recorded"));
        }
        if toolchain.trim().is_empty() {
            return Err(BaselineError::InvalidRun("toolchain must be recorded"));
        }
        if configuration.trim().is_empty() {
            return Err(BaselineError::InvalidRun(
                "build configuration must be recorded",
            ));
        }
        if sample_count == 0 {
            return Err(BaselineError::InvalidRun(
                "a baseline run must contain samples",
            ));
        }
        Ok(Self {
            metric,
            application,
            target_class,
            hardware,
            toolchain,
            configuration,
            sample_count,
        })
    }

    /// Returns the performance metric measured by this run.
    #[inline]
    pub const fn metric(&self) -> PerformanceMetric {
        self.metric
    }

    /// Returns the reference application measured by this run.
    #[inline]
    pub const fn application(&self) -> ReferenceApp {
        self.application
    }

    /// Returns the target class measured by this run.
    #[inline]
    pub const fn target_class(&self) -> TargetClass {
        self.target_class
    }

    /// Returns the recorded hardware description.
    #[inline]
    pub fn hardware(&self) -> &str {
        &self.hardware
    }

    /// Returns the pinned Rust and platform toolchain description.
    #[inline]
    pub fn toolchain(&self) -> &str {
        &self.toolchain
    }

    /// Returns the exact target and build configuration.
    #[inline]
    pub fn configuration(&self) -> &str {
        &self.configuration
    }

    /// Returns the number of deterministic observations in this run.
    #[inline]
    pub const fn sample_count(&self) -> usize {
        self.sample_count
    }
}

/// Coverage accumulated by a baseline campaign.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct BaselineCoverage {
    /// Whether the small app was measured on low-resource mobile hardware.
    pub small_mobile: bool,
    /// Whether the small app was measured on representative desktop hardware.
    pub small_desktop: bool,
    /// Whether the stateful app was measured on low-resource mobile hardware.
    pub stateful_mobile: bool,
    /// Whether the stateful app was measured on representative desktop hardware.
    pub stateful_desktop: bool,
    /// Whether the near-limit app was measured on low-resource mobile hardware.
    pub near_limit_mobile: bool,
    /// Whether the near-limit app was measured on representative desktop hardware.
    pub near_limit_desktop: bool,
}

impl BaselineCoverage {
    #[inline]
    const fn is_complete(self) -> bool {
        self.small_mobile
            && self.small_desktop
            && self.stateful_mobile
            && self.stateful_desktop
            && self.near_limit_mobile
            && self.near_limit_desktop
    }
}

/// A bounded set of baseline runs awaiting Phase 10 coverage validation.
#[derive(Debug)]
pub struct BaselineCampaign {
    runs: Vec<BaselineRun>,
    maximum: usize,
    coverage: BaselineCoverage,
    metric: Option<PerformanceMetric>,
}

impl BaselineCampaign {
    /// Creates a campaign with an explicit maximum number of recorded runs.
    pub fn new(maximum: usize) -> Result<Self, BaselineError> {
        let mut runs = Vec::new();
        runs.try_reserve_exact(maximum)
            .map_err(|_| BaselineError::RunCapacityUnavailable { maximum })?;
        Ok(Self {
            runs,
            maximum,
            coverage: BaselineCoverage::default(),
            metric: None,
        })
    }

    /// Records one completed run without growing the campaign allocation.
    pub fn record(&mut self, run: BaselineRun) -> Result<(), BaselineError> {
        if self.runs.len() == self.maximum {
            return Err(BaselineError::RunLimitExceeded {
                maximum: self.maximum,
            });
        }
        match self.metric {
            Some(expected) if expected != run.metric() => {
                return Err(BaselineError::MixedMetrics {
                    expected,
                    actual: run.metric(),
                });
            }
            None => self.metric = Some(run.metric()),
            Some(_) => {}
        }
        match (run.application(), run.target_class()) {
            (ReferenceApp::Small, TargetClass::LowResourceMobile) => {
                self.coverage.small_mobile = true;
            }
            (ReferenceApp::Small, TargetClass::Desktop) => {
                self.coverage.small_desktop = true;
            }
            (ReferenceApp::Stateful, TargetClass::LowResourceMobile) => {
                self.coverage.stateful_mobile = true;
            }
            (ReferenceApp::Stateful, TargetClass::Desktop) => {
                self.coverage.stateful_desktop = true;
            }
            (ReferenceApp::NearLimit, TargetClass::LowResourceMobile) => {
                self.coverage.near_limit_mobile = true;
            }
            (ReferenceApp::NearLimit, TargetClass::Desktop) => {
                self.coverage.near_limit_desktop = true;
            }
        }
        self.runs.push(run);
        Ok(())
    }

    /// Validates mandatory matrix coverage and seals the evidence for approval.
    pub fn finish(self) -> Result<BaselineEvidence, BaselineError> {
        if !self.coverage.is_complete() {
            return Err(BaselineError::IncompleteCoverage(self.coverage));
        }
        Ok(BaselineEvidence {
            metric: self.metric.expect("complete coverage contains measured runs"),
            runs: self.runs,
        })
    }
}

/// Complete measured evidence required to approve numeric budgets.
#[derive(Debug)]
pub struct BaselineEvidence {
    metric: PerformanceMetric,
    runs: Vec<BaselineRun>,
}

impl BaselineEvidence {
    /// Returns the single performance metric measured by every sealed run.
    #[inline]
    pub const fn metric(&self) -> PerformanceMetric {
        self.metric
    }
    /// Returns all pinned runs that satisfied the mandatory target matrix.
    #[inline]
    pub fn runs(&self) -> &[BaselineRun] {
        &self.runs
    }
}

/// Failure while recording or validating a Phase 10 baseline campaign.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BaselineError {
    /// The declared bounded run allocation could not be reserved.
    RunCapacityUnavailable { maximum: usize },
    /// Recording would exceed the declared campaign run count.
    RunLimitExceeded { maximum: usize },
    /// Required metadata or observations are absent from one run.
    InvalidRun(&'static str),
    /// A single evidence campaign attempted to combine unrelated metrics.
    MixedMetrics {
        expected: PerformanceMetric,
        actual: PerformanceMetric,
    },
    /// The campaign lacks at least one mandatory application or target class.
    IncompleteCoverage(BaselineCoverage),
}

impl fmt::Display for BaselineError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::RunCapacityUnavailable { maximum } => write!(
                formatter,
                "cannot reserve the bounded baseline capacity of {maximum} runs"
            ),
            Self::RunLimitExceeded { maximum } => {
                write!(formatter, "baseline campaign exceeds its {maximum}-run limit")
            }
            Self::InvalidRun(detail) => formatter.write_str(detail),
            Self::MixedMetrics { expected, actual } => write!(
                formatter,
                "baseline campaign for {expected:?} cannot include {actual:?} measurements"
            ),
            Self::IncompleteCoverage(_) => formatter.write_str(
                "baseline campaign requires small, stateful, and near-limit applications on low-resource mobile and desktop targets",
            ),
        }
    }
}

impl std::error::Error for BaselineError {}

/// A metric required by the Phase 10 performance matrix.
///
/// Durations use nanoseconds, sizes use bytes, throughput uses bytes per
/// second, fuel and wakeups use integer counts, and retained resources use the
/// host registry count after a cycle returns to its idle baseline.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PerformanceMetric {
    SourceChangeToBuildStart,
    GuestCompileDuration,
    GuestArtifactSize,
    GuestValidationDuration,
    DiscoveryAuthenticationDuration,
    ReconnectDuration,
    UploadThroughput,
    PeakStagingMemory,
    ModuleValidationDuration,
    ModuleInstantiationDuration,
    StateExportDuration,
    StateMigrationDuration,
    StateImportDuration,
    StateVerificationDuration,
    StateBytes,
    InitialBuildDuration,
    WidgetIrValidationDuration,
    NativeMaterializationDuration,
    ReconciliationPlanDuration,
    EventBarrierDuration,
    QueuedEventCount,
    SafePointCommitDuration,
    FirstPostCommitFrameDuration,
    GuestCallbackDuration,
    GuestCallbackFuel,
    IdleListenerCpuTime,
    IdleListenerWakeups,
    IdleListenerMemory,
    RetainedMemory,
    RetainedResources,
    NativeAotBinarySize,
    NativeAotStartupDuration,
}

/// The reported distribution statistic compared with one approved budget.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BudgetStatistic {
    Median,
    P95,
    P99,
    Worst,
}

impl BudgetStatistic {
    #[inline]
    const fn observe(self, distribution: MeasurementDistribution) -> u64 {
        match self {
            Self::Median => distribution.median(),
            Self::P95 => distribution.p95(),
            Self::P99 => distribution.p99(),
            Self::Worst => distribution.worst(),
        }
    }
}

/// Whether lower or higher values are better for one metric.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BudgetDirection {
    /// Values at or below the threshold pass, as for latency and memory.
    AtMost,
    /// Values at or above the threshold pass, as for upload throughput.
    AtLeast,
}

/// An approved warning and hard gate backed by complete baseline evidence.
#[derive(Debug)]
pub struct PerformanceBudget {
    metric: PerformanceMetric,
    statistic: BudgetStatistic,
    direction: BudgetDirection,
    soft_warning: u64,
    hard_gate: u64,
    rationale: String,
    review_date: String,
    evidence: Arc<BaselineEvidence>,
}

impl PerformanceBudget {
    /// Creates a budget only when thresholds and approval metadata are valid.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        metric: PerformanceMetric,
        statistic: BudgetStatistic,
        direction: BudgetDirection,
        soft_warning: u64,
        hard_gate: u64,
        rationale: impl Into<String>,
        review_date: impl Into<String>,
        evidence: Arc<BaselineEvidence>,
    ) -> Result<Self, BudgetError> {
        let valid_order = match direction {
            BudgetDirection::AtMost => soft_warning <= hard_gate,
            BudgetDirection::AtLeast => soft_warning >= hard_gate,
        };
        if !valid_order {
            return Err(BudgetError::InvalidThresholdOrder);
        }
        let rationale = rationale.into();
        if rationale.trim().is_empty() {
            return Err(BudgetError::MissingRationale);
        }
        let review_date = review_date.into();
        if review_date.trim().is_empty() {
            return Err(BudgetError::MissingReviewDate);
        }
        if evidence.metric() != metric {
            return Err(BudgetError::MetricMismatch {
                budget: metric,
                evidence: evidence.metric(),
            });
        }
        Ok(Self {
            metric,
            statistic,
            direction,
            soft_warning,
            hard_gate,
            rationale,
            review_date,
            evidence,
        })
    }

    /// Evaluates the budget's named statistic against both approved thresholds.
    pub fn evaluate(&self, distribution: MeasurementDistribution) -> BudgetOutcome {
        let observed = self.statistic.observe(distribution);
        let passes_soft = match self.direction {
            BudgetDirection::AtMost => observed <= self.soft_warning,
            BudgetDirection::AtLeast => observed >= self.soft_warning,
        };
        if passes_soft {
            return BudgetOutcome::Pass { observed };
        }
        let passes_hard = match self.direction {
            BudgetDirection::AtMost => observed <= self.hard_gate,
            BudgetDirection::AtLeast => observed >= self.hard_gate,
        };
        if passes_hard {
            BudgetOutcome::Warning { observed }
        } else {
            BudgetOutcome::HardFailure { observed }
        }
    }

    /// Returns the metric governed by this budget.
    #[inline]
    pub const fn metric(&self) -> PerformanceMetric {
        self.metric
    }

    /// Returns the distribution statistic compared with the thresholds.
    #[inline]
    pub const fn statistic(&self) -> BudgetStatistic {
        self.statistic
    }

    /// Returns the soft warning threshold.
    #[inline]
    pub const fn soft_warning(&self) -> u64 {
        self.soft_warning
    }

    /// Returns the hard failure threshold.
    #[inline]
    pub const fn hard_gate(&self) -> u64 {
        self.hard_gate
    }

    /// Returns the measured rationale for the selected thresholds.
    #[inline]
    pub fn rationale(&self) -> &str {
        &self.rationale
    }

    /// Returns the date on which this budget must next be reviewed.
    #[inline]
    pub fn review_date(&self) -> &str {
        &self.review_date
    }

    /// Returns the complete pinned baseline evidence that approved this budget.
    #[inline]
    pub fn evidence(&self) -> &BaselineEvidence {
        &self.evidence
    }
}

/// Result of evaluating one measured distribution against an approved budget.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BudgetOutcome {
    /// The selected statistic met the soft threshold.
    Pass { observed: u64 },
    /// The selected statistic exceeded the warning but met the hard gate.
    Warning { observed: u64 },
    /// The selected statistic failed the hard gate and blocks the milestone.
    HardFailure { observed: u64 },
}

/// Invalid metadata or thresholds supplied for a performance budget.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BudgetError {
    /// Warning and hard thresholds are ordered contrary to the metric direction.
    InvalidThresholdOrder,
    /// The budget does not explain why its thresholds were selected.
    MissingRationale,
    /// The budget has no explicit review date.
    MissingReviewDate,
    /// The sealed evidence measured a different metric from this budget.
    MetricMismatch {
        budget: PerformanceMetric,
        evidence: PerformanceMetric,
    },
}

impl fmt::Display for BudgetError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidThresholdOrder => formatter.write_str(
                "budget warning and hard thresholds are ordered contrary to the metric direction",
            ),
            Self::MissingRationale => {
                formatter.write_str("budget approval requires a measured rationale")
            }
            Self::MissingReviewDate => {
                formatter.write_str("budget approval requires a review date")
            }
            Self::MetricMismatch { budget, evidence } => write!(
                formatter,
                "budget for {budget:?} cannot use {evidence:?} baseline evidence"
            ),
        }
    }
}

impl std::error::Error for BudgetError {}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;

    #[test]
    fn a_bounded_series_reports_deterministic_percentiles_and_worst_case() {
        let mut series = MeasurementSeries::new(20).unwrap();
        for sample in [
            20, 1, 19, 2, 18, 3, 17, 4, 16, 5, 15, 6, 14, 7, 13, 8, 12, 9, 11, 10,
        ] {
            series.record(sample).unwrap();
        }

        let distribution = series.into_distribution().unwrap();

        assert_eq!(distribution.sample_count(), 20);
        assert_eq!(distribution.median(), 10);
        assert_eq!(distribution.p95(), 19);
        assert_eq!(distribution.p99(), 20);
        assert_eq!(distribution.worst(), 20);
    }

    #[test]
    fn a_series_rejects_samples_beyond_its_campaign_bound() {
        let mut series = MeasurementSeries::new(2).unwrap();
        series.record(7).unwrap();
        series.record(9).unwrap();

        assert_eq!(
            series.record(11),
            Err(MeasurementError::SampleLimitExceeded { maximum: 2 })
        );
        assert_eq!(series.len(), 2);
    }

    #[test]
    fn budget_evidence_requires_every_reference_app_and_target_class() {
        let mut campaign = BaselineCampaign::new(5).unwrap();
        campaign
            .record(BaselineRun::new(
                PerformanceMetric::SafePointCommitDuration,
                ReferenceApp::Small,
                TargetClass::Desktop,
                "Apple M4 Pro",
                "rustc 1.90.0",
                "aarch64-apple-darwin debug",
                100,
            )
            .unwrap())
            .unwrap();
        campaign
            .record(BaselineRun::new(
                PerformanceMetric::SafePointCommitDuration,
                ReferenceApp::Stateful,
                TargetClass::Desktop,
                "Apple M4 Pro",
                "rustc 1.90.0",
                "aarch64-apple-darwin debug",
                100,
            )
            .unwrap())
            .unwrap();

        let error = campaign.finish().unwrap_err();

        assert_eq!(
            error,
            BaselineError::IncompleteCoverage(BaselineCoverage {
                small_mobile: false,
                small_desktop: true,
                stateful_mobile: false,
                stateful_desktop: true,
                near_limit_mobile: false,
                near_limit_desktop: false,
            })
        );
    }

    #[test]
    fn an_approved_budget_reports_warning_and_hard_failure_from_its_named_statistic() {
        let evidence = Arc::new(complete_evidence(
            PerformanceMetric::SafePointCommitDuration,
        ));
        let budget = PerformanceBudget::new(
            PerformanceMetric::SafePointCommitDuration,
            BudgetStatistic::P99,
            BudgetDirection::AtMost,
            10,
            20,
            "keeps the event loop responsive on the measured device floor",
            "2026-08-19",
            evidence,
        )
        .unwrap();

        assert_eq!(
            budget.evaluate(MeasurementDistribution {
                sample_count: 100,
                median: 5,
                p95: 8,
                p99: 15,
                worst: 22,
            }),
            BudgetOutcome::Warning { observed: 15 }
        );
        assert_eq!(
            budget.evaluate(MeasurementDistribution {
                sample_count: 100,
                median: 5,
                p95: 8,
                p99: 21,
                worst: 30,
            }),
            BudgetOutcome::HardFailure { observed: 21 }
        );
    }

    #[test]
    fn a_budget_rejects_complete_evidence_for_another_metric() {
        let evidence = Arc::new(complete_evidence(
            PerformanceMetric::SafePointCommitDuration,
        ));

        let error = PerformanceBudget::new(
            PerformanceMetric::UploadThroughput,
            BudgetStatistic::P95,
            BudgetDirection::AtLeast,
            20,
            10,
            "measured upload floor",
            "2026-08-19",
            evidence,
        )
        .unwrap_err();

        assert_eq!(
            error,
            BudgetError::MetricMismatch {
                budget: PerformanceMetric::UploadThroughput,
                evidence: PerformanceMetric::SafePointCommitDuration,
            }
        );
    }

    fn complete_evidence(metric: PerformanceMetric) -> BaselineEvidence {
        let mut campaign = BaselineCampaign::new(6).unwrap();
        for (application, target_class, hardware) in [
            (
                ReferenceApp::Small,
                TargetClass::LowResourceMobile,
                "iPhone device floor",
            ),
            (
                ReferenceApp::Small,
                TargetClass::Desktop,
                "Apple M4 Pro",
            ),
            (
                ReferenceApp::Stateful,
                TargetClass::LowResourceMobile,
                "iPhone device floor",
            ),
            (
                ReferenceApp::Stateful,
                TargetClass::Desktop,
                "Apple M4 Pro",
            ),
            (
                ReferenceApp::NearLimit,
                TargetClass::LowResourceMobile,
                "iPhone device floor",
            ),
            (
                ReferenceApp::NearLimit,
                TargetClass::Desktop,
                "Apple M4 Pro",
            ),
        ] {
            campaign
                .record(
                    BaselineRun::new(
                        metric,
                        application,
                        target_class,
                        hardware,
                        "rustc 1.90.0",
                        "pinned debug profile",
                        100,
                    )
                    .unwrap(),
                )
                .unwrap();
        }
        campaign.finish().unwrap()
    }
}
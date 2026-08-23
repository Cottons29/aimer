use std::fmt;
use std::time::{Duration, Instant};

/// Stable identity assigned to a stage while it is retained by a [`StageBook`].
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct StageId(u64);

impl StageId {
    /// Return the numeric representation of this stage identity.
    #[inline]
    pub const fn raw(self) -> u64 {
        self.0
    }
}

/// The semantic kind of work represented by a stage.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum StageKind {
    /// Resolve and compile the application.
    Compile,
    /// Assemble the compiled application into its launchable form.
    Assemble,
    /// Package the application for its target.
    Package,
    /// Launch the assembled application.
    Launch,
    /// The long-running application session.
    Application,
    /// Rebuild and apply a hot reload.
    HotReload,
    /// A stage kind introduced by a target or a future orchestration path.
    Custom(String),
    /// A free-form stage label supplied by an adapter.
    Other(String),
}

impl StageKind {
    /// Return the compact label shown in a stage summary.
    pub fn label(&self) -> &str {
        match self {
            Self::Compile => "Compile",
            Self::Assemble => "Assemble",
            Self::Package => "Package",
            Self::Launch => "Launch",
            Self::Application => "Application",
            Self::HotReload => "Hot reload",
            Self::Custom(label) | Self::Other(label) => label,
        }
    }
}

/// The lifecycle state of a stage.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StageStatus {
    /// Work is still in progress.
    Running,
    /// Work completed successfully.
    Succeeded,
    /// Work completed with an error.
    Failed,
    /// Work was stopped before it completed.
    Cancelled,
}

/// Start and end timestamps retained for a stage.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StageTiming {
    started_at: Instant,
    finished_at: Option<Instant>,
}

impl StageTiming {
    /// Create timing metadata for work beginning at `started_at`.
    #[inline]
    pub const fn new(started_at: Instant) -> Self {
        Self {
            started_at,
            finished_at: None,
        }
    }

    /// The timestamp at which the stage began.
    #[inline]
    pub const fn started_at(self) -> Instant {
        self.started_at
    }

    /// The timestamp at which the stage reached a terminal state, if it has.
    #[inline]
    pub const fn finished_at(self) -> Option<Instant> {
        self.finished_at
    }

    /// The elapsed time as of `at`, clamped to zero before the start time.
    #[inline]
    pub fn elapsed_at(self, at: Instant) -> Duration {
        at.saturating_duration_since(self.started_at)
    }

    /// Return the completed duration, or `None` while the stage is running.
    #[inline]
    pub fn elapsed(self) -> Option<Duration> {
        self.finished_at
            .map(|finished_at| finished_at.saturating_duration_since(self.started_at))
    }
}

/// A retained detail entry. The text is kept verbatim so renderers can
/// preserve ANSI styling and structured blocks supplied by an adapter.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DetailEntry {
    text: String,
}

impl DetailEntry {
    /// Return the original detail text, including ANSI escapes and newlines.
    #[inline]
    pub fn as_str(&self) -> &str {
        &self.text
    }

    /// Count the logical lines in this detail entry.
    #[inline]
    pub fn line_count(&self) -> usize {
        self.text.lines().count().max(1)
    }
}

/// One retained execution stage.
pub struct Stage {
    id: StageId,
    kind: StageKind,
    parent: Option<StageId>,
    status: StageStatus,
    timing: StageTiming,
    progress: Option<StageProgress>,
    details: Vec<DetailEntry>,
    expanded: bool,
}

/// Optional progress metadata for a running stage.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StageProgress {
    current: u64,
    total: Option<u64>,
}

impl StageProgress {
    /// Create progress with an optional determinate total.
    #[inline]
    pub const fn new(current: u64, total: Option<u64>) -> Self {
        Self { current, total }
    }

    /// The current progress value.
    #[inline]
    pub const fn current(self) -> u64 {
        self.current
    }

    /// The determinate total, when one is available.
    #[inline]
    pub const fn total(self) -> Option<u64> {
        self.total
    }

    /// Return a bounded percentage for determinate progress.
    #[inline]
    pub fn percentage(self) -> Option<u8> {
        let total = self.total?;
        if total == 0 {
            return Some(0);
        }
        Some(
            ((u128::from(self.current.min(total)) * 100) / u128::from(total)) as u8,
        )
    }
}

/// Failure returned when a stage operation cannot be applied.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StageError {
    /// The requested stage identity is not retained.
    UnknownStage(StageId),
    /// The requested stage has already reached a terminal state.
    AlreadyFinished(StageId),
}

impl fmt::Display for StageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownStage(id) => write!(f, "unknown stage {}", id.raw()),
            Self::AlreadyFinished(id) => write!(f, "stage {} is already finished", id.raw()),
        }
    }
}

impl std::error::Error for StageError {}

/// Collection of retained stages and their current selection.
pub struct StageBook {
    stages: Vec<Stage>,
    next_id: u64,
    active: Option<StageId>,
    selected: Option<StageId>,
}

impl StageBook {
    /// Create an empty stage collection.
    #[inline]
    pub const fn new() -> Self {
        Self {
            stages: Vec::new(),
            next_id: 1,
            active: None,
            selected: None,
        }
    }

    /// Start a collapsed stage at `started_at`, returning its stable identity.
    pub fn start(&mut self, kind: StageKind, started_at: Instant) -> StageId {
        let id = StageId(self.next_id);
        self.next_id += 1;
        let parent = self.active;
        self.stages.push(Stage {
            id,
            kind,
            parent,
            status: StageStatus::Running,
            timing: StageTiming::new(started_at),
            progress: None,
            details: Vec::new(),
            expanded: false,
        });
        self.active = Some(id);
        self.selected = Some(id);
        id
    }

    /// Find a retained stage by identity.
    #[inline]
    pub fn stage(&self, id: StageId) -> Option<&Stage> {
        self.stages.iter().find(|stage| stage.id == id)
    }

    /// Return every retained stage in creation order.
    #[inline]
    pub fn stages(&self) -> &[Stage] {
        &self.stages
    }

    /// Return a compact, owned view of one stage for a renderer.
    pub fn snapshot(&self, id: StageId) -> Option<StageSnapshot> {
        let stage = self.stage(id)?;
        Some(StageSnapshot {
            id: stage.id,
            kind: stage.kind.clone(),
            status: stage.status,
            progress: stage.progress,
            details: stage
                .details
                .iter()
                .map(|detail| detail.text.clone())
                .collect(),
            expanded: stage.expanded,
        })
    }

    /// The currently active stage, if any.
    #[inline]
    pub const fn active(&self) -> Option<StageId> {
        self.active
    }

    /// The currently selected stage, if any.
    #[inline]
    pub const fn selected(&self) -> Option<StageId> {
        self.selected
    }

    /// Append one retained detail entry to a running stage.
    pub fn append_detail(
        &mut self,
        id: StageId,
        text: impl Into<String>,
    ) -> Result<(), StageError> {
        let stage = self.stage_mut(id)?;
        stage.details.push(DetailEntry { text: text.into() });
        Ok(())
    }

    /// Update determinate or indeterminate progress on a running stage.
    pub fn set_progress(
        &mut self,
        id: StageId,
        progress: Option<StageProgress>,
    ) -> Result<(), StageError> {
        let stage = self.stage_mut(id)?;
        if stage.status != StageStatus::Running {
            return Err(StageError::AlreadyFinished(id));
        }
        stage.progress = progress;
        Ok(())
    }

    /// Mark a stage successful and restore its parent as active when present.
    pub fn finish(&mut self, id: StageId, at: Instant) -> Result<(), StageError> {
        self.close(id, StageStatus::Succeeded, at, false)
    }

    /// Mark a stage failed, expand it, and select it for immediate inspection.
    pub fn fail(&mut self, id: StageId, at: Instant) -> Result<(), StageError> {
        self.close(id, StageStatus::Failed, at, true)
    }

    /// Mark a stage cancelled and restore its parent as active when present.
    pub fn cancel(&mut self, id: StageId, at: Instant) -> Result<(), StageError> {
        self.close(id, StageStatus::Cancelled, at, false)
    }

    /// Select a retained stage by identity.
    pub fn select(&mut self, id: StageId) -> Result<(), StageError> {
        if self.stage(id).is_none() {
            return Err(StageError::UnknownStage(id));
        }
        self.selected = Some(id);
        Ok(())
    }

    /// Select the next stage, stopping at the newest stage rather than wrapping.
    pub fn select_next(&mut self) -> Option<StageId> {
        self.select_relative(1)
    }

    /// Select the previous stage, stopping at the oldest stage rather than wrapping.
    pub fn select_previous(&mut self) -> Option<StageId> {
        self.select_relative(-1)
    }

    /// Toggle the selected stage and return its new expansion state.
    pub fn toggle_selected(&mut self) -> Option<bool> {
        let id = self.selected?;
        let stage = self.stage_mut(id).ok()?;
        stage.expanded = !stage.expanded;
        Some(stage.expanded)
    }

    /// Expand every retained stage.
    pub fn expand_all(&mut self) {
        for stage in &mut self.stages {
            stage.expanded = true;
        }
    }

    /// Collapse every retained stage.
    pub fn collapse_all(&mut self) {
        for stage in &mut self.stages {
            stage.expanded = false;
        }
    }

    fn stage_mut(&mut self, id: StageId) -> Result<&mut Stage, StageError> {
        self.stages
            .iter_mut()
            .find(|stage| stage.id == id)
            .ok_or(StageError::UnknownStage(id))
    }

    fn close(
        &mut self,
        id: StageId,
        status: StageStatus,
        at: Instant,
        expand_and_select: bool,
    ) -> Result<(), StageError> {
        let parent = {
            let stage = self.stage_mut(id)?;
            if stage.status != StageStatus::Running {
                return Err(StageError::AlreadyFinished(id));
            }
            stage.status = status;
            stage.timing.finish_at(at);
            if expand_and_select {
                stage.expanded = true;
            }
            stage.parent
        };

        if expand_and_select {
            self.selected = Some(id);
        }
        if self.active == Some(id) {
            self.active = parent.filter(|parent_id| {
                self.stage(*parent_id)
                    .is_some_and(|stage| stage.status == StageStatus::Running)
            });
        }
        Ok(())
    }

    fn select_relative(&mut self, offset: isize) -> Option<StageId> {
        if self.stages.is_empty() {
            self.selected = None;
            return None;
        }
        let current = self
            .selected
            .and_then(|id| self.stages.iter().position(|stage| stage.id == id))
            .unwrap_or(0);
        let next = if offset.is_negative() {
            current.saturating_sub(offset.unsigned_abs())
        } else {
            current.saturating_add(offset as usize).min(self.stages.len() - 1)
        };
        let id = self.stages[next].id;
        self.selected = Some(id);
        Some(id)
    }
}

/// An owned, renderer-friendly snapshot of one stage.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StageSnapshot {
    /// Stable stage identity.
    pub id: StageId,
    /// Semantic stage kind.
    pub kind: StageKind,
    /// Current lifecycle state.
    pub status: StageStatus,
    /// Current progress, if any.
    pub progress: Option<StageProgress>,
    /// Detail text in insertion order.
    pub details: Vec<String>,
    /// Whether details should be shown.
    pub expanded: bool,
}

impl Default for StageBook {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

impl Stage {
    /// The stable identity of this stage.
    #[inline]
    pub const fn id(&self) -> StageId {
        self.id
    }

    /// The semantic kind of this stage.
    #[inline]
    pub fn kind(&self) -> &StageKind {
        &self.kind
    }

    /// The parent stage when this stage was started while another stage was
    /// active.
    #[inline]
    pub const fn parent(&self) -> Option<StageId> {
        self.parent
    }

    /// The current lifecycle state.
    #[inline]
    pub const fn status(&self) -> StageStatus {
        self.status
    }

    /// The retained timing metadata.
    #[inline]
    pub const fn timing(&self) -> StageTiming {
        self.timing
    }

    /// The current progress metadata, if supplied by the producer.
    #[inline]
    pub const fn progress(&self) -> Option<StageProgress> {
        self.progress
    }

    /// The detail entries retained for this stage.
    #[inline]
    pub fn details(&self) -> &[DetailEntry] {
        &self.details
    }

    /// Whether the renderer should show this stage's details.
    #[inline]
    pub const fn is_expanded(&self) -> bool {
        self.expanded
    }
}

impl StageTiming {
    #[inline]
    fn finish_at(&mut self, finished_at: Instant) {
        self.finished_at = Some(finished_at);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn starting_a_stage_records_identity_kind_and_running_state() {
        let started_at = Instant::now();
        let mut book = StageBook::new();

        let id = book.start(StageKind::Compile, started_at);
        let stage = book.stage(id).expect("started stage should be retained");

        assert_eq!(stage.id(), id);
        assert_eq!(stage.kind(), &StageKind::Compile);
        assert_eq!(stage.status(), StageStatus::Running);
        assert!(!stage.is_expanded());
        assert_eq!(stage.timing().started_at(), started_at);
        assert_eq!(stage.timing().finished_at(), None);
        assert_eq!(stage.progress(), None);
        assert!(stage.details().is_empty());
        assert_eq!(book.active(), Some(id));
        assert_eq!(book.selected(), Some(id));
    }

    #[test]
    fn detail_and_progress_are_retained_verbatim() {
        let mut book = StageBook::new();
        let id = book.start(StageKind::Compile, Instant::now());
        let detail = "\x1b[31mcompiler output\x1b[0m\nsecond line";

        book.append_detail(id, detail).unwrap();
        book.set_progress(id, Some(StageProgress::new(3, Some(10))))
            .unwrap();

        let stage = book.stage(id).unwrap();
        assert_eq!(stage.details()[0].as_str(), detail);
        assert_eq!(stage.details()[0].line_count(), 2);
        assert_eq!(stage.progress().unwrap().current(), 3);
        assert_eq!(stage.progress().unwrap().total(), Some(10));
        assert_eq!(stage.progress().unwrap().percentage(), Some(30));
    }

    #[test]
    fn finishing_a_stage_records_terminal_state_and_elapsed_time() {
        let started_at = Instant::now();
        let ended_at = started_at + Duration::from_secs(2);
        let mut book = StageBook::new();
        let id = book.start(StageKind::Assemble, started_at);

        book.finish(id, ended_at).unwrap();

        let stage = book.stage(id).unwrap();
        assert_eq!(stage.status(), StageStatus::Succeeded);
        assert_eq!(stage.timing().finished_at(), Some(ended_at));
        assert_eq!(stage.timing().elapsed(), Some(Duration::from_secs(2)));
        assert_eq!(book.active(), None);
    }

    #[test]
    fn failure_expands_and_selects_the_failed_stage() {
        let now = Instant::now();
        let mut book = StageBook::new();
        let compile = book.start(StageKind::Compile, now);
        book.finish(compile, now + Duration::from_millis(1)).unwrap();
        let launch = book.start(StageKind::Launch, now + Duration::from_millis(2));

        book.select(compile).unwrap();
        book.fail(launch, now + Duration::from_millis(3)).unwrap();

        let stage = book.stage(launch).unwrap();
        assert_eq!(stage.status(), StageStatus::Failed);
        assert!(stage.is_expanded());
        assert_eq!(book.selected(), Some(launch));
    }

    #[test]
    fn cancelling_nested_stages_restores_the_parent_active_stage() {
        let now = Instant::now();
        let mut book = StageBook::new();
        let parent = book.start(StageKind::Application, now);
        let child = book.start(StageKind::HotReload, now + Duration::from_millis(1));

        assert_eq!(book.stage(child).unwrap().parent(), Some(parent));
        book.cancel(child, now + Duration::from_millis(2)).unwrap();
        assert_eq!(book.active(), Some(parent));

        book.cancel(parent, now + Duration::from_millis(3)).unwrap();
        assert_eq!(book.active(), None);
    }

    #[test]
    fn repeated_stages_keep_distinct_ids_and_selection_moves_without_wrapping() {
        let now = Instant::now();
        let mut book = StageBook::new();
        let first = book.start(StageKind::Compile, now);
        book.finish(first, now + Duration::from_millis(1)).unwrap();
        let second = book.start(StageKind::Compile, now + Duration::from_millis(2));

        assert_ne!(first, second);
        book.select(first).unwrap();
        assert_eq!(book.select_next(), Some(second));
        assert_eq!(book.select_next(), Some(second));
        assert_eq!(book.select_previous(), Some(first));
        assert_eq!(book.select_previous(), Some(first));
    }

    #[test]
    fn selected_stage_can_toggle_and_be_read_as_a_render_snapshot() {
        let mut book = StageBook::new();
        let id = book.start(StageKind::Other("index assets".to_string()), Instant::now());

        assert_eq!(book.toggle_selected(), Some(true));
        let snapshot = book.snapshot(id).unwrap();
        assert_eq!(snapshot.id, id);
        assert_eq!(snapshot.kind.label(), "index assets");
        assert!(snapshot.expanded);
        assert!(snapshot.details.is_empty());
    }
}

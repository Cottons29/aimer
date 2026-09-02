use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// The effect one filesystem notification may have on the running app.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum ChangeImpact {
    /// The notification is unrelated or generated output.
    Ignored,
    /// Portable guest code must be rebuilt.
    RebuildGuest,
    /// Permanent host/provider code changed and the native app must restart.
    RestartNativeHost,
}

/// Resolved source roots used to classify raw filesystem notifications.
#[derive(Clone, Debug)]
pub struct WatchSet {
    guest_roots: Vec<PathBuf>,
    native_roots: Vec<PathBuf>,
    ignored_roots: Vec<PathBuf>,
    watch_roots: Vec<PathBuf>,
}

impl WatchSet {
    /// Creates a watch set from Cargo-resolved portable, native, and output roots.
    pub fn new(
        guest_roots: Vec<PathBuf>,
        native_roots: Vec<PathBuf>,
        ignored_roots: Vec<PathBuf>,
    ) -> Self {
        let mut watch_roots = guest_roots.clone();
        watch_roots.extend(native_roots.iter().cloned());
        watch_roots.retain(|root| {
            !ignored_roots.iter().any(|ignored| root.starts_with(ignored))
        });
        watch_roots.sort();
        watch_roots.dedup();
        Self {
            guest_roots,
            native_roots,
            ignored_roots,
            watch_roots,
        }
    }

    /// Creates the default classification for an automatically extracted guest.
    pub fn automatic(
        project_root: PathBuf,
        portable_source_root: PathBuf,
        path_dependencies: Vec<PathBuf>,
        ignored_roots: Vec<PathBuf>,
    ) -> Self {
        let mut guest_roots = vec![
            portable_source_root,
            project_root.join("assets"),
            project_root.join("Cargo.toml"),
            project_root.join("Aimer.toml"),
        ];
        guest_roots.extend(path_dependencies);
        let mut set = Self::new(guest_roots, vec![project_root.clone()], ignored_roots);
        set.watch_roots.retain(|root| root != &project_root);
        if let Ok(entries) = std::fs::read_dir(&project_root) {
            for entry in entries.flatten() {
                let root = entry.path();
                if !entry.file_name().to_string_lossy().starts_with('.')
                    && !set.ignored_roots.iter().any(|ignored| root.starts_with(ignored))
                    && !set.watch_roots.iter().any(|watched| {
                        root.starts_with(watched) || watched.starts_with(&root)
                    })
                {
                    set.watch_roots.push(root);
                }
            }
            set.watch_roots.sort();
            set.watch_roots.dedup();
        }
        set
    }

    /// Returns source roots that may safely be registered with the filesystem watcher.
    #[inline]
    pub fn watch_roots(&self) -> &[PathBuf] {
        &self.watch_roots
    }

    /// Classifies a complete notification, including both sides of a rename.
    pub fn classify<'a>(&self, paths: impl IntoIterator<Item = &'a Path>) -> ChangeImpact {
        paths
            .into_iter()
            .map(|path| self.classify_path(path))
            .max()
            .unwrap_or(ChangeImpact::Ignored)
    }

    fn classify_path(&self, path: &Path) -> ChangeImpact {
        if is_temporary(path) || self.ignored_roots.iter().any(|root| path.starts_with(root)) {
            return ChangeImpact::Ignored;
        }
        if self.guest_roots.iter().any(|root| path.starts_with(root)) {
            return ChangeImpact::RebuildGuest;
        }
        if self.native_roots.iter().any(|root| path.starts_with(root)) {
            return ChangeImpact::RestartNativeHost;
        }
        ChangeImpact::Ignored
    }
}

fn is_temporary(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
        return true;
    };
    name.starts_with(".#")
        || name.ends_with('~')
        || matches!(
            path.extension().and_then(|extension| extension.to_str()),
            Some("swp" | "swx" | "tmp" | "temp" | "part")
        )
}

/// Collapses a burst of filesystem notifications into one build request.
///
/// The coalescer is trailing: a build becomes due only after the source tree
/// has been quiet for the configured window, so an editor that writes several
/// files in one save never queues several builds. Time is supplied by the
/// caller instead of read from the clock, which keeps the watcher deterministic
/// and testable outside the notification thread.
#[derive(Clone, Copy, Debug)]
pub struct ChangeCoalescer {
    window: Duration,
    quiet_at: Option<Instant>,
}

impl ChangeCoalescer {
    /// Creates a coalescer that waits `window` after the last notification.
    #[inline]
    pub const fn new(window: Duration) -> Self {
        Self {
            window,
            quiet_at: None,
        }
    }

    /// Records one relevant notification observed at `at`.
    #[inline]
    pub fn notify(&mut self, at: Instant) {
        self.quiet_at = Some(at + self.window);
    }

    /// Returns the instant at which the pending burst becomes buildable.
    #[inline]
    pub const fn due_at(&self) -> Option<Instant> {
        self.quiet_at
    }

    /// Consumes the pending burst when the quiet window has elapsed.
    pub fn take_due(&mut self, now: Instant) -> bool {
        match self.quiet_at {
            Some(quiet_at) if now >= quiet_at => {
                self.quiet_at = None;
                true
            }
            _ => false,
        }
    }
}

/// The externally visible phase of one deterministic guest rebuild.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WatchPhase {
    /// No relevant change is waiting.
    Idle,
    /// One guest build owns the compiler slot.
    Building,
    /// A validated artifact is waiting to upload.
    ReadyToPush,
    /// The authenticated transfer is in progress.
    Uploading,
    /// The app accepted the artifact and owns the terminal outcome.
    WaitingForResult,
}

/// Work requested by a watcher or build completion.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WatchAction {
    /// No new work may start.
    None,
    /// Start exactly one guest build.
    StartBuild,
    /// Push the successful build artifact.
    PushArtifact,
}

/// A deterministic one-build state machine independent of filesystem and UI.
#[derive(Debug, Default)]
pub struct WatchStateMachine {
    phase: WatchPhase,
    dirty: bool,
}

impl Default for WatchPhase {
    fn default() -> Self {
        Self::Idle
    }
}

impl WatchStateMachine {
    /// Creates an idle watcher with no pending source changes.
    #[inline]
    pub const fn new() -> Self {
        Self {
            phase: WatchPhase::Idle,
            dirty: false,
        }
    }

    /// Returns the current externally visible phase.
    #[inline]
    pub const fn phase(&self) -> WatchPhase {
        self.phase
    }

    /// Records one relevant source-change notification.
    pub fn source_changed(&mut self) -> WatchAction {
        match self.phase {
            WatchPhase::Idle => {
                self.phase = WatchPhase::Building;
                WatchAction::StartBuild
            }
            WatchPhase::Building
            | WatchPhase::ReadyToPush
            | WatchPhase::Uploading
            | WatchPhase::WaitingForResult => {
                self.dirty = true;
                WatchAction::None
            }
        }
    }

    /// Completes the active build while preserving the running app on failure.
    pub fn build_finished(&mut self, succeeded: bool) -> WatchAction {
        if self.phase != WatchPhase::Building {
            return WatchAction::None;
        }
        if succeeded {
            self.phase = WatchPhase::ReadyToPush;
            return WatchAction::PushArtifact;
        }
        if std::mem::take(&mut self.dirty) {
            WatchAction::StartBuild
        } else {
            self.phase = WatchPhase::Idle;
            WatchAction::None
        }
    }

    /// Marks the validated artifact as being transferred.
    pub fn upload_started(&mut self) -> WatchAction {
        if self.phase == WatchPhase::ReadyToPush {
            self.phase = WatchPhase::Uploading;
        }
        WatchAction::None
    }

    /// Marks the upload as accepted by the runtime staging queue.
    pub fn upload_accepted(&mut self) -> WatchAction {
        if self.phase == WatchPhase::Uploading {
            self.phase = WatchPhase::WaitingForResult;
        }
        WatchAction::None
    }

    /// Records any terminal commit, rejection, or cancellation.
    pub fn terminal_result(&mut self) -> WatchAction {
        if self.phase != WatchPhase::WaitingForResult {
            return WatchAction::None;
        }
        if std::mem::take(&mut self.dirty) {
            self.phase = WatchPhase::Building;
            WatchAction::StartBuild
        } else {
            self.phase = WatchPhase::Idle;
            WatchAction::None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn watch_set_ignores_outputs_and_temporaries_but_escalates_native_changes() {
        let watch_set = WatchSet::new(
            vec![PathBuf::from("/app/src"), PathBuf::from("/portable/src")],
            vec![PathBuf::from("/provider/src")],
            vec![PathBuf::from("/app/target"), PathBuf::from("/app/builds")],
        );

        assert_eq!(
            watch_set.classify([
                Path::new("/app/src/page.rs"),
                Path::new("/app/src/page.rs"),
            ]),
            ChangeImpact::RebuildGuest
        );
        assert_eq!(
            watch_set.classify([
                Path::new("/app/src/.page.rs.swp"),
                Path::new("/app/target/wasm32-unknown-unknown/app.wasm"),
                Path::new("/app/builds/macos/App.app"),
            ]),
            ChangeImpact::Ignored
        );
        assert_eq!(
            watch_set.classify([
                Path::new("/app/src/old.rs"),
                Path::new("/provider/src/native_sdk.rs"),
            ]),
            ChangeImpact::RestartNativeHost
        );
    }

    #[test]
    fn automatic_watch_set_watches_portable_inputs_without_subscribing_to_outputs() {
        let watch_set = WatchSet::automatic(
            PathBuf::from("/app"),
            PathBuf::from("/app/src"),
            vec![PathBuf::from("/shared/widgets")],
            vec![PathBuf::from("/app/target"), PathBuf::from("/app/builds")],
        );

        assert_eq!(watch_set.classify([Path::new("/app/src/page.rs")]), ChangeImpact::RebuildGuest);
        assert_eq!(watch_set.classify([Path::new("/app/assets/logo.png")]), ChangeImpact::RebuildGuest);
        assert_eq!(watch_set.classify([Path::new("/shared/widgets/src/card.rs")]), ChangeImpact::RebuildGuest);
        assert_eq!(watch_set.classify([Path::new("/app/native/provider.rs")]), ChangeImpact::RestartNativeHost);
        assert_eq!(watch_set.classify([Path::new("/app/target/aimer-hot-reload/application/src/main.rs")]), ChangeImpact::Ignored);
        assert_eq!(
            watch_set.watch_roots(),
            [
                PathBuf::from("/app/Aimer.toml"),
                PathBuf::from("/app/Cargo.toml"),
                PathBuf::from("/app/assets"),
                PathBuf::from("/app/src"),
                PathBuf::from("/shared/widgets"),
            ]
        );
        assert!(watch_set.watch_roots().iter().all(|root| !root.starts_with("/app/target")));
    }

    #[test]
    fn a_burst_of_notifications_becomes_one_build_after_the_quiet_window() {
        let window = Duration::from_millis(50);
        let start = Instant::now();
        let mut coalescer = ChangeCoalescer::new(window);

        assert!(!coalescer.take_due(start));
        for offset in [0, 5, 10, 20] {
            coalescer.notify(start + Duration::from_millis(offset));
        }

        assert_eq!(coalescer.due_at(), Some(start + Duration::from_millis(70)));
        assert!(!coalescer.take_due(start + Duration::from_millis(69)));
        assert!(coalescer.take_due(start + Duration::from_millis(70)));
        assert_eq!(coalescer.due_at(), None);
        assert!(!coalescer.take_due(start + Duration::from_secs(1)));
    }

    #[test]
    fn burst_during_build_never_overlaps_and_starts_one_follow_up() {
        let mut watcher = WatchStateMachine::new();

        assert_eq!(watcher.source_changed(), WatchAction::StartBuild);
        assert_eq!(watcher.phase(), WatchPhase::Building);
        assert_eq!(watcher.source_changed(), WatchAction::None);
        assert_eq!(watcher.source_changed(), WatchAction::None);
        assert_eq!(watcher.build_finished(true), WatchAction::PushArtifact);
        assert_eq!(watcher.upload_started(), WatchAction::None);
        assert_eq!(watcher.upload_accepted(), WatchAction::None);
        assert_eq!(watcher.terminal_result(), WatchAction::StartBuild);
        assert_eq!(watcher.phase(), WatchPhase::Building);
        assert_eq!(watcher.build_finished(false), WatchAction::None);
        assert_eq!(watcher.phase(), WatchPhase::Idle);
    }

    #[test]
    fn compile_failure_and_rejection_keep_accepting_later_edits() {
        let mut watcher = WatchStateMachine::new();

        assert_eq!(watcher.source_changed(), WatchAction::StartBuild);
        assert_eq!(watcher.build_finished(false), WatchAction::None);
        assert_eq!(watcher.phase(), WatchPhase::Idle);

        assert_eq!(watcher.source_changed(), WatchAction::StartBuild);
        assert_eq!(watcher.build_finished(true), WatchAction::PushArtifact);
        assert_eq!(watcher.phase(), WatchPhase::ReadyToPush);
        assert_eq!(watcher.upload_started(), WatchAction::None);
        assert_eq!(watcher.phase(), WatchPhase::Uploading);
        assert_eq!(watcher.upload_accepted(), WatchAction::None);
        assert_eq!(watcher.phase(), WatchPhase::WaitingForResult);
        assert_eq!(watcher.terminal_result(), WatchAction::None);
        assert_eq!(watcher.phase(), WatchPhase::Idle);
        assert_eq!(watcher.source_changed(), WatchAction::StartBuild);
    }

    #[cfg(feature = "hot-reload")]
    #[test]
    fn repeated_compile_failure_and_cancelled_reload_cycles_return_to_idle() {
        const CYCLES: usize = 10_000;

        let mut watcher = WatchStateMachine::new();
        let mut started_builds = 0_usize;

        for cycle in 0..CYCLES {
            assert_eq!(watcher.source_changed(), WatchAction::StartBuild);
            started_builds += 1;
            if cycle % 4 == 0 {
                assert_eq!(watcher.build_finished(false), WatchAction::None);
                assert_eq!(watcher.phase(), WatchPhase::Idle);
                continue;
            }

            assert_eq!(watcher.build_finished(true), WatchAction::PushArtifact);
            assert_eq!(watcher.upload_started(), WatchAction::None);
            assert_eq!(watcher.upload_accepted(), WatchAction::None);
            if cycle % 4 == 1 {
                assert_eq!(watcher.source_changed(), WatchAction::None);
                assert_eq!(watcher.terminal_result(), WatchAction::StartBuild);
                started_builds += 1;
                assert_eq!(watcher.build_finished(false), WatchAction::None);
            } else {
                assert_eq!(watcher.terminal_result(), WatchAction::None);
            }
            assert_eq!(watcher.phase(), WatchPhase::Idle);
        }

        assert_eq!(started_builds, CYCLES + CYCLES / 4);
    }
}
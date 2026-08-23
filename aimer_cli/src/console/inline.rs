//! Inline terminal presentation for the experimental `-Z inline-render` mode.
//!
//! The runner still emits the normal [`RunnerEvent`] stream. This module only
//! changes how that stream is presented: completed stages become scrollback,
//! while the active stage and control navbar occupy a small managed region at
//! the bottom of the terminal.

use std::collections::HashMap;
use std::fs::File;
use std::io::{self, IsTerminal, Write, stdout};
use std::net::{IpAddr, Ipv4Addr};
use std::time::{Duration, Instant, SystemTime};

use aimer_inspector::InspectorServer;
use anyhow::Context;
use arboard::Clipboard;
use crossterm::event::Event;
use crossterm::terminal;
use tokio::runtime::Runtime;

use crate::commands::run::Device;
use crate::console::hotkeys::{ConsoleAction, ConsolePane, map_key_event};
use crate::console::stage::{StageBook, StageId, StageKind, StageProgress, StageStatus};
use crate::console::state::{AppState, RunnerEvent, Status};
use crate::console::input;
use crate::targets::Targets;
use crate::tui::RawModeGuard;

const SPINNERS: [&str; 10] = ["⠋", "⠙", "⠚", "⠞", "⠖", "⠦", "⠴", "⠲", "⠳", "⠓"];
#[cfg(test)]
const APPLICATION_TAIL_LINES: usize = 3;
#[cfg(test)]
const MAX_EXPANDED_LINES: usize = 200;

/// State shared by the inline renderer and its input handler.
pub struct InlineSession {
    /// Target label shown in the session header.
    pub target: String,
    /// Existing application/build log state and formatting behaviour.
    pub state: AppState,
    /// Retained stage summaries and expansion state.
    pub stages: StageBook,
    /// Current spinner frame.
    pub frame: usize,
    hot_reload_enabled: bool,
    notice: Option<String>,
}

impl InlineSession {
    /// Create an empty inline session for `device`.
    pub fn new(device: &Device) -> Self {
        Self::new_with_hot_reload(device, false)
    }

    /// Create an inline session with explicit hot-reload capability.
    pub fn new_with_hot_reload(device: &Device, hot_reload_enabled: bool) -> Self {
        Self {
            target: device.to_string(),
            state: AppState::new(),
            stages: StageBook::new(),
            frame: 0,
            hot_reload_enabled,
            notice: None,
        }
    }

    /// Consume one existing runner event without performing terminal I/O.
    pub fn apply_event(&mut self, event: RunnerEvent) {
        match event {
            RunnerEvent::BuildLog(message) => self.push_build_detail(message),
            RunnerEvent::BuildReport(report) => {
                let lines = report.lines();
                self.state.push_build_report(report);
                let stage = self.ensure_detail_stage(StageKind::Compile);
                for line in lines {
                    let _ = self.stages.append_detail(stage, line);
                }
            }
            RunnerEvent::AppLog(log) => {
                let rendered = log.render(self.state.show_log_source);
                self.state.push_app_log(log);
                let stage = self.ensure_detail_stage(StageKind::Application);
                let _ = self.stages.append_detail(stage, rendered);
            }
            RunnerEvent::AppPanic(report) => {
                let lines = report.lines();
                self.state.push_app_panic(report);
                let stage = self.ensure_detail_stage(StageKind::Application);
                for line in lines {
                    let _ = self.stages.append_detail(stage, line);
                }
            }
            RunnerEvent::StatusChange(status) => self.apply_status(status),
            RunnerEvent::HotReload if self.hot_reload_enabled => {
                self.notice("File change detected; rebuilding...".to_string());
                let stage = self.ensure_detail_stage(StageKind::HotReload);
                let _ = self.stages.append_detail(stage, "File change detected");
            }
            RunnerEvent::HotReload => {}
        }
    }

    /// Apply a lifecycle status and update the corresponding stage.
    pub fn apply_status(&mut self, status: Status) {
        self.state.apply_status(status.clone());
        let now = Instant::now();

        if status == Status::Error {
            if let Some(id) = self.stages.active() {
                let _ = self.stages.fail(id, now);
            } else {
                let id = self.stages.start(StageKind::Compile, now);
                let _ = self.stages.fail(id, now);
            }
            return;
        }

        if status == Status::Idling {
            if let Some(id) = self.stages.active()
                && self
                    .stages
                    .stage(id)
                    .is_some_and(|stage| stage.kind() == &StageKind::Application)
            {
                let _ = self.stages.finish(id, now);
            }
            return;
        }

        let kind = match status {
            Status::Locking | Status::Fetching(_) | Status::Compiling(_) => StageKind::Compile,
            Status::Building(_) => StageKind::Assemble,
            Status::Launching => StageKind::Launch,
            Status::Running => StageKind::Application,
            Status::Error | Status::Idling => return,
        };
        let id = self.ensure_stage(kind);
        let progress = match status {
            Status::Fetching(percent) | Status::Compiling(percent) | Status::Building(percent) => {
                Some(StageProgress::new(percent as u64, Some(100)))
            }
            Status::Locking | Status::Launching | Status::Running => None,
            Status::Error | Status::Idling => None,
        };
        let _ = self.stages.set_progress(id, progress);
    }

    /// Reset retained output before a deliberate hot restart.
    pub fn reset_for_restart(&mut self) {
        self.state.clear_build();
        self.state.clear_app();
        self.state.status = Status::Compiling(0);
        self.stages = StageBook::new();
        self.notice("Hot restart requested".to_string());
    }

    /// Add a one-line notice to the current application-visible transcript.
    pub fn notice(&mut self, notice: String) {
        self.notice = Some(notice);
    }

    fn push_build_detail(&mut self, message: String) {
        self.state.push_build_log(message.clone());
        let stage = self.ensure_detail_stage(self.build_kind());
        let _ = self.stages.append_detail(stage, message);
    }

    fn build_kind(&self) -> StageKind {
        match self.state.status {
            Status::Building(_) => StageKind::Assemble,
            Status::Launching => StageKind::Launch,
            _ => StageKind::Compile,
        }
    }

    fn ensure_detail_stage(&mut self, kind: StageKind) -> StageId {
        if let Some(id) = self.stages.active()
            && self
                .stages
                .stage(id)
                .is_some_and(|stage| stage.status() == StageStatus::Running && stage.kind() == &kind)
        {
            return id;
        }
        self.ensure_stage(kind)
    }

    fn ensure_stage(&mut self, kind: StageKind) -> StageId {
        if let Some(id) = self.stages.active() {
            let same_kind = self
                .stages
                .stage(id)
                .is_some_and(|stage| stage.status() == StageStatus::Running && stage.kind() == &kind);
            if same_kind {
                return id;
            }
            let _ = self.stages.finish(id, Instant::now());
        }
        self.stages.start(kind, Instant::now())
    }
}

/// An inline renderer that updates only the most recent managed output region.
pub struct InlineRenderer<W> {
    writer: W,
    committed_stages: Vec<StageId>,
    emitted_details: HashMap<StageId, usize>,
    live_region_active: bool,
    started: bool,
}

impl<W: Write> InlineRenderer<W> {
    /// Create a renderer writing to `writer`.
    #[inline]
    pub fn new(writer: W) -> Self {
        Self {
            writer,
            committed_stages: Vec::new(),
            emitted_details: HashMap::new(),
            live_region_active: false,
            started: false,
        }
    }

    /// Forget which stage summaries have already been emitted to scrollback.
    ///
    /// The managed region is still cleared on the next render. This is used
    /// when a hot restart replaces the session with a fresh stage sequence.
    pub fn reset(&mut self) {
        self.committed_stages.clear();
        self.emitted_details.clear();
    }

    /// Render one frame at the supplied terminal width.
    ///
    /// Completed summaries and newly visible details are appended as ordinary
    /// terminal lines. Only the two-line live region is rewritten, which keeps
    /// the transcript in scrollback and avoids full-screen redraws.
    pub fn render(&mut self, session: &InlineSession, width: usize) -> io::Result<()> {
        let width = width.max(1);
        self.clear_live_region()?;

        if !self.started {
            self.append_line(format!("◆ Aimer · {}", session.target), width)?;
            self.append_line(String::new(), width)?;
            self.started = true;
        }

        let now = Instant::now();
        let newly_committed: Vec<(StageId, String)> = session
            .stages
            .stages()
            .iter()
            .filter(|stage| {
                matches!(
                    stage.status(),
                    StageStatus::Succeeded | StageStatus::Failed | StageStatus::Cancelled
                ) && !self.committed_stages.contains(&stage.id())
            })
            .map(|stage| {
                (
                    stage.id(),
                    fit_line(render_stage_summary(stage, session.frame, now, false), width),
                )
            })
            .collect();
        for (id, summary) in newly_committed {
            self.append_line(summary, width)?;
            self.committed_stages.push(id);
        }

        self.append_expanded_details(session, width)?;

        let live_stage = render_active_stage(session, width);
        let navbar = render_navbar(session, width);
        write!(self.writer, "\r\x1b[2K{live_stage}\n\r\x1b[2K{navbar}")?;
        self.writer.flush()?;
        self.live_region_active = true;
        Ok(())
    }

    /// Leave the terminal on a clean line before returning to the shell.
    pub fn finish(&mut self) -> io::Result<()> {
        if self.live_region_active {
            self.writer.write_all(b"\r\x1b[2K\x1b[1A\r\x1b[2K\r\n")?;
            self.writer.flush()?;
            self.live_region_active = false;
        }
        Ok(())
    }

    fn clear_live_region(&mut self) -> io::Result<()> {
        if self.live_region_active {
            self.writer.write_all(b"\r\x1b[2K\x1b[1A\r\x1b[2K")?;
            self.live_region_active = false;
        }
        Ok(())
    }

    fn append_line(&mut self, line: String, width: usize) -> io::Result<()> {
        let line = fit_line(line, width);
        write!(self.writer, "\r\x1b[2K{line}\n")
    }

    fn append_expanded_details(
        &mut self,
        session: &InlineSession,
        width: usize,
    ) -> io::Result<()> {
        for stage in session.stages.stages() {
            if stage.kind() != &StageKind::Application
                && !stage.is_expanded()
                && stage.status() != StageStatus::Failed
            {
                continue;
            }

            let details = detail_text_lines(stage);
            let emitted = self
                .emitted_details
                .get(&stage.id())
                .copied()
                .unwrap_or_default();
            for detail in details.iter().skip(emitted) {
                self.append_line(format!("    {detail}"), width)?;
            }
            self.emitted_details.insert(stage.id(), details.len());
        }
        Ok(())
    }

    /// Return the wrapped writer after the renderer is no longer needed.
    #[inline]
    pub fn into_inner(self) -> W {
        self.writer
    }
}

#[cfg(test)]
fn render_lines(session: &InlineSession, width: usize) -> Vec<String> {
    render_managed_lines(session, width, &[])
}

fn render_active_stage(session: &InlineSession, width: usize) -> String {
    let mut line = if let Some(id) = session.stages.active()
        && let Some(stage) = session.stages.stage(id)
    {
        let selected = session.stages.selected() == Some(id);
        let mut line = render_stage_summary(stage, session.frame, Instant::now(), selected);
        line.push_str(" · ");
        line.push_str(status_label(&session.state.status));
        line
    } else {
        format!("  status: {}", status_label(&session.state.status))
    };

    if let Some(notice) = &session.notice {
        line.push_str(" · ");
        line.push_str(notice);
    }
    fit_line(line, width)
}

fn render_navbar(session: &InlineSession, width: usize) -> String {
    let mut navbar = String::from("  ↑/↓ select · ");
    if session.hot_reload_enabled {
        navbar.push_str("r hot-reload · ");
    }
    navbar.push_str("Shift+R hot-restart · Enter expand · Shift+Q quit");
    fit_line(navbar, width)
}

#[cfg(test)]
fn render_managed_lines(
    session: &InlineSession,
    width: usize,
    committed_stages: &[StageId],
) -> Vec<String> {
    let now = Instant::now();
    let mut lines = vec![
        format!("◆ Aimer · {}", session.target),
        String::new(),
    ];

    for stage in session.stages.stages() {
        if committed_stages.contains(&stage.id()) {
            continue;
        }
        let selected = session.stages.selected() == Some(stage.id());
        lines.push(render_stage_summary(stage, session.frame, now, selected));

        let show_tail = stage.kind() == &StageKind::Application
            && stage.status() == StageStatus::Running
            && !stage.is_expanded();
        if stage.is_expanded() || show_tail {
            let detail_lines = detail_text_lines(stage);
            let start = if show_tail {
                detail_lines.len().saturating_sub(APPLICATION_TAIL_LINES)
            } else {
                detail_lines.len().saturating_sub(MAX_EXPANDED_LINES)
            };
            if start > 0 && !show_tail {
                lines.push(format!("    ... {} earlier lines", start));
            }
            for detail in detail_lines.into_iter().skip(start) {
                lines.push(format!("    {detail}"));
            }
        }
    }

    if let Some(notice) = &session.notice {
        lines.push(format!("  {notice}"));
    }
    lines.push(String::new());
    lines.push(render_navbar(session, width));
    lines.push("  1 app · 2 build · 3 inspector · F12 inspector".to_string());

    lines.into_iter().map(|line| fit_line(line, width)).collect()
}

fn render_stage_summary(
    stage: &crate::console::stage::Stage,
    frame: usize,
    now: Instant,
    selected: bool,
) -> String {
    let marker = if selected { "▸" } else { " " };
    let icon = match stage.status() {
        StageStatus::Running => SPINNERS[frame % SPINNERS.len()],
        StageStatus::Succeeded => "✓",
        StageStatus::Failed => "✗",
        StageStatus::Cancelled => "⊘",
    };
    let elapsed = stage
        .timing()
        .elapsed()
        .unwrap_or_else(|| stage.timing().elapsed_at(now));
    let detail_lines: usize = stage.details().iter().map(|detail| detail.line_count()).sum();
    let progress = stage
        .progress()
        .and_then(|progress| progress.percentage())
        .map(|percent| format!(" · {percent}%"))
        .unwrap_or_default();
    format!(
        "{marker} {icon} {:<28} {}{} · {} lines",
        stage.kind().label(),
        format_duration(elapsed),
        progress,
        detail_lines,
    )
}

fn detail_text_lines(stage: &crate::console::stage::Stage) -> Vec<String> {
    stage
        .details()
        .iter()
        .flat_map(|detail| detail.as_str().lines().map(str::to_owned))
        .collect()
}

fn fit_line(mut line: String, width: usize) -> String {
    // ANSI-styled detail must remain intact. Let the terminal wrap a styled
    // line rather than truncating in the middle of an escape sequence and
    // leaking its style into the rest of the managed region.
    if line.contains('\x1b') || line.chars().count() <= width {
        return line;
    }
    line.truncate(line.char_indices().nth(width.saturating_sub(1)).map_or(0, |(i, _)| i));
    line.push('…');
    line
}

fn format_duration(duration: Duration) -> String {
    if duration.as_secs() > 0 {
        format!("{}.{:01}s", duration.as_secs(), duration.subsec_millis() / 100)
    } else {
        format!("{}ms", duration.as_millis())
    }
}

fn status_label(status: &Status) -> &'static str {
    match status {
        Status::Locking => "locking",
        Status::Fetching(_) => "fetching",
        Status::Compiling(_) => "compiling",
        Status::Building(_) => "assembling",
        Status::Launching => "launching",
        Status::Running => "running",
        Status::Idling => "idle",
        Status::Error => "error",
    }
}

/// Start the experimental inline console.
pub fn start(
    device: Device,
    pkg_name: String,
    release: bool,
    hot_reload_enabled: bool,
) -> anyhow::Result<()> {
    if !stdout().is_terminal() {
        return super::start_no_tui(device, pkg_name, release);
    }

    let _guard = RawModeGuard::new()?;
    let (tx, rx) = crossbeam::channel::unbounded();
    let inspector_runtime = Runtime::new().context("failed to start inspector server tokio runtime")?;
    let inspector_server_address = match device.target {
        Targets::Ios | Targets::Android => IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)),
        _ => IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)),
    };
    let inspector_handle = InspectorServer::start(
        inspector_server_address,
        9229,
        inspector_runtime.handle(),
    )
    .context("failed to start inspector server")?;

    let project_root = super::get_project_root(false)
        .map_err(|_| anyhow::anyhow!("failed to get project root"))?;
    let mut file_writer = if hot_reload_enabled && device.target == Targets::Web {
        let lib_path = project_root.join("src/main.rs");
        Some(
            File::options()
                .append(true)
                .read(true)
                .create(false)
                .open(lib_path)
                .context("failed to open src/main.rs for inline reload")?,
        )
    } else {
        None
    };

    let mut current_child = super::spawn_runner(
        device.clone(),
        pkg_name.clone(),
        tx.clone(),
        inspector_handle.address,
        inspector_handle.port,
        release,
    );

    let mut session = InlineSession::new_with_hot_reload(&device, hot_reload_enabled);
    let mut renderer = InlineRenderer::new(stdout());
    let tick_rate = Duration::from_millis(100);
    let mut last_tick = Instant::now();
    let mut quit = false;

    while !quit {
        while let Ok(event) = rx.try_recv() {
            session.apply_event(event);
        }

        let width = terminal::size().map(|(width, _)| width as usize).unwrap_or(120);
        renderer.render(&session, width)?;

        let timeout = tick_rate
            .checked_sub(last_tick.elapsed())
            .unwrap_or(Duration::ZERO);
        for event in input::drain_terminal(timeout)? {
            let Event::Key(key) = event else {
                continue;
            };
            let Some(action) = map_key_event(&key) else {
                continue;
            };
            match action {
                ConsoleAction::Quit => {
                    if let Some(mut child) = current_child.lock().unwrap().take() {
                        let _ = child.kill();
                    }
                    quit = true;
                }
                ConsoleAction::HotReload => {
                    if !hot_reload_enabled {
                        continue;
                    }
                    if let Some(file_writer) = file_writer.as_mut() {
                        file_writer.write_all(b" ")?;
                        file_writer.set_modified(SystemTime::now())?;
                        session.notice("Hot reload requested".to_string());
                    } else {
                        session.notice(
                            "Hot reload is unavailable for this target; press Shift+R for hot restart"
                                .to_string(),
                        );
                    }
                }
                ConsoleAction::HotRestart => {
                    if let Some(mut child) = current_child.lock().unwrap().take() {
                        let _ = child.kill();
                    }
                    session.reset_for_restart();
                    renderer.reset();
                    current_child = super::spawn_runner(
                        device.clone(),
                        pkg_name.clone(),
                        tx.clone(),
                        inspector_handle.address,
                        inspector_handle.port,
                        release,
                    );
                }
                ConsoleAction::SelectPane(pane) => select_pane(&mut session, pane),
                ConsoleAction::NextPane => session.state.next_pane(),
                ConsoleAction::ToggleInspector => {
                    inspector_handle.send_toggle();
                    session.state.pane = super::ConsoleType::Inspector;
                }
                ConsoleAction::ToggleInspectorTree => {
                    session.state.inspector_full_tree = !session.state.inspector_full_tree;
                }
                ConsoleAction::ToggleSourceLocations => session.state.toggle_log_source(),
                ConsoleAction::ToggleSelectionMode => {
                    session.state.selection_mode = !session.state.selection_mode;
                    if !session.state.selection_mode {
                        session.state.clear_selection();
                    }
                }
                ConsoleAction::YankSelection
                | ConsoleAction::CopyPane
                | ConsoleAction::CopySelectionOrPane => copy_focused_pane(&session),
                ConsoleAction::ClearPane => match session.state.pane {
                    super::ConsoleType::App => session.state.app_logs.clear(),
                    super::ConsoleType::Build => session.state.build_logs.clear(),
                    super::ConsoleType::Inspector => {}
                },
                ConsoleAction::ScrollUp | ConsoleAction::PageUp => {
                    let count = if matches!(action, ConsoleAction::PageUp) { 10 } else { 1 };
                    for _ in 0..count {
                        session.stages.select_previous();
                    }
                }
                ConsoleAction::ScrollDown | ConsoleAction::PageDown => {
                    let count = if matches!(action, ConsoleAction::PageDown) { 10 } else { 1 };
                    for _ in 0..count {
                        session.stages.select_next();
                    }
                }
                ConsoleAction::ExpandStage => {
                    session.stages.toggle_selected();
                }
            }
        }

        if last_tick.elapsed() >= tick_rate {
            session.frame = session.frame.wrapping_add(1);
            last_tick = Instant::now();
        }
    }

    renderer.finish()?;
    Ok(())
}

fn select_pane(session: &mut InlineSession, pane: ConsolePane) {
    session.state.pane = match pane {
        ConsolePane::App => super::ConsoleType::App,
        ConsolePane::Build => super::ConsoleType::Build,
        ConsolePane::Inspector => super::ConsoleType::Inspector,
    };
    session.state.clear_selection();
}

fn copy_focused_pane(session: &InlineSession) {
    let text = match session.state.pane {
        super::ConsoleType::App => session.state.app_log_text(),
        super::ConsoleType::Build => session.state.build_log_text(),
        super::ConsoleType::Inspector => return,
    };
    if let Ok(mut clipboard) = Clipboard::new() {
        let _ = clipboard.set_text(text);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn device() -> Device {
        Device {
            name: "macOS Desktop".to_string(),
            target: Targets::Macos,
            id: "local".to_string(),
        }
    }

    #[test]
    fn collapsed_successful_stages_hide_details() {
        let mut session = InlineSession::new(&device());
        let id = session.stages.start(StageKind::Compile, Instant::now());
        session.stages.append_detail(id, "cargo check output").unwrap();
        session.stages.finish(id, Instant::now()).unwrap();

        let lines = render_lines(&session, 120);
        assert!(lines.iter().any(|line| line.contains("✓ Compile")));
        assert!(!lines.iter().any(|line| line.contains("cargo check output")));
    }

    #[test]
    fn enter_expansion_reveals_retained_stage_details() {
        let mut session = InlineSession::new(&device());
        let id = session.stages.start(StageKind::Compile, Instant::now());
        session.stages.append_detail(id, "cargo check output").unwrap();
        session.stages.finish(id, Instant::now()).unwrap();
        session.stages.toggle_selected();

        let lines = render_lines(&session, 120);
        assert!(lines.iter().any(|line| line.contains("cargo check output")));
    }

    #[test]
    fn failed_stage_is_visible_and_expanded() {
        let mut session = InlineSession::new(&device());
        let id = session.stages.start(StageKind::Compile, Instant::now());
        session.stages.append_detail(id, "error: failed").unwrap();
        session.stages.fail(id, Instant::now()).unwrap();

        let lines = render_lines(&session, 120);
        assert!(lines.iter().any(|line| line.contains("✗ Compile")));
        assert!(lines.iter().any(|line| line.contains("error: failed")));
    }

    #[test]
    fn status_transitions_create_the_expected_execution_stages() {
        let mut session = InlineSession::new(&device());
        session.apply_status(Status::Compiling(10));
        session.apply_status(Status::Building(50));
        session.apply_status(Status::Launching);
        session.apply_status(Status::Running);

        let labels: Vec<String> = session
            .stages
            .stages()
            .iter()
            .map(|stage| stage.kind().label().to_owned())
            .collect();
        assert_eq!(labels, ["Compile", "Assemble", "Launch", "Application"]);
        assert_eq!(session.stages.active(), session.stages.selected());
    }

    #[test]
    fn application_log_details_are_visible_without_expansion() {
        let mut session = InlineSession::new(&device());
        session.apply_status(Status::Running);
        session.apply_event(RunnerEvent::AppLog("window ready".into()));

        let mut output = Vec::new();
        let mut renderer = InlineRenderer::new(&mut output);
        renderer.render(&session, 120).unwrap();

        let output = String::from_utf8(output).unwrap();
        assert!(output.contains("window ready"));
    }

    #[test]
    fn active_stage_spinner_is_rendered_with_the_stage_not_status_controls() {
        let mut session = InlineSession::new(&device());
        session.apply_status(Status::Compiling(10));

        let mut output = Vec::new();
        let mut renderer = InlineRenderer::new(&mut output);
        renderer.render(&session, 120).unwrap();

        let output = String::from_utf8(output).unwrap();
        assert!(output.contains("\r\x1b[2K▸ ⠋ Compile"));
        assert!(!output.contains("status: compiling · ⠋ Compile"));
        assert!(
            output.find("▸ ⠋ Compile").unwrap() < output.find("Shift+R hot-restart").unwrap()
        );
    }

    #[test]
    fn hot_reload_events_are_hidden_when_hot_reload_is_disabled() {
        let mut session = InlineSession::new(&device());
        session.apply_event(RunnerEvent::HotReload);

        assert!(session.stages.stages().is_empty());
    }

    #[test]
    fn hot_reload_control_is_hidden_when_hot_reload_is_disabled() {
        let session = InlineSession::new(&device());
        let mut output = Vec::new();
        let mut renderer = InlineRenderer::new(&mut output);
        renderer.render(&session, 120).unwrap();

        let output = String::from_utf8(output).unwrap();
        assert!(!output.contains("r hot-reload"));
        assert!(output.contains("Shift+R hot-restart"));
    }

    #[test]
    fn hot_reload_control_is_visible_when_hot_reload_is_enabled() {
        let session = InlineSession::new_with_hot_reload(&device(), true);
        let mut output = Vec::new();
        let mut renderer = InlineRenderer::new(&mut output);
        renderer.render(&session, 120).unwrap();

        let output = String::from_utf8(output).unwrap();
        assert!(output.contains("r hot-reload"));
    }

    #[test]
    fn hot_reload_events_are_retained_when_hot_reload_is_enabled() {
        let mut session = InlineSession::new_with_hot_reload(&device(), true);
        session.apply_event(RunnerEvent::HotReload);

        assert!(session
            .stages
            .stages()
            .iter()
            .any(|stage| stage.kind() == &StageKind::HotReload));
    }

    #[test]
    fn repeated_frames_update_the_live_block_without_rewriting_the_transcript() {
        let session = InlineSession::new(&device());
        let mut output = Vec::new();
        let mut renderer = InlineRenderer::new(&mut output);

        renderer.render(&session, 120).unwrap();
        renderer.render(&session, 120).unwrap();

        let output = String::from_utf8(output).unwrap();
        assert!(output.contains("\x1b[1A"));
        assert!(!output.contains("\x1b[2J"));
        assert_eq!(output.matches("◆ Aimer").count(), 1);
        assert!(output.matches("\r\x1b[2K").count() >= 2);
    }

    #[test]
    fn completed_stage_summary_is_emitted_to_scrollback_once() {
        let mut session = InlineSession::new(&device());
        let id = session.stages.start(StageKind::Compile, Instant::now());
        session.stages.finish(id, Instant::now()).unwrap();

        let mut output = Vec::new();
        let mut renderer = InlineRenderer::new(&mut output);
        renderer.render(&session, 120).unwrap();
        renderer.render(&session, 120).unwrap();

        let output = String::from_utf8(output).unwrap();
        assert_eq!(output.matches("✓ Compile").count(), 1);
    }

    #[test]
    fn failed_stage_summary_and_details_are_appended_once() {
        let mut session = InlineSession::new(&device());
        let id = session.stages.start(StageKind::Compile, Instant::now());
        session.stages.append_detail(id, "error: failed").unwrap();
        session.stages.fail(id, Instant::now()).unwrap();

        let mut output = Vec::new();
        let mut renderer = InlineRenderer::new(&mut output);
        renderer.render(&session, 120).unwrap();
        renderer.render(&session, 120).unwrap();

        let output = String::from_utf8(output).unwrap();
        assert_eq!(output.matches("✗ Compile").count(), 1);
        assert_eq!(output.matches("error: failed").count(), 1);
    }
}

//! What the system appearance is allowed to do to a running application.
//!
//! The user switches appearance in the system settings while the application is
//! open, and the platform announces it instead of restarting the app. An
//! `AnimatedTheme` that follows the system therefore has to cross into the other
//! theme mid-run, and one that was told to use a specific theme has to stay
//! exactly where it is — an application that pins its appearance is not asking
//! for the platform's opinion.
//!
//! The application may also change its mind about following at all, and that
//! change is a theme change like any other: it animates.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::thread::sleep;
use std::time::Duration;

use aimer::quiver::aimer_app::HeadlessAimerApp;
use aimer::quiver::winit::event::WindowEvent;
use aimer::quiver::winit::window::Theme as SystemTheme;
use aimer::style::{AnimatedTheme, Theme, ThemeData, ThemeMode};
use aimer::{
    AimerApp, AnyElement, BuildContext, Color, Element, ModalHost, SizedBox, State, StateUpdater,
    StatefulElement, StatefulWidget, StatelessElement, Widget,
};

/// A widget that records the theme it was built with, and counts its builds.
#[derive(Clone)]
struct Probe {
    background: Rc<Cell<Color>>,
    builds: Rc<Cell<u32>>,
}

impl Probe {
    fn new() -> Self {
        Self {
            background: Rc::new(Cell::new(Color::Transparent)),
            builds: Rc::new(Cell::new(0)),
        }
    }
}

impl Widget for Probe {
    fn to_element(self, ctx: &BuildContext) -> AnyElement {
        let source = self.clone();
        StatelessElement::from_builder(
            ctx,
            move |ctx| {
                source.builds.set(source.builds.get() + 1);
                source.background.set(ThemeData::of(ctx).background_color);
                SizedBox::new().width(10).height(10).to_element(ctx)
            },
            None,
            "Probe",
        )
        .boxed()
    }
}

/// Announces the appearance the system switched to, the way the platform does.
fn switch_system_to<W: Widget + 'static>(app: &mut HeadlessAimerApp<W>, theme: SystemTheme) {
    app.send_window_event(WindowEvent::ThemeChanged(theme));
    app.render_frame();
}

/// How long a transition in these tests lasts.
const TRANSITION: Duration = Duration::from_millis(250);

thread_local! {
    /// The shell's state, so a test can flip the mode the way a button does.
    static MODE: RefCell<Option<StateUpdater<ThemeSwitchState>>> = const { RefCell::new(None) };
}

/// An application with its own "follow the system" switch, as `jaime/src/system_theme.rs`
/// offers it.
struct ThemeSwitch {
    initial: ThemeMode,
    probe: Probe,
}

struct ThemeSwitchState {
    mode: ThemeMode,
    probe: Probe,
}

impl StatefulWidget for ThemeSwitch {
    type State = ThemeSwitchState;

    fn create_state(self) -> Self::State {
        ThemeSwitchState {
            mode: self.initial,
            probe: self.probe.clone(),
        }
    }
}

impl State<ThemeSwitch> for ThemeSwitchState {
    fn init_state(&mut self, updater: StateUpdater<Self>) {
        MODE.replace(Some(updater));
    }

    fn build(&self, _ctx: &BuildContext) -> impl Widget {
        AnimatedTheme::new()
            .mode(self.mode)
            .duration(TRANSITION)
            .child(self.probe.clone())
    }
}

impl Widget for ThemeSwitch {
    fn to_element(self, ctx: &BuildContext) -> AnyElement {
        StatefulElement::new_with_name(self, ctx, "ThemeSwitch", None)
            .0
            .boxed()
    }

    fn debug_name(&self) -> &'static str {
        "ThemeSwitch"
    }
}

/// Flips the application's mode, the way pressing its button does.
fn set_mode(mode: ThemeMode) {
    MODE.with_borrow(|updater| {
        updater
            .as_ref()
            .expect("the shell state should have published its updater")
            .set_state(move |state| state.mode = mode)
    });
}

/// Draws past the end of a transition.
fn settle<W: Widget + 'static>(app: &mut HeadlessAimerApp<W>) {
    for _ in 0..20 {
        app.render_frame();
        sleep(Duration::from_millis(20));
    }
}

/// Starts an application whose mode a test can flip, settled on `initial`.
fn app_switching_mode_from(
    initial: ThemeMode,
    probe: &Probe,
) -> HeadlessAimerApp<ModalHost<ThemeSwitch>> {
    let mut app = AimerApp::start_headless(ThemeSwitch {
        initial,
        probe: probe.clone(),
    });
    settle(&mut app);
    app
}

#[test]
fn a_system_appearance_switch_crosses_the_application_into_the_other_theme() {
    let probe = Probe::new();
    let mut app = AimerApp::start_headless(
        AnimatedTheme::new()
            // The transition itself is covered by the theme's own tests; this
            // one is about the appearance arriving at all.
            .duration(Duration::ZERO)
            .child(probe.clone()),
    );

    app.render_frame();
    assert_eq!(
        probe.background.get(),
        ThemeData::light().background_color,
        "a fresh application did not start in the appearance the platform reports"
    );

    switch_system_to(&mut app, SystemTheme::Dark);

    assert_eq!(
        probe.background.get(),
        ThemeData::dark().background_color,
        "the application kept the light theme after the system switched to dark"
    );

    switch_system_to(&mut app, SystemTheme::Light);

    assert_eq!(
        probe.background.get(),
        ThemeData::light().background_color,
        "the application stopped following the system after the first switch"
    );
}

#[test]
fn an_application_that_pins_its_theme_ignores_the_system_appearance() {
    let probe = Probe::new();
    let mut app = AimerApp::start_headless(
        AnimatedTheme::new()
            .mode(ThemeMode::Light)
            .duration(Duration::ZERO)
            .child(probe.clone()),
    );

    app.render_frame();
    let settled = probe.builds.get();

    switch_system_to(&mut app, SystemTheme::Dark);

    assert_eq!(
        probe.background.get(),
        ThemeData::light().background_color,
        "a pinned light theme was overruled by the system"
    );
    assert_eq!(
        probe.builds.get(),
        settled,
        "a system switch the application ignores rebuilt it {} times",
        probe.builds.get() - settled
    );
}

#[test]
fn a_single_theme_ignores_the_system_appearance() {
    let probe = Probe::new();
    let mut app = AimerApp::start_headless(
        AnimatedTheme::new()
            .data(ThemeData::dark())
            .duration(Duration::ZERO)
            .child(probe.clone()),
    );

    app.render_frame();

    switch_system_to(&mut app, SystemTheme::Light);

    assert_eq!(
        probe.background.get(),
        ThemeData::dark().background_color,
        "a named theme was replaced by the system appearance"
    );
}

#[test]
fn following_the_system_again_animates_out_of_the_pinned_theme() {
    let probe = Probe::new();
    // Pinned to dark while the system reports light, so restoring the system
    // means crossing the whole way back.
    let mut app = app_switching_mode_from(ThemeMode::Dark, &probe);
    assert_eq!(probe.background.get(), ThemeData::dark().background_color);

    set_mode(ThemeMode::System);
    app.render_frame();

    assert_ne!(
        probe.background.get(),
        ThemeData::light().background_color,
        "following the system again jumped straight into its theme instead of animating"
    );

    settle(&mut app);

    assert_eq!(
        probe.background.get(),
        ThemeData::light().background_color,
        "the transition into the system appearance never arrived"
    );
}

#[test]
fn pinning_a_theme_animates_out_of_the_system_appearance() {
    let probe = Probe::new();
    let mut app = app_switching_mode_from(ThemeMode::System, &probe);
    assert_eq!(probe.background.get(), ThemeData::light().background_color);

    set_mode(ThemeMode::Dark);
    app.render_frame();

    assert_ne!(
        probe.background.get(),
        ThemeData::dark().background_color,
        "pinning a theme jumped straight into it instead of animating"
    );

    settle(&mut app);

    assert_eq!(
        probe.background.get(),
        ThemeData::dark().background_color,
        "the transition into the pinned theme never arrived"
    );
}

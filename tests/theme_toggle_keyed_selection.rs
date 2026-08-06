//! A theme change must not reset a keyed section's selection.
//!
//! `website/src/screen/home_screen.rs` mounts `SameLookingSection` with an
//! explicit key inside a `Scrollable`, and
//! `website/src/components/app_shell.rs` animates the theme above it through a
//! *stateless* themed frame. Every tick of that transition rebuilds the frame,
//! which rebuilds the page and the section below it.
//!
//! The selection the user made lives in the section's own `State`, so it must
//! survive that rebuild — the section is the same widget, under the same key,
//! in the same place.

use std::cell::RefCell;
use std::thread::sleep;
use std::time::Duration;

use aimer::animation::{AnimatedSwitcher, Curve};
use aimer::base::{ResolvedSize, Size, Vec2d};
use aimer::quiver::aimer_app::HeadlessAimerApp;
use aimer::router::{Navigator, Outlet, Route, Router, Shell};
use aimer::style::{AnimatedTheme, Theme, ThemeData};
use aimer::{
    AimerApp, AnyElement, AnyWidget, BuildContext, Column, Container, Drawable, Element,
    EventElement, Expanded, Key, LayoutElement, ModalHost, Rebuildable, ScrollAxis, Scrollable,
    SizedBox, State, StateUpdater, StatefulElement, StatefulWidget, StatelessElement,
    VisitorElement, Widget,
};

/// The identity the section keeps wherever the page rebuilds it, as in
/// `website/src/screen/home_screen.rs`.
const SECTION_KEY: &str = "same-looking-section";
/// The identity the cross-fade inside the section keeps, as in
/// `website/src/components/same_looking.rs`.
const SWITCHER_KEY: &str = "platform-image-switcher";
/// The identity every route's transition shares, as in `website/src/router.rs`.
const ROUTE_SWITCHER_KEY: &str = "route-switcher";

const PLATFORMS: [&str; 4] = ["macos", "ios", "web", "android"];

thread_local! {
    /// The index each frame actually painted, in order.
    static PAINTED: RefCell<Vec<usize>> = const { RefCell::new(Vec::new()) };
    /// The shell's state, so a test can toggle the theme the way the header does.
    static THEME: RefCell<Option<StateUpdater<AppShellState>>> = const { RefCell::new(None) };
    /// The section's state, so a test can select a platform the way a tap does.
    static SELECTION: RefCell<Option<StateUpdater<SectionState>>> = const { RefCell::new(None) };
}

fn painted() -> Vec<usize> {
    PAINTED.with_borrow(|painted| painted.clone())
}

fn clear_painted() {
    PAINTED.with_borrow_mut(|painted| painted.clear());
}

// ---------------------------------------------------------------------------
// The route: `website/src/router.rs`.
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq)]
struct HomeRoute;

impl Route for HomeRoute {
    fn parse(path: &str) -> Option<Self> {
        (path == "/").then_some(Self)
    }

    fn format(&self) -> String {
        "/".to_owned()
    }
}

impl Router for HomeRoute {
    fn build(&self, _ctx: &BuildContext) -> AnyWidget {
        Shell::new(AppShell, |_| {
            AnimatedSwitcher::new(
                Duration::from_millis(200),
                Curve::FastOutSlowIn,
                HomePage.boxed(),
            )
            .child_key("home")
            .key(ROUTE_SWITCHER_KEY)
            .boxed()
        })
        .boxed()
    }
}

impl Widget for HomeRoute {
    fn to_element(&self, ctx: &BuildContext) -> AnyElement {
        Router::build(self, ctx).to_element(ctx)
    }
}

// ---------------------------------------------------------------------------
// The shell: `website/src/components/app_shell.rs`.
// ---------------------------------------------------------------------------

struct AppShell;

struct AppShellState {
    dark: bool,
}

impl StatefulWidget for AppShell {
    type State = AppShellState;

    fn create_state(&self) -> Self::State {
        AppShellState { dark: false }
    }
}

impl State<AppShell> for AppShellState {
    fn init_state(&mut self, updater: StateUpdater<Self>) {
        THEME.replace(Some(updater));
    }

    fn build(&self, _ctx: &BuildContext) -> impl Widget {
        AnimatedTheme::new()
            .data(if self.dark {
                ThemeData::dark()
            } else {
                ThemeData::light()
            })
            .duration(Duration::from_millis(250))
            .curve(Curve::EaseInOut)
            .child(ThemedFrame)
    }
}

impl Widget for AppShell {
    fn to_element(&self, ctx: &BuildContext) -> AnyElement {
        StatefulElement::new_with_name(self, ctx, "AppShell", None)
            .0
            .boxed()
    }

    fn debug_name(&self) -> &'static str {
        "AppShell"
    }
}

/// The themed frame around the `Outlet`: a *stateless* widget that reads the
/// theme, so every tick of the transition rebuilds it and everything below.
struct ThemedFrame;

impl Widget for ThemedFrame {
    fn to_element(&self, ctx: &BuildContext) -> AnyElement {
        StatelessElement::from_builder(
            ctx,
            move |ctx| {
                let theme = ThemeData::of(ctx);
                Container::new()
                    .color(theme.background_color)
                    .child(Column::new().children([
                        SizedBox::new().height(40).boxed(),
                        Expanded::new()
                            .child(Container::new().color(theme.background_color).child(Outlet))
                            .boxed(),
                    ]))
                    .to_element(ctx)
            },
            None,
            "ThemedFrame",
        )
        .boxed()
    }

    fn debug_name(&self) -> &'static str {
        "ThemedFrame"
    }
}

// ---------------------------------------------------------------------------
// The page: `website/src/screen/home_screen.rs`.
// ---------------------------------------------------------------------------

struct HomePage;

struct HomePageState;

impl StatefulWidget for HomePage {
    type State = HomePageState;

    fn create_state(&self) -> Self::State {
        HomePageState
    }
}

impl State<HomePage> for HomePageState {
    fn init_state(&mut self, _updater: StateUpdater<Self>) {}

    fn build(&self, ctx: &BuildContext) -> impl Widget {
        let theme = ThemeData::of(ctx);
        Container::new().color(theme.background_color).child(
            Scrollable::new()
                .axis(ScrollAxis::Vertical)
                .child(Column::new().children([
                    SizedBox::new().height(32).boxed(),
                    SelectionSection {
                        key: Some(SECTION_KEY.into()),
                    }
                    .boxed(),
                    SizedBox::new().height(48).boxed(),
                ])),
        )
    }
}

impl Widget for HomePage {
    fn to_element(&self, ctx: &BuildContext) -> AnyElement {
        StatefulElement::new_with_name(self, ctx, "HomePage", None)
            .0
            .boxed()
    }

    fn debug_name(&self) -> &'static str {
        "HomePage"
    }
}

// ---------------------------------------------------------------------------
// The section: `website/src/components/same_looking.rs`.
// ---------------------------------------------------------------------------

struct SelectionSection {
    key: Option<Key>,
}

struct SectionState {
    current_index: usize,
    state: StateUpdater<Self>,
}

impl StatefulWidget for SelectionSection {
    type State = SectionState;

    fn create_state(&self) -> Self::State {
        SectionState {
            current_index: 0,
            state: StateUpdater::new(),
        }
    }
}

impl State<SelectionSection> for SectionState {
    fn init_state(&mut self, updater: StateUpdater<Self>) {
        self.state = updater;
    }

    fn build(&self, ctx: &BuildContext) -> impl Widget {
        let theme = ThemeData::of(ctx);
        let index = self.current_index;
        // The real section hands this same clone to its buttons' callback, so a
        // test drives exactly the updater a tap would.
        SELECTION.replace(Some(self.state.clone()));
        Container::new()
            .color(theme.background_color)
            .child(Column::new().children([
                Container::new()
                    .height(120)
                    .child(
                        AnimatedSwitcher::new(
                            Duration::from_millis(350),
                            Curve::FastOutSlowIn,
                            Marker::new(index),
                        )
                        .child_key(PLATFORMS[index % PLATFORMS.len()])
                        .key(SWITCHER_KEY),
                    )
                    .boxed(),
                SizedBox::new().height(40).boxed(),
            ]))
    }
}

impl Widget for SelectionSection {
    fn key(&self) -> Option<Key> {
        self.key.clone()
    }

    fn to_element(&self, ctx: &BuildContext) -> AnyElement {
        StatefulElement::new_with_name(self, ctx, "SelectionSection", self.key())
            .0
            .boxed()
    }

    fn debug_name(&self) -> &'static str {
        "SelectionSection"
    }
}

// ---------------------------------------------------------------------------
// A leaf that records the index it was painted with.
// ---------------------------------------------------------------------------

struct Marker {
    index: usize,
}

impl Marker {
    fn new(index: usize) -> Self {
        Self { index }
    }
}

impl Widget for Marker {
    fn to_element(&self, ctx: &BuildContext) -> AnyElement {
        MarkerElement {
            index: self.index,
            child: SizedBox::new().height(80).to_element(ctx),
        }
        .boxed()
    }

    fn debug_name(&self) -> &'static str {
        "Marker"
    }
}

struct MarkerElement {
    index: usize,
    child: AnyElement,
}

impl VisitorElement for MarkerElement {
    fn visit_children<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
        visitor(self.child.as_ref());
    }

    fn debug_name(&self) -> &'static str {
        "Marker"
    }
}

impl Drawable for MarkerElement {
    fn draw(&self, ctx: &BuildContext) {
        PAINTED.with_borrow_mut(|painted| painted.push(self.index));
        self.child.draw(ctx);
    }
}

impl EventElement for MarkerElement {}

impl Rebuildable for MarkerElement {
    fn rebuild_if_dirty(&self, ctx: &BuildContext) {
        self.child.rebuild_if_dirty(ctx);
    }

    fn mark_needs_rebuild(&self) {
        self.child.mark_needs_rebuild();
    }
}

impl LayoutElement for MarkerElement {
    fn pos(&self) -> Option<Vec2d> {
        self.child.pos()
    }

    fn size(&self) -> Option<Size> {
        self.child.size()
    }

    fn layout(&self, ctx: &BuildContext) -> ResolvedSize {
        self.child.layout(ctx)
    }

    fn computed_size(&self, ctx: &BuildContext) -> ResolvedSize {
        self.child.computed_size(ctx)
    }

    fn content_size(&self, ctx: &BuildContext) -> ResolvedSize {
        self.child.content_size(ctx)
    }

    fn layer(&self) -> u32 {
        self.child.layer()
    }

    fn flex(&self) -> Option<f32> {
        self.child.flex()
    }

    fn get_size_from_child(&self) -> Option<Size> {
        self.child.get_size_from_child()
    }

    fn invalidate_layout(&self) {
        self.child.invalidate_layout();
    }

    fn pos_start_end(&self) -> Option<(Vec2d, Vec2d)> {
        self.child.pos_start_end()
    }
}

// ---------------------------------------------------------------------------

fn select(index: usize) {
    SELECTION.with_borrow(|selection| {
        selection
            .as_ref()
            .expect("the section should have published its updater")
            .set_state(move |state| state.current_index = index)
    });
}

fn toggle_theme() {
    THEME.with_borrow(|theme| {
        theme
            .as_ref()
            .expect("the shell should have published its updater")
            .set_state(|state| state.dark = !state.dark)
    });
}

fn app_on_the_selected_section() -> HeadlessAimerApp<ModalHost<Navigator<HomeRoute>>> {
    let mut app = AimerApp::start_headless(Navigator::<HomeRoute>::new(HomeRoute, |route| {
        route.boxed()
    }));

    app.render_frame();
    select(1);
    // Draw past the cross-fade the selection starts.
    for _ in 0..20 {
        app.render_frame();
        sleep(Duration::from_millis(20));
    }

    clear_painted();
    app.render_frame();
    assert_eq!(
        painted().last().copied(),
        Some(1),
        "the selection never reached the section: {:?}",
        painted()
    );

    app
}

#[test]
fn a_theme_change_keeps_the_selection_a_keyed_section_holds() {
    let mut app = app_on_the_selected_section();

    toggle_theme();

    // Draw the whole transition: every frame of it rebuilds the section.
    for _ in 0..20 {
        app.render_frame();
        sleep(Duration::from_millis(20));
    }

    clear_painted();
    app.render_frame();

    assert_eq!(
        painted().last().copied(),
        Some(1),
        "the theme change reset the selection: {:?}",
        painted()
    );
}

#[test]
fn a_selection_still_changes_after_a_theme_change() {
    let mut app = app_on_the_selected_section();

    toggle_theme();
    for _ in 0..20 {
        app.render_frame();
        sleep(Duration::from_millis(20));
    }

    select(2);
    for _ in 0..20 {
        app.render_frame();
        sleep(Duration::from_millis(20));
    }

    clear_painted();
    app.render_frame();

    assert_eq!(
        painted().last().copied(),
        Some(2),
        "the section stopped responding to selections: {:?}",
        painted()
    );
}

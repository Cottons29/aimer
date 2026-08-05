//! What a rebuild above a route transition may and may not do to the page below.
//!
//! `website/src/router.rs` wraps every page in one `AnimatedSwitcher` that keeps
//! the same identity (`ROUTE_SWITCHER_KEY`) across routes, so the switcher's
//! state — and its cross-fade — survives navigation. The shell above it
//! (`website/src/components/app_shell.rs`) animates the theme, and
//! `website/src/screen/blog.rs` reads that theme with `ThemeData::of` and puts an
//! `AsyncBuilder` below it. Every tick of the transition therefore rebuilds the
//! shell, the `Outlet`, the switcher and the page.
//!
//! Two things must hold at once:
//!
//! * a theme change keeps the request the page already completed — it belongs to
//!   the element, not to the frame that rebuilt it;
//! * a navigation still switches the page — the outgoing page's state must not be
//!   handed to the page replacing it.

use std::cell::{Cell, RefCell};
use std::thread::sleep;
use std::time::Duration;

use aimer::animation::{AnimatedSwitcher, Curve};
use aimer::base::{ResolvedSize, Size, Vec2d};
use aimer::router::{
    Navigator, NavigatorController, NavigatorInstance, Outlet, Route, Router, Shell,
};
use aimer::quiver::aimer_app::HeadlessAimerApp;
use aimer::style::{AnimatedTheme, Theme, ThemeData};
use aimer::{
    AimerApp, AnyElement, AnyWidget, AsyncBuilder, AsyncSnapshot, BuildContext, Column, Container,
    Drawable, Element, EventElement, Expanded, LayoutElement, Rebuildable, ScrollAxis, Scrollable,
    ModalHost, SizedBox, State, StateUpdater, StatefulElement, StatefulWidget, StatelessElement,
    VisitorElement, Widget,
};

/// Height of the loaded content: taller than the headless viewport, so a frame
/// that paints the waiting state is impossible to mistake for a loaded one.
const CONTENT_HEIGHT: u32 = 4_000;

/// The identity every route's transition shares, as in `website/src/router.rs`.
const ROUTE_SWITCHER_KEY: &str = "route-switcher";

thread_local! {
    /// Labels of what the pages actually painted, in order.
    static PAINTED: RefCell<Vec<&'static str>> = const { RefCell::new(Vec::new()) };
    /// How many times the blog page's request was started.
    static LAUNCHES: Cell<usize> = const { Cell::new(0) };
    /// The shell's state, so a test can toggle the theme the way the header does.
    static THEME: RefCell<Option<StateUpdater<AppShellState>>> = const { RefCell::new(None) };
    /// The navigator the blog page looked up, so a test can navigate from it.
    static NAVIGATOR: RefCell<Option<NavigatorInstance<TestRoute>>> = const { RefCell::new(None) };
}

fn painted() -> Vec<&'static str> {
    PAINTED.with_borrow(|painted| painted.clone())
}

fn clear_painted() {
    PAINTED.with_borrow_mut(|painted| painted.clear());
}

// ---------------------------------------------------------------------------
// The routes: `website/src/router.rs`.
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq)]
enum TestRoute {
    Blog,
    Learn,
}

impl Route for TestRoute {
    fn parse(path: &str) -> Option<Self> {
        match path {
            "/blog" => Some(Self::Blog),
            "/learn" => Some(Self::Learn),
            _ => None,
        }
    }

    fn format(&self) -> String {
        match self {
            Self::Blog => "/blog".to_owned(),
            Self::Learn => "/learn".to_owned(),
        }
    }
}

fn transitioned_page(key: &'static str, child: AnyWidget) -> AnimatedSwitcher<AnyWidget> {
    AnimatedSwitcher::new(Duration::from_millis(200), Curve::FastOutSlowIn, child)
        .child_key(key)
        .key(ROUTE_SWITCHER_KEY)
}

impl Router for TestRoute {
    fn build(&self, _ctx: &BuildContext) -> AnyWidget {
        match self {
            Self::Blog => Shell::new(AppShell, |_| {
                transitioned_page("blog", BlogListPage.boxed()).boxed()
            })
            .boxed(),
            Self::Learn => Shell::new(AppShell, |_| {
                transitioned_page("learn", LearnPage.boxed()).boxed()
            })
            .boxed(),
        }
    }
}

impl Widget for TestRoute {
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

/// The themed frame around the `Outlet`: it reads the theme, so every tick of
/// the transition rebuilds it and the route below.
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
// The pages: `website/src/screen/*`.
// ---------------------------------------------------------------------------

/// Reads the theme, looks up the navigator and hosts the request — the shape of
/// `website/src/screen/blog.rs`.
struct BlogListPage;

impl Widget for BlogListPage {
    fn to_element(&self, ctx: &BuildContext) -> AnyElement {
        StatelessElement::from_builder(
            ctx,
            move |ctx| {
                let theme = ThemeData::of(ctx);
                NAVIGATOR.replace(Some(NavigatorController::<TestRoute>::of(ctx)));

                let content = AsyncBuilder::new()
                    .future(|| {
                        LAUNCHES.set(LAUNCHES.get() + 1);
                        async { Ok::<_, String>(CONTENT_HEIGHT) }
                    })
                    .child(blog_list_content)
                    .boxed();

                Container::new()
                    .color(theme.background_color)
                    .child(
                        Scrollable::new().axis(ScrollAxis::Vertical).child(
                            Container::new().child(Column::new().children([
                                SizedBox::new().height(32).boxed(),
                                content,
                                SizedBox::new().height(48).boxed(),
                            ])),
                        ),
                    )
                    .to_element(ctx)
            },
            None,
            "BlogListPage",
        )
        .boxed()
    }

    fn debug_name(&self) -> &'static str {
        "BlogListPage"
    }
}

fn blog_list_content(snapshot: &AsyncSnapshot<u32, String>) -> AnyWidget {
    match snapshot {
        AsyncSnapshot::Waiting => Marker::new("waiting", 40).boxed(),
        AsyncSnapshot::Error(_) => Marker::new("error", 40).boxed(),
        AsyncSnapshot::Data(height) => Marker::new("data", *height).boxed(),
    }
}

/// The page navigated to.
///
/// Built like every other screen of the site — the theme, a scroll view, an
/// `AsyncBuilder` — because that is what makes a mistaken hand-over visible: an
/// `AsyncBuilder` here and one on the blog page are the same widget under the
/// same name, so state grafted from one renders the other's content.
struct LearnPage;

impl Widget for LearnPage {
    fn to_element(&self, ctx: &BuildContext) -> AnyElement {
        StatelessElement::from_builder(
            ctx,
            move |ctx| {
                let theme = ThemeData::of(ctx);

                let content = AsyncBuilder::new()
                    .future(|| async { Ok::<_, String>(120_u32) })
                    .child(learn_content)
                    .boxed();

                Container::new()
                    .color(theme.background_color)
                    .child(
                        Scrollable::new().axis(ScrollAxis::Vertical).child(
                            Container::new().child(Column::new().children([
                                SizedBox::new().height(32).boxed(),
                                content,
                                SizedBox::new().height(48).boxed(),
                            ])),
                        ),
                    )
                    .to_element(ctx)
            },
            None,
            "LearnPage",
        )
        .boxed()
    }

    fn debug_name(&self) -> &'static str {
        "LearnPage"
    }
}

fn learn_content(snapshot: &AsyncSnapshot<u32, String>) -> AnyWidget {
    match snapshot {
        AsyncSnapshot::Waiting => Marker::new("learn-waiting", 40).boxed(),
        AsyncSnapshot::Error(_) => Marker::new("learn-error", 40).boxed(),
        AsyncSnapshot::Data(height) => Marker::new("learn", *height).boxed(),
    }
}

// ---------------------------------------------------------------------------
// A leaf that records the frames it was painted in.
// ---------------------------------------------------------------------------

struct Marker {
    label: &'static str,
    height: u32,
}

impl Marker {
    fn new(label: &'static str, height: u32) -> Self {
        Self { label, height }
    }
}

impl Widget for Marker {
    fn to_element(&self, ctx: &BuildContext) -> AnyElement {
        MarkerElement {
            label: self.label,
            child: SizedBox::new().height(self.height).to_element(ctx),
        }
        .boxed()
    }

    fn debug_name(&self) -> &'static str {
        "Marker"
    }
}

struct MarkerElement {
    label: &'static str,
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
        PAINTED.with_borrow_mut(|painted| painted.push(self.label));
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

/// Starts the application on the blog route and draws until its request landed.
fn app_on_the_loaded_blog_route() -> HeadlessAimerApp<ModalHost<Navigator<TestRoute>>> {
    let mut app = AimerApp::start_headless(Navigator::<TestRoute>::new(TestRoute::Blog, |route| {
        route.boxed()
    }));

    app.render_frame();
    sleep(Duration::from_millis(100));
    app.render_frame();
    app.render_frame();

    assert_eq!(
        painted().last().copied(),
        Some("data"),
        "the request never reached the page: {:?}",
        painted()
    );
    assert_eq!(LAUNCHES.get(), 1);

    app
}

#[test]
fn a_theme_change_keeps_the_request_the_page_already_completed() {
    let mut app = app_on_the_loaded_blog_route();
    clear_painted();

    THEME.with_borrow(|theme| {
        theme
            .as_ref()
            .expect("the shell state should have published its updater")
            .set_state(|state| state.dark = !state.dark)
    });

    // Draw the whole transition: every frame of it rebuilds the page.
    for _ in 0..20 {
        app.render_frame();
        sleep(Duration::from_millis(20));
    }

    assert_eq!(
        LAUNCHES.get(),
        1,
        "the theme change started the request again; the page painted {:?}",
        painted()
    );
    assert!(
        !painted().contains(&"waiting"),
        "the theme change painted the waiting state again: {:?}",
        painted()
    );
}

#[test]
fn a_navigation_replaces_the_page_the_transition_kept_its_identity_for() {
    let mut app = app_on_the_loaded_blog_route();

    NAVIGATOR.with_borrow(|navigator| {
        navigator
            .as_ref()
            .expect("the blog page should have looked up the navigator")
            .push(TestRoute::Learn)
    });

    // Draw past the end of the cross-fade.
    for _ in 0..20 {
        app.render_frame();
        sleep(Duration::from_millis(20));
    }

    clear_painted();
    app.render_frame();

    assert_eq!(
        painted(),
        vec!["learn"],
        "the navigation did not settle on the page pushed"
    );
}

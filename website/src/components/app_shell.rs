use std::sync::atomic::Ordering;

use aimer::router::{Navigator, Outlet};
use aimer::{BuildContext, Widget, widget, *};

use crate::components::header::HeaderSection;
use crate::screen::home_screen::SHOW_ICON;

/// The persistent application shell frame: a fixed [`HeaderSection`] on top and
/// a flexible content area below where the active route renders through an
/// [`Outlet`].
///
/// The shell frame stays mounted while the router only swaps the `Outlet`'s
/// child, so navigating between `Home`, `Docs` and `Learn` never rebuilds the
/// header from scratch conceptually — it is the content area that changes.

#[widget(Stateless)]
#[derive(Clone)]
pub struct AppShell {
    /// Index of the currently active header tab (0 = Home, 1 = Docs, 2 =
    /// Learn), used to highlight the matching header button.
    pub active_tab: usize,
}

impl StatelessWidget for AppShell {
    fn build(&self, _ctx: &BuildContext) -> impl Widget {
        Container::new()
            .color(Color::WHITE)
            .child(Column::new().children(vec![
                    Box::new(HeaderSection { active_tab: self.active_tab }),
                    Expanded::new()
                        .child(Container::new()
                            .color(Color::WHITE)
                            .child(Outlet))
                        .boxed(),
                ]))
    }
}

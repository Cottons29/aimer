use std::time::Duration;

use aimer::animation::Curve;
use aimer::router::Outlet;
use aimer::style::{AnimatedTheme, Theme, ThemeData};
use aimer::{BuildContext, Widget, widget, *};

use crate::components::header::HeaderSection;

/// The persistent application shell frame: a fixed [`HeaderSection`] on top and
/// a flexible content area below where the active route renders through an
/// [`Outlet`].
///
/// The shell frame stays mounted while the router only swaps the `Outlet`'s
/// child, so navigating between `Home`, `Docs` and `Learn` never rebuilds the
/// header from scratch conceptually — it is the content area that changes.

pub(crate) const DARK_THEME_ICON: &[u8] = include_bytes!("../../assets/dark-svgrepo-com.svg");
pub(crate) const LIGHT_THEME_ICON: &[u8] = include_bytes!("../../assets/light-svgrepo-com.svg");

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum WebsiteThemeMode {
    Light,
    Dark,
}

impl WebsiteThemeMode {
    fn theme(self) -> ThemeData {
        match self {
            Self::Light => ThemeData::light(),
            Self::Dark => ThemeData::dark(),
        }
    }

    fn toggled(self) -> Self {
        match self {
            Self::Light => Self::Dark,
            Self::Dark => Self::Light,
        }
    }

    pub(crate) fn toggle_icon(self) -> &'static [u8] {
        match self {
            Self::Light => DARK_THEME_ICON,
            Self::Dark => LIGHT_THEME_ICON,
        }
    }
}

#[widget(Stateful)]
pub struct AppShell {
    /// Index of the currently active header tab (0 = Home, 1 = Docs, 2 =
    /// Learn), used to highlight the matching header button.
    pub active_tab: usize,
}

pub struct AppShellState {
    active_tab: usize,
    theme_mode: WebsiteThemeMode,
    updater: StateUpdater<Self>,
}

impl AppShellState {
    pub(crate) fn toggle_theme(&mut self) {
        self.theme_mode = self.theme_mode.toggled();
    }
}

impl StatefulWidget for AppShell {
    type State = AppShellState;

    fn create_state(&self) -> Self::State {
        AppShellState {
            active_tab: self.active_tab,
            theme_mode: WebsiteThemeMode::Light,
            updater: StateUpdater::new(),
        }
    }
}

impl State<AppShell> for AppShellState {
    fn init_state(&mut self, updater: StateUpdater<Self>) {
        self.updater = updater;
    }

    fn adopt_config_from(&mut self, new: &Self) {
        self.active_tab = new.active_tab;
    }

    fn build(&self, _ctx: &BuildContext) -> impl Widget {
        AnimatedTheme::new()
            .data(self.theme_mode.theme())
            .duration(Duration::from_millis(250))
            .curve(Curve::EaseInOut)
            .child(ThemedAppShellFrame {
                active_tab: self.active_tab,
                theme_mode: self.theme_mode,
                theme_updater: self.updater.clone(),
            })
    }
}

#[widget(Stateless)]
#[derive(Clone)]
struct ThemedAppShellFrame {
    active_tab: usize,
    theme_mode: WebsiteThemeMode,
    theme_updater: StateUpdater<AppShellState>,
}

impl StatelessWidget for ThemedAppShellFrame {
    fn build(&self, ctx: &BuildContext) -> impl Widget {
        let theme = ThemeData::of(ctx);

        Container::new()
            .color(theme.background_color)
            .child(Column::new().children(vec![
            #[cfg(any(target_os = "android", target_os = "ios") )]
            SizedBox::new().height(40).boxed(),
            HeaderSection {
                active_tab: self.active_tab,
                theme_mode: self.theme_mode,
                theme_updater: self.theme_updater.clone(),
            }.boxed(),
            Expanded::new()
                .child(Container::new().color(theme.background_color).child(Outlet))
                .boxed(),
        ]))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn website_theme_mode_selects_the_matching_theme() {
        assert_eq!(WebsiteThemeMode::Light.theme(), ThemeData::light());
        assert_eq!(WebsiteThemeMode::Dark.theme(), ThemeData::dark());
    }

    #[test]
    fn website_theme_mode_toggles_and_selects_the_opposite_mode_icon() {
        assert_eq!(WebsiteThemeMode::Light.toggled(), WebsiteThemeMode::Dark);
        assert_eq!(WebsiteThemeMode::Dark.toggled(), WebsiteThemeMode::Light);
        assert_eq!(WebsiteThemeMode::Light.toggle_icon(), DARK_THEME_ICON);
        assert_eq!(WebsiteThemeMode::Dark.toggle_icon(), LIGHT_THEME_ICON);
    }

    #[test]
    fn bundled_theme_icons_are_valid_svg_documents() {
        assert!(SvgDocument::from_svg(DARK_THEME_ICON).is_ok());
        assert!(SvgDocument::from_svg(LIGHT_THEME_ICON).is_ok());
    }
}

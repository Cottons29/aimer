#[macro_use]
pub mod home;
#[macro_use]
pub mod profile;
#[macro_use]
pub mod setting;

use crate::routing::home::HomeWidget;
use crate::routing::setting::SettingPage;
use aimer::*;
use aimer::macros::widget;
use aimer::router::{Navigator, Router};
use crate::routing::profile::ProfilePage;

#[widget(Router)]
#[derive(Clone)]
pub enum AppRouting {
    #[route("/")]
    Home,
    #[route("/profile/{name}")]
    Profile { name: String },
    #[route("/settings")]
    Settings,
}


impl Router for AppRouting {
    fn build(&self, _: &BuildContext) -> Box<dyn Widget> {
        match self {
            AppRouting::Home => Box::new(HomeWidget {}),
            AppRouting::Settings => Box::new(SettingPage {}),
            AppRouting::Profile { name } => {
                Box::new(ProfilePage! ( name: name.clone() ))
            }
        }
    }
}

pub fn state_router() {
    AimerApp::start(Navigator::<AppRouting>::new(AppRouting::Home, |route| {
        Box::new(route)
    }))
}

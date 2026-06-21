use aimer::macros::widget;
use aimer::style::*;
use aimer::*;
use std::time::Duration;
use aimer::animation::{AnimInstant, Animated, AnimationController};
use uuid::Uuid;

const ANIM_DURATION: Duration = Duration::from_millis(50);

pub fn start_my_animated_list() {
    AimerApp::start(MyAnimatedList {})
}
#[allow(non_snake_case)]
#[widget(Stateful)]
pub struct MyAnimatedList {}

pub struct ListItem {
    id: String,
    text: String,
    controller: AnimationController,
    pending_removal: bool,
    removal_started_at: Option<AnimInstant>,
}

pub struct MyListState {
    list: Vec<ListItem>,
    updater: StateUpdater<Self>,
}

impl Drop for ListItem {
    fn drop(&mut self) {
        println!("Dropping ListItem {}", self.id);
    }
}

impl StatefulWidget for MyAnimatedList {
    type State = MyListState;
    fn create_state(&self) -> Self::State {
        MyListState {
            list: vec![],
            updater: StateUpdater::empty(),
        }
    }
}

impl State<MyAnimatedList> for MyListState {
    fn init_state(&mut self, _updater: StateUpdater<Self>)
    where
        Self: Sized,
    {
        self.updater = _updater;
    }

    fn build(&self, _: &BuildContext) -> impl Widget {
        // Clean up items whose reverse animation has finished (based on elapsed time).
        // We spawn this on a separate thread because build() is called while the state
        // lock is held, and set_state() also needs to acquire that lock — calling it
        // directly here would deadlock.
        {
            let now = AnimInstant::now();
            let has_dismissed = self.list.iter().any(|item| {
                item.pending_removal
                    && item
                        .removal_started_at
                        .map(|t| now.duration_since(t) >= ANIM_DURATION)
                        .unwrap_or(false)
            });
            if has_dismissed {
                let updater = self.updater.clone();
                let cleanup = move || {
                    updater.set_state(|state| {
                        let now = AnimInstant::now();
                        state.list.retain(|item| {
                            !(item.pending_removal
                                && item
                                    .removal_started_at
                                    .map(|t| now.duration_since(t) >= ANIM_DURATION)
                                    .unwrap_or(false))
                        });
                    });
                };
                #[cfg(target_arch = "wasm32")]
                wasm_bindgen_futures::spawn_local(async move {
                    cleanup();
                });
                #[cfg(not(target_arch = "wasm32"))]
                std::thread::spawn(move || {
                    cleanup();
                });
            }
        }

        Container!(
            child: Column!(
                children: [
                    Container!(
                        height: 90.0,
                        box_decoration: BoxDecoration!(
                            background_color: Colors::Gray.alpha(120),
                        ),
                        child: Row!(
                            vertical_alignment: BoxAlignment::Center,
                            children: [
                                Container!(
                                    child: Text!(
                                        format!("Item in List: {}", self.list.iter().filter(|i| !i.pending_removal).count()),
                                        text_align: TextAlign::MidCenter,
                                        text_style: TextStyle!(
                                            font_size: 20,
                                            color: Colors::Black,
                                        )
                                    )
                                ),
                                Container!(
                                     height: 50,
                                     width: 200,
                                    child: Button!(
                                        on_press: {
                                            let updater = self.updater.clone();
                                            move || {
                                                updater.set_state(|state| {
                                                    let uuid = Uuid::new_v4().to_string();
                                                    let mut controller = AnimationController::new(
                                                        Duration::from_millis(1000),
                                                        Curve::EaseOut,
                                                    );
                                                    controller.forward();
                                                    state.list.push(ListItem {
                                                        id: uuid.clone(),
                                                        text: uuid,
                                                        controller,
                                                        pending_removal: false,
                                                        removal_started_at: None,
                                                    })
                                                });
                                            }
                                        },
                                        decoration: BoxDecoration!(background_color: Colors::Gray),
                                        hover_decoration: BoxDecoration!(background_color: Colors::Gray.alpha(120)),
                                        child: Container!(
                                            child: Text!(
                                                "Add Item",
                                                text_align: TextAlign::MidCenter,
                                                text_style: TextStyle!(
                                                    color: Colors::Black,
                                                    font_size: 15,
                                                )
                                            )
                                        )
                                    )
                                ),
                            ]
                        )
                    ),
                    Scrollable!(
                        child: Column!(
                            children: self.list.iter().map(|item| {
                                Animated! (
                                    controller: item.controller.clone(),
                                    effect: AnimationEffect::SlideX { from: -1.0, to: 0.0 },
                                    child: Container!(
                                        margin: LayoutSpacing!(top: Spacing::Px(10)),
                                        color: Colors::Blue.alpha(100),
                                        height: 50,
                                        child: Row!(
                                            children: [
                                                Container!(
                                                    padding: LayoutSpacing!(left: Spacing::Px(10)),
                                                    child: Text!(
                                                        format!("Item : {}", item.text),
                                                        text_align: TextAlign::MidLeft,
                                                        text_style: TextStyle!(
                                                            color: Colors::Black,
                                                            font_size: 15,
                                                        )
                                                    )
                                                ),
                                                Container!(
                                                    width: 200,
                                                    height: 50,
                                                    child: Button!(
                                                        on_press: {
                                                            let item_id = item.id.clone();
                                                            let updater = self.updater.clone();
                                                            move || {
                                                                updater.set_state_with(&item_id, |state, id| {
                                                                    if let Some(item) = state.list.iter_mut().find(|i| i.id == id) {
                                                                        if !item.pending_removal {
                                                                            item.pending_removal = true;
                                                                            item.removal_started_at = Some(AnimInstant::now());
                                                                            item.controller.reverse();
                                                                        }
                                                                    }
                                                                });
                                                            }
                                                        },
                                                        decoration: BoxDecoration!(background_color: Colors::Gray),
                                                        hover_decoration: BoxDecoration!(background_color: Colors::Gray.alpha(120)),
                                                        child: Container!(
                                                            height: 50,
                                                            width: 200,
                                                            child: Text!(
                                                                "Delete",
                                                                text_align: TextAlign::MidCenter,
                                                                text_style: TextStyle!(
                                                                    color: Colors::Black,
                                                                    font_size: 15,
                                                                )
                                                            )
                                                        )
                                                    )
                                                )
                                            ]
                                        )
                                    )
                                )
                            }).collect::<Vec<Box<dyn Widget>>>()
                        )
                    )
                ]
            )
        )
    }
}

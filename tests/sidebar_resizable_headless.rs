//! A `Resizable` wrapping an interactive child — the shape of RustForum's
//! side bar — must not swallow the child's events.
//!
//! The side bar wraps everything in an `ImplicitAnimatedBuilder`, whose target
//! flips whenever the pointer crosses the resize handle. The animation element
//! rebuilds its subtree on every frame it draws, and each rebuild replaces the
//! subtree's element identities. The dispatcher routes captured pointers and
//! focus by identity, so a press that spans one of those rebuilds dies: the
//! release is addressed to an element that no longer exists and is dropped.
//!
//! The interesting claims are about *routing*, so everything here is driven by
//! window events — the same ones a mouse produces — through the real headless
//! event loop.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use aimer::animation::{Curve, ImplicitAnimatedBuilder};
use aimer::quiver::aimer_app::HeadlessAimerApp;
use aimer::quiver::winit::dpi::PhysicalPosition;
use aimer::quiver::winit::event::{DeviceId, ElementState, MouseButton, WindowEvent};
use aimer::style::LayoutSpacing;
use aimer::{
    AimerApp, AnyElement, BuildContext, Button, Container, Direction, Resizable, ResolvedSize,
    SizedBox, State, StateUpdater, StatefulElement, StatefulWidget, Widget,
};

/// The side bar itself: an animated border that grows a stroke while the
/// resize handle is hovered, wrapped around a `Resizable` holding a button —
/// the same shape `AppSideBar` builds, with a long animation so every event
/// below lands while it is still running.
struct SideBarLike {
    presses: Rc<Cell<u32>>,
    resizes: Rc<RefCell<Vec<ResolvedSize>>>,
}

struct SideBarLikeState {
    resizable: bool,
    presses: Rc<Cell<u32>>,
    resizes: Rc<RefCell<Vec<ResolvedSize>>>,
    updater: StateUpdater<Self>,
}

impl Widget for SideBarLike {
    fn to_element(self, ctx: &BuildContext) -> AnyElement {
        let key = Widget::key(&self);
        StatefulElement::new_with_name(self, ctx, "SideBarLike", key).0.boxed()
    }
}

impl StatefulWidget for SideBarLike {
    type State = SideBarLikeState;

    fn create_state(self) -> Self::State {
        SideBarLikeState {
            resizable: false,
            presses: self.presses,
            resizes: self.resizes,
            updater: StateUpdater::new(),
        }
    }
}

impl State<SideBarLike> for SideBarLikeState {
    fn init_state(&mut self, updater: StateUpdater<Self>)
    where
        Self: Sized,
    {
        self.updater = updater;
    }

    fn build(&self, _ctx: &BuildContext) -> impl Widget {
        let zone_updater = self.updater.clone();
        let presses = self.presses.clone();
        let resizes = self.resizes.clone();

        ImplicitAnimatedBuilder::new(
            if self.resizable { 1.0 } else { 0.0 },
            // Long enough that every event below lands mid-animation.
            Duration::from_secs(30),
            Curve::EaseInOut,
            move |_delta| {
                let zone_updater = zone_updater.clone();
                let presses = presses.clone();
                let resizes = resizes.clone();

                Resizable::new()
                    .width(250.0)
                    .height(500.0)
                    .min_width(200.0)
                    .max_width(500.0)
                    .handle_thickness(10.0)
                    .handle_outset(6.0)
                    .direction(Direction::RIGHT)
                    .on_resize(move |size: ResolvedSize| resizes.borrow_mut().push(size))
                    .on_resize_zone(move |zone| {
                        zone_updater.set_state(move |state| {
                            state.resizable = zone != Direction::NONE;
                        });
                    })
                    .child(
                        Container::new()
                            .padding(LayoutSpacing::all(12.0))
                            .child(
                                Button::new()
                                    .on_press(move || presses.set(presses.get() + 1))
                                    .child(SizedBox::new().width(100.0).height(40.0)),
                            ),
                    )
            },
        )
    }
}

fn move_to<W: Widget + 'static>(app: &mut HeadlessAimerApp<W>, x: f64, y: f64) {
    app.send_window_event(WindowEvent::CursorMoved {
        device_id: DeviceId::dummy(),
        position: PhysicalPosition::new(x, y),
    });
}

fn press<W: Widget + 'static>(app: &mut HeadlessAimerApp<W>, state: ElementState) {
    app.send_window_event(WindowEvent::MouseInput {
        device_id: DeviceId::dummy(),
        state,
        button: MouseButton::Left,
    });
}

fn click<W: Widget + 'static>(app: &mut HeadlessAimerApp<W>, x: f64, y: f64) {
    // The recognizer merges two taps within 300ms into a double tap, so a
    // test click must stay clear of the previous one to count as a press.
    std::thread::sleep(Duration::from_millis(400));
    move_to(app, x, y);
    press(app, ElementState::Pressed);
    // A real click spans several redraws: draw the frame a window would draw
    // between the press and the release.
    app.render_frame();
    press(app, ElementState::Released);
}

#[test]
fn the_sidebar_child_keeps_receiving_events_while_the_resize_zone_animates() {
    let presses = Rc::new(Cell::new(0));
    let resizes = Rc::new(RefCell::new(Vec::new()));

    let mut app = AimerApp::start_headless(SideBarLike {
        presses: presses.clone(),
        resizes: resizes.clone(),
    });
    app.render_frame();

    // The baseline: the button inside the resizable works before any zone
    // change has animated.
    click(&mut app, 50.0, 30.0);
    assert_eq!(presses.get(), 1, "the baseline click never fired");

    // Hover the resize handle: the zone flips, the state rebuilds, and a long
    // animation starts — every pumped frame rebuilds the whole subtree with
    // fresh element identities.
    move_to(&mut app, 245.0, 250.0);
    app.pump_frames(3);

    // A click while the animation is running must still fire.
    click(&mut app, 50.0, 30.0);
    assert_eq!(
        presses.get(),
        2,
        "the click during the zone animation was swallowed"
    );

    // Drag the handle while the animation is running, with a frame between
    // each step the way a real drag spans redraws.
    move_to(&mut app, 245.0, 250.0);
    press(&mut app, ElementState::Pressed);
    app.render_frame();
    move_to(&mut app, 300.0, 250.0);
    app.render_frame();
    move_to(&mut app, 330.0, 250.0);
    app.render_frame();
    press(&mut app, ElementState::Released);

    // The width follows the pointer: 250 + (330 - 245).
    let final_width = resizes.borrow().last().map(|size| size.width);
    assert!(
        final_width.is_some_and(|width| (width - 335.0).abs() < 1.0),
        "the drag did not reach 335: {:?}",
        resizes.borrow()
    );

    // And the child still works after the drag.
    click(&mut app, 50.0, 30.0);
    assert_eq!(presses.get(), 3, "the click after the drag was swallowed");
}

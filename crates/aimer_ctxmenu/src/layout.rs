//! Where a context menu lands, and what a press on it means.
//!
//! Two shapes, because two input devices ask for two different things. A finger
//! has no cursor and hides what it touches, so the touch shape is the *pill*
//! every mobile system draws: a short row of verbs floating clear of the thing
//! it acts on, flipping underneath when there is no room above. A mouse points
//! at an exact spot and expects the menu to appear *there*, growing down and to
//! the right like every desktop menu since 1988 — the *list* shape.
//!
//! This module is those two shapes and nothing else: placement and hit testing
//! over plain rectangles, with no canvas, no window and no element, so the rules
//! above are asserted directly.

use aimer_attribute::{Bounds, Vec2d};
use aimer_events::pointer::PointerSource;

/// Height of one row of the pill, in logical pixels.
pub const PILL_HEIGHT: f32 = 40.0;

/// Horizontal padding around each label of the pill, in logical pixels.
pub const PILL_ITEM_PADDING: f32 = 16.0;

/// Size of the pill's labels, in logical pixels.
pub const PILL_FONT_SIZE: f32 = 15.0;

/// Distance kept between the anchor and the pill, in logical pixels.
pub const PILL_GAP: f32 = 10.0;

/// Height of one row of the list, in logical pixels.
pub const LIST_ROW_HEIGHT: f32 = 28.0;

/// Horizontal padding around each label of the list, in logical pixels.
pub const LIST_ITEM_PADDING: f32 = 14.0;

/// Size of the list's labels, in logical pixels.
pub const LIST_FONT_SIZE: f32 = 13.0;

/// Padding above the first row and below the last, in logical pixels.
pub const LIST_VERTICAL_PADDING: f32 = 5.0;

/// Narrowest a list may be, in logical pixels.
///
/// A menu whose width tracked its longest label alone would jump about between
/// openings; desktop menus have a floor for exactly that reason.
pub const LIST_MIN_WIDTH: f32 = 150.0;

/// Corner radius of the list panel, in logical pixels.
pub const LIST_RADIUS: f32 = 8.0;

/// Distance kept between the menu and the edge of the window, in logical
/// pixels.
pub const MENU_MARGIN: f32 = 8.0;

/// Which of the two shapes a menu is drawn in.
///
/// # Examples
///
/// ```
/// use aimer_ctxmenu::ContextMenuStyle;
/// use aimer_events::pointer::PointerSource;
///
/// assert_eq!(
///     ContextMenuStyle::for_source(PointerSource::Touch),
///     ContextMenuStyle::Pill
/// );
/// assert_eq!(
///     ContextMenuStyle::for_source(PointerSource::Mouse),
///     ContextMenuStyle::List
/// );
/// ```
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ContextMenuStyle {
    /// The horizontal pill a touch screen offers above the selection.
    #[default]
    Pill,
    /// The vertical list a desktop opens at the pointer.
    List,
}

impl ContextMenuStyle {
    /// The shape the pointer that opened the menu asks for.
    ///
    /// A finger gets the pill: it has no cursor to place a list under, and it
    /// hides the screen where it touches. A mouse gets the desktop list. The
    /// rule is the input device's, not the platform's, so a phone browser and a
    /// desktop browser each get the shape their user is actually holding.
    #[inline]
    pub const fn for_source(source: PointerSource) -> Self {
        match source {
            PointerSource::Mouse => Self::List,
            PointerSource::Touch => Self::Pill,
        }
    }

    /// Height of one row in this shape, in logical pixels.
    #[inline]
    pub const fn row_height(self) -> f32 {
        match self {
            Self::Pill => PILL_HEIGHT,
            Self::List => LIST_ROW_HEIGHT,
        }
    }

    /// Horizontal padding around a label in this shape, in logical pixels.
    #[inline]
    pub const fn item_padding(self) -> f32 {
        match self {
            Self::Pill => PILL_ITEM_PADDING,
            Self::List => LIST_ITEM_PADDING,
        }
    }

    /// Size of a label in this shape, in logical pixels.
    #[inline]
    pub const fn font_size(self) -> f32 {
        match self {
            Self::Pill => PILL_FONT_SIZE,
            Self::List => LIST_FONT_SIZE,
        }
    }

    /// Corner radius of the panel in this shape, in logical pixels.
    #[inline]
    pub const fn radius(self) -> f32 {
        match self {
            Self::Pill => PILL_HEIGHT * 0.5,
            Self::List => LIST_RADIUS,
        }
    }
}

/// What the menu is placed against.
///
/// # Examples
///
/// ```
/// use aimer_attribute::Vec2d;
/// use aimer_ctxmenu::ContextMenuAnchor;
///
/// let at_pointer = ContextMenuAnchor::Point(Vec2d { x: 40.0, y: 60.0 });
/// assert_eq!(at_pointer.rect().x, 40.0);
/// assert_eq!(at_pointer.rect().height, 0.0);
/// ```
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ContextMenuAnchor {
    /// A rectangle the menu keeps clear of — a selection, a chip, a row.
    Rect(Bounds),
    /// A single point the menu is pinned to — where a right-click landed.
    Point(Vec2d),
}

impl ContextMenuAnchor {
    /// The anchor as a rectangle; a point becomes an empty one.
    #[inline]
    pub const fn rect(self) -> Bounds {
        match self {
            Self::Rect(bounds) => bounds,
            Self::Point(pos) => Bounds::new(pos.x, pos.y, 0.0, 0.0),
        }
    }
}

/// Where the panel and each of its items landed, in absolute logical
/// coordinates.
///
/// # Examples
///
/// ```
/// use aimer_attribute::{Bounds, Vec2d};
/// use aimer_ctxmenu::{ContextMenuAnchor, ContextMenuLayout, ContextMenuStyle};
///
/// let layout = ContextMenuLayout::place(
///     ContextMenuStyle::List,
///     ContextMenuAnchor::Point(Vec2d { x: 40.0, y: 60.0 }),
///     400.0,
///     800.0,
///     &[52.0, 80.0],
/// );
///
/// assert_eq!(layout.bounds.x, 40.0);
/// assert_eq!(layout.items.len(), 2);
/// assert_eq!(layout.action_at(45.0, layout.items[0].y + 2.0), Some(0));
/// ```
#[derive(Clone, Debug, PartialEq)]
pub struct ContextMenuLayout {
    /// The shape the panel was placed in.
    pub style: ContextMenuStyle,
    /// The panel itself.
    pub bounds: Bounds,
    /// One rectangle per item, in the order the items were given.
    pub items: Vec<Bounds>,
    /// `true` when there was no room in the preferred direction and the panel
    /// was flipped: above the anchor becomes below it for a pill, below the
    /// pointer becomes above it for a list.
    pub flipped: bool,
}

impl ContextMenuLayout {
    /// Places a panel holding items of `item_widths` against `anchor`, inside a
    /// window of `viewport_width` by `viewport_height` logical pixels.
    ///
    /// `item_widths` are the measured label widths; the padding around each one
    /// is added here so the caller only has to measure text.
    pub fn place(
        style: ContextMenuStyle,
        anchor: ContextMenuAnchor,
        viewport_width: f32,
        viewport_height: f32,
        item_widths: &[f32],
    ) -> Self {
        match style {
            ContextMenuStyle::Pill => {
                Self::place_pill(anchor.rect(), viewport_width, viewport_height, item_widths)
            }
            ContextMenuStyle::List => {
                Self::place_list(anchor.rect(), viewport_width, viewport_height, item_widths)
            }
        }
    }

    fn place_pill(
        anchor: Bounds,
        viewport_width: f32,
        viewport_height: f32,
        item_widths: &[f32],
    ) -> Self {
        let widths = item_widths
            .iter()
            .map(|width| width + PILL_ITEM_PADDING * 2.0)
            .collect::<Vec<_>>();
        let width = widths.iter().sum::<f32>();

        let above = anchor.y - PILL_GAP - PILL_HEIGHT;
        let below = anchor.y + anchor.height + PILL_GAP;
        let flipped = above < MENU_MARGIN;
        let y = clamp_span(
            if flipped { below } else { above },
            PILL_HEIGHT,
            viewport_height,
        );
        let x = clamp_span(
            anchor.x + anchor.width * 0.5 - width * 0.5,
            width,
            viewport_width,
        );

        let mut items = Vec::with_capacity(widths.len());
        let mut cursor = x;
        for item_width in widths {
            items.push(Bounds::new(cursor, y, item_width, PILL_HEIGHT));
            cursor += item_width;
        }

        Self {
            style: ContextMenuStyle::Pill,
            bounds: Bounds::new(x, y, width, PILL_HEIGHT),
            items,
            flipped,
        }
    }

    fn place_list(
        anchor: Bounds,
        viewport_width: f32,
        viewport_height: f32,
        item_widths: &[f32],
    ) -> Self {
        if item_widths.is_empty() {
            return Self {
                style: ContextMenuStyle::List,
                bounds: Bounds::new(anchor.x, anchor.y, 0.0, 0.0),
                items: Vec::new(),
                flipped: false,
            };
        }

        let width = item_widths
            .iter()
            .fold(LIST_MIN_WIDTH, |widest, label| {
                widest.max(label + LIST_ITEM_PADDING * 2.0)
            })
            .min((viewport_width - MENU_MARGIN * 2.0).max(0.0));
        let height = LIST_ROW_HEIGHT * item_widths.len() as f32 + LIST_VERTICAL_PADDING * 2.0;

        // The list grows down and to the right from the pointer, and flips back
        // over it — never merely slides — when that would leave the window: a
        // menu sliding up under the cursor would open with an item already
        // beneath it, ready to be triggered by the release of the very click
        // that opened it.
        let left = anchor.x;
        let top = anchor.y + anchor.height;
        let flipped = top + height > viewport_height - MENU_MARGIN;
        let y = clamp_span(
            if flipped { anchor.y - height } else { top },
            height,
            viewport_height,
        );
        let x = clamp_span(
            if left + width > viewport_width - MENU_MARGIN {
                anchor.x + anchor.width - width
            } else {
                left
            },
            width,
            viewport_width,
        );

        let mut items = Vec::with_capacity(item_widths.len());
        let mut cursor = y + LIST_VERTICAL_PADDING;
        for _ in item_widths {
            items.push(Bounds::new(x, cursor, width, LIST_ROW_HEIGHT));
            cursor += LIST_ROW_HEIGHT;
        }

        Self {
            style: ContextMenuStyle::List,
            bounds: Bounds::new(x, y, width, height),
            items,
            flipped,
        }
    }

    /// Whether `(x, y)` landed on the panel at all.
    ///
    /// A press on the panel belongs to the menu even where it hit no item, or
    /// it would fall through to whatever the menu is covering.
    #[inline]
    pub fn contains(&self, x: f32, y: f32) -> bool {
        !self.items.is_empty() && contains(self.bounds, x, y)
    }

    /// The index of the item under `(x, y)`, if the press landed on one.
    ///
    /// The pill resolves a press on its rounded corners to the nearest item,
    /// because its items tile it edge to edge and the corner is plainly part of
    /// the button beside it. The list does not: its padding is deliberate empty
    /// space, and a press there must run nothing.
    pub fn action_at(&self, x: f32, y: f32) -> Option<usize> {
        if !self.contains(x, y) {
            return None;
        }
        let hit = self.items.iter().position(|item| contains(*item, x, y));
        match self.style {
            ContextMenuStyle::Pill => hit.or(Some(if x <= self.bounds.x {
                0
            } else {
                self.items.len() - 1
            })),
            ContextMenuStyle::List => hit,
        }
    }
}

/// Keeps a span of `length` inside `viewport`, honouring the margin on both
/// sides but never pushing it off the near edge to satisfy the far one.
#[inline]
fn clamp_span(start: f32, length: f32, viewport: f32) -> f32 {
    start
        .min((viewport - length - MENU_MARGIN).max(MENU_MARGIN))
        .max(MENU_MARGIN)
}

#[inline]
fn contains(bounds: Bounds, x: f32, y: f32) -> bool {
    x >= bounds.x
        && x <= bounds.x + bounds.width
        && y >= bounds.y
        && y <= bounds.y + bounds.height
}

#[cfg(test)]
mod tests {
    use super::*;

    const VIEWPORT: (f32, f32) = (400.0, 800.0);

    fn pill(anchor: Bounds) -> ContextMenuLayout {
        ContextMenuLayout::place(
            ContextMenuStyle::Pill,
            ContextMenuAnchor::Rect(anchor),
            VIEWPORT.0,
            VIEWPORT.1,
            &[40.0, 70.0],
        )
    }

    fn list(x: f32, y: f32) -> ContextMenuLayout {
        ContextMenuLayout::place(
            ContextMenuStyle::List,
            ContextMenuAnchor::Point(Vec2d { x, y }),
            VIEWPORT.0,
            VIEWPORT.1,
            &[40.0, 70.0],
        )
    }

    #[test]
    fn the_pill_floats_above_the_anchor_and_is_centred_on_it() {
        let layout = pill(Bounds::new(100.0, 300.0, 100.0, 20.0));

        assert!(!layout.flipped);
        assert_eq!(layout.bounds.y, 300.0 - PILL_GAP - PILL_HEIGHT);
        assert_eq!(layout.bounds.width, 40.0 + 70.0 + PILL_ITEM_PADDING * 4.0);
        assert_eq!(
            layout.bounds.x + layout.bounds.width * 0.5,
            150.0,
            "centred on the anchor"
        );
    }

    #[test]
    fn an_anchor_at_the_top_flips_the_pill_underneath_it() {
        let layout = pill(Bounds::new(100.0, 4.0, 100.0, 20.0));

        assert!(layout.flipped);
        assert_eq!(layout.bounds.y, 4.0 + 20.0 + PILL_GAP);
    }

    #[test]
    fn the_pill_never_leaves_the_window() {
        let left = pill(Bounds::new(0.0, 300.0, 4.0, 20.0));
        assert_eq!(left.bounds.x, MENU_MARGIN);

        let right = pill(Bounds::new(396.0, 300.0, 4.0, 20.0));
        assert_eq!(
            right.bounds.x + right.bounds.width,
            VIEWPORT.0 - MENU_MARGIN
        );

        let bottom = pill(Bounds::new(100.0, 790.0, 100.0, 20.0));
        assert!(bottom.bounds.y + PILL_HEIGHT <= VIEWPORT.1 - MENU_MARGIN);
    }

    #[test]
    fn the_pill_items_tile_it_in_order() {
        let layout = pill(Bounds::new(100.0, 300.0, 100.0, 20.0));

        assert_eq!(layout.items.len(), 2);
        assert_eq!(layout.items[0].x, layout.bounds.x);
        assert_eq!(
            layout.items[0].x + layout.items[0].width,
            layout.items[1].x,
            "no gap between the items"
        );
        assert_eq!(
            layout.items[1].x + layout.items[1].width,
            layout.bounds.x + layout.bounds.width
        );
    }

    #[test]
    fn a_press_resolves_to_the_pill_item_it_landed_on() {
        let layout = pill(Bounds::new(100.0, 300.0, 100.0, 20.0));
        let y = layout.bounds.y + PILL_HEIGHT * 0.5;

        assert_eq!(layout.action_at(layout.items[0].x + 2.0, y), Some(0));
        assert_eq!(layout.action_at(layout.items[1].x + 2.0, y), Some(1));
    }

    #[test]
    fn a_press_outside_the_pill_resolves_to_nothing() {
        let layout = pill(Bounds::new(100.0, 300.0, 100.0, 20.0));

        assert_eq!(layout.action_at(0.0, 0.0), None);
        assert_eq!(
            layout.action_at(layout.bounds.x + 1.0, layout.bounds.y + 1.0),
            Some(0),
            "but a press on its very corner is still the pill's"
        );
    }

    #[test]
    fn an_empty_menu_places_nothing_and_swallows_nothing() {
        for style in [ContextMenuStyle::Pill, ContextMenuStyle::List] {
            let layout = ContextMenuLayout::place(
                style,
                ContextMenuAnchor::Rect(Bounds::new(100.0, 300.0, 100.0, 20.0)),
                400.0,
                800.0,
                &[],
            );

            assert!(layout.items.is_empty());
            assert_eq!(layout.bounds.width, 0.0);
            assert_eq!(layout.action_at(layout.bounds.x, layout.bounds.y), None);
        }
    }

    #[test]
    fn the_list_opens_with_its_corner_at_the_pointer() {
        let layout = list(40.0, 60.0);

        assert!(!layout.flipped);
        assert_eq!(layout.bounds.x, 40.0);
        assert_eq!(layout.bounds.y, 60.0);
        assert_eq!(layout.bounds.width, LIST_MIN_WIDTH, "a floor on the width");
        assert_eq!(
            layout.bounds.height,
            LIST_ROW_HEIGHT * 2.0 + LIST_VERTICAL_PADDING * 2.0
        );
    }

    #[test]
    fn the_list_rows_stack_downwards_inside_the_panel() {
        let layout = list(40.0, 60.0);

        assert_eq!(layout.items.len(), 2);
        assert_eq!(layout.items[0].y, layout.bounds.y + LIST_VERTICAL_PADDING);
        assert_eq!(
            layout.items[1].y,
            layout.items[0].y + LIST_ROW_HEIGHT,
            "the rows are stacked, not tiled sideways"
        );
        assert_eq!(layout.items[0].width, layout.bounds.width);
    }

    #[test]
    fn a_list_opened_near_the_bottom_flips_above_the_pointer() {
        let layout = list(40.0, 790.0);

        assert!(layout.flipped);
        assert_eq!(
            layout.bounds.y + layout.bounds.height,
            790.0,
            "it grows upwards from the pointer instead of covering it"
        );
    }

    #[test]
    fn a_list_opened_near_the_right_edge_grows_leftwards() {
        let layout = list(380.0, 60.0);

        assert_eq!(
            layout.bounds.x + layout.bounds.width,
            380.0,
            "its right edge meets the pointer"
        );
    }

    #[test]
    fn the_list_never_leaves_the_window() {
        let layout = list(2.0, 2.0);

        assert!(layout.bounds.x >= MENU_MARGIN);
        assert!(layout.bounds.y >= MENU_MARGIN);
    }

    #[test]
    fn a_press_in_the_lists_padding_runs_nothing_but_is_still_the_menus() {
        let layout = list(40.0, 60.0);
        let (x, y) = (layout.bounds.x + 4.0, layout.bounds.y + 1.0);

        assert!(layout.contains(x, y), "the panel swallows it");
        assert_eq!(layout.action_at(x, y), None, "but it runs nothing");
        assert_eq!(
            layout.action_at(x, layout.items[1].y + 2.0),
            Some(1),
            "while a press on a row is that row's"
        );
    }

    #[test]
    fn the_shape_follows_the_pointer_that_opened_the_menu() {
        assert_eq!(
            ContextMenuStyle::for_source(PointerSource::Touch),
            ContextMenuStyle::Pill
        );
        assert_eq!(
            ContextMenuStyle::for_source(PointerSource::Mouse),
            ContextMenuStyle::List
        );
    }
}

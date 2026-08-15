use std::any::TypeId;

use aimer_container::{AspectRatio, Container, Opacity, SizedBox, ZeroSizedBox};
use aimer_flex::{BoxAlignment, Column, Expanded, Flex, FlexDirection, OverflowBehavior, Row};
use aimer_grid::{
    Grid, GridAlignment, GridError, GridItem, GridOverflow, GridPlacement, GridTrack,
};
use aimer_scroll::{DragMode, ScrollAxis, ScrollBar, ScrollBehavior, ScrollController, Scrollable};
use aimer_space::{Align, Alignment, Positioned, Stack};

#[test]
fn split_crates_expose_their_widget_families() {
    let _ = TypeId::of::<AspectRatio>();
    let _ = TypeId::of::<Container>();
    let _ = TypeId::of::<Opacity>();
    let _ = TypeId::of::<SizedBox>();
    let _ = TypeId::of::<ZeroSizedBox>();

    let _ = TypeId::of::<BoxAlignment>();
    let _ = TypeId::of::<Column>();
    let _ = TypeId::of::<Expanded>();
    let _ = TypeId::of::<Flex>();
    let _ = TypeId::of::<FlexDirection>();
    let _ = TypeId::of::<OverflowBehavior>();
    let _ = TypeId::of::<Row>();

    let _ = TypeId::of::<Grid>();
    let _ = TypeId::of::<GridAlignment>();
    let _ = TypeId::of::<GridError>();
    let _ = TypeId::of::<GridItem<ZeroSizedBox>>();
    let _ = TypeId::of::<GridOverflow>();
    let _ = TypeId::of::<GridPlacement>();
    let _ = TypeId::of::<GridTrack>();

    let _ = TypeId::of::<DragMode>();
    let _ = TypeId::of::<ScrollAxis>();
    let _ = TypeId::of::<ScrollBar>();
    let _ = TypeId::of::<ScrollBehavior>();
    let _ = TypeId::of::<ScrollController>();
    let _ = TypeId::of::<Scrollable>();

    let _ = TypeId::of::<Align>();
    let _ = TypeId::of::<Alignment>();
    let _ = TypeId::of::<Positioned>();
    let _ = TypeId::of::<Stack>();
}

#[test]
fn split_crates_preserve_public_submodule_paths() {
    let _ = TypeId::of::<aimer_flex::row_column::Row>();
    let _ = TypeId::of::<aimer_scroll::controller::ScrollController>();
    let _ = TypeId::of::<aimer_space::align::Alignment>();
}

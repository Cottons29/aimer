use aimer_laboratory::v2_render::{CounterSimulation, RenderOp};
use aimer_cupid::damage_region::DamageSet;
use aimer_cupid::draw_cmd::DrawCommand as LegacyDrawCommand;
use aimer_cupid::frame::{FramePacket, FrameRenderMetadata};

#[test]
fn state_update_relayouts_both_row_children_and_rerecords_only_the_counter() {
    let mut simulation = CounterSimulation::new().unwrap();
    let elements = simulation.element_ids();
    let initial_bounds = simulation.counter_row_bounds().unwrap();
    assert_eq!(initial_bounds[0].width, 128.0);
    assert_eq!(initial_bounds[1].x, 508.0);

    assert!(simulation.dispatch_pointer_down((100.0, 100.0)).unwrap().is_none());
    assert_eq!(simulation.count(), 9);

    let frame = simulation
        .dispatch_pointer_down((400.0, 280.0))
        .unwrap()
        .unwrap();
    let updated_bounds = simulation.counter_row_bounds().unwrap();

    assert_eq!(simulation.count(), 10);
    assert_eq!(simulation.handled_pointer_events(), 1);
    assert_eq!(updated_bounds[0].width, 142.0);
    assert_eq!(updated_bounds[1].x, 522.0);
    assert_eq!(frame.damage.len(), 1);
    assert_eq!(frame.damage[0].x, 372.0);
    assert_eq!(frame.damage[0].y, 262.0);
    assert_eq!(frame.damage[0].width, 214.0);
    assert_eq!(
        elements
            .iter()
            .map(|element| simulation.tree().draw_list_revision(*element).unwrap())
            .collect::<Vec<_>>(),
        [1, 1, 1, 2, 1]
    );

    let draw_order = frame
        .operations
        .iter()
        .filter_map(|operation| match operation {
            RenderOp::Draw(item) => Some(item.element),
            RenderOp::BeginOpacityGroup { .. } | RenderOp::EndOpacityGroup { .. } => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(draw_order, elements);
}

#[test]
fn counter_simulation_lowers_initial_and_incremental_frames_into_packets() {
    let simulation = CounterSimulation::new().unwrap();
    let metadata = || {
        FrameRenderMetadata::new(1.0, 17, 2, 3, 4, DamageSet::new(1000, 800))
    };

    let initial = FramePacket::from_v2(simulation.initial_frame().unwrap(), metadata()).unwrap();
    assert_eq!((initial.frame().width, initial.frame().height), (1000, 800));
    assert!(initial.metadata().damage().is_full());
    assert_eq!(
        initial
            .frame()
            .draw_list
            .commands()
            .iter()
            .filter(|command| matches!(command, LegacyDrawCommand::FillRect { .. }))
            .count(),
        3
    );

    let updated = simulation.increment().unwrap();
    let packet = FramePacket::from_v2(updated, metadata()).unwrap();
    assert_eq!(
        packet.metadata().damage().regions(),
        &[aimer_cupid::damage_region::DamageRect::new(372, 262, 214, 48)]
    );
    assert!(packet
        .frame()
        .draw_list
        .commands()
        .iter()
        .any(|command| matches!(command, LegacyDrawCommand::DrawText { text, .. } if &**text == "Count: 10")));
}

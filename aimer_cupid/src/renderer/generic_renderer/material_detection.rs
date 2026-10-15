//! Detect backdrop consumers before allocating an offscreen frame target.

use crate::draw_cmd::{DrawCommand, DrawList};
use crate::frame::RetainedRenderPlan;
use crate::pipeline::material::{
    MATERIAL_PACKET_LEN, MATERIAL_PACKET_VERSION, MATERIAL_PIPELINE_NAME, MaterialKind,
    MaterialRequest,
};

fn packet_needs_backdrop(bytes: &[u8]) -> bool {
    // Inspect only the fixed header on this per-frame path. Full decoding and
    // validation happen when preparing the command. Unknown packets retain the
    // conservative capture route used before analytic materials were added.
    bytes.len() != MATERIAL_PACKET_LEN
        || !bytes.starts_with(b"AIMR")
        || bytes[4] != MATERIAL_PACKET_VERSION
        || bytes[5] != MaterialKind::LightEdge as u8
}

pub(super) fn draw_list_uses_material(draw_list: &DrawList) -> bool {
    draw_list.commands().iter().any(|command| {
        let DrawCommand::Custom {
            pipeline_name,
            data,
        } = command
        else {
            return false;
        };
        if pipeline_name != MATERIAL_PIPELINE_NAME {
            return false;
        }
        if let Some(request) = data.downcast_ref::<MaterialRequest>() {
            return request.kind != MaterialKind::LightEdge;
        }
        data.downcast_ref::<Vec<u8>>()
            .is_none_or(|bytes| packet_needs_backdrop(bytes))
    })
}

pub(super) fn render_plan_uses_material(
    plan: &RetainedRenderPlan,
    width: u32,
    height: u32,
) -> bool {
    plan.operations_for_region(crate::damage_region::DamageRect::new(0, 0, width, height))
        .any(|operation| match &operation.kind {
            crate::frame::RetainedRenderOperationKind::LocalV2(item) => item.commands.iter().any(
                |command| matches!(command,
                    crate::draw_cmd_v2::DrawCommand::DrawCustom { pipeline_name, data }
                        if pipeline_name.as_ref() == MATERIAL_PIPELINE_NAME && packet_needs_backdrop(data)),
            ),
            crate::frame::RetainedRenderOperationKind::OpacityGroupBegin { .. }
            | crate::frame::RetainedRenderOperationKind::OpacityGroupEnd { .. } => false,
        })
}

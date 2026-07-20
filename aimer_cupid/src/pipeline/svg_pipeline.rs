use std::collections::{HashMap, HashSet};
use std::ops::Range;
use std::sync::Arc;

use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt;

use super::image_pipeline::InstanceBufferPolicy;
use crate::renderer::SvgRenderItem;
use crate::svg::{
    SvgColor, SvgGeometryCache, SvgMesh, SvgMeshStyle, SvgNode, SvgNodeStyleOverride, SvgPaintOrder,
};
use crate::utilities::Mat3;

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
struct SvgVertex {
    position: [f32; 2],
}

impl SvgVertex {
    const ATTRIBUTES: [wgpu::VertexAttribute; 1] = wgpu::vertex_attr_array![0 => Float32x2];

    fn layout() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout { array_stride: size_of::<Self>() as wgpu::BufferAddress,
                                   step_mode: wgpu::VertexStepMode::Vertex,
                                   attributes: &Self::ATTRIBUTES }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
struct SvgInstance {
    transform_x: [f32; 4],
    transform_y: [f32; 4],
    color: [f32; 4],
    clip_rect: [f32; 4],
    clip_border_radius: [f32; 4],
    viewport: [f32; 4],
}

impl SvgInstance {
    const ATTRIBUTES: [wgpu::VertexAttribute; 6] = wgpu::vertex_attr_array![
        1 => Float32x4,
        2 => Float32x4,
        3 => Float32x4,
        4 => Float32x4,
        5 => Float32x4,
        6 => Float32x4,
    ];

    fn layout() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout { array_stride: size_of::<Self>() as wgpu::BufferAddress,
                                   step_mode: wgpu::VertexStepMode::Instance,
                                   attributes: &Self::ATTRIBUTES }
    }
}

struct GpuMesh {
    _mesh: Arc<SvgMesh>,
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    index_count: u32,
    bytes: u64,
    last_used: u64,
}

struct PreparedDraw {
    mesh_key: usize,
    instance_index: u32,
}

pub struct SvgPipeline {
    pipeline: wgpu::RenderPipeline,
    geometry_cache: SvgGeometryCache,
    gpu_meshes: HashMap<usize, GpuMesh>,
    prepared_draws: Vec<PreparedDraw>,
    item_ranges: Vec<Range<usize>>,
    instances: Vec<SvgInstance>,
    instance_buffer: wgpu::Buffer,
    instance_policy: InstanceBufferPolicy,
    gpu_mesh_bytes: u64,
    usage_clock: u64,
    max_gpu_mesh_bytes: u64,
    max_gpu_meshes: usize,
}

impl SvgPipeline {
    const INITIAL_INSTANCE_CAPACITY: usize = 64;
    const MAX_CPU_MESH_BYTES: usize = 32 * 1024 * 1024;
    const MAX_CPU_MESHES: usize = 4096;
    const MAX_GPU_MESH_BYTES: u64 = 64 * 1024 * 1024;
    const MAX_GPU_MESHES: usize = 4096;

    pub fn new(device: &wgpu::Device,
               format: wgpu::TextureFormat,
               pipeline_cache: Option<&wgpu::PipelineCache>)
               -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("svg shader"),
            source: wgpu::ShaderSource::Wgsl(Self::shader_source().into()),
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("svg pipeline layout"),
            bind_group_layouts: &[],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("svg pipeline"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[Some(SvgVertex::layout()), Some(SvgInstance::layout())],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: Self::multisample_state(),
            multiview_mask: None,
            cache: pipeline_cache,
        });
        let instance_buffer = create_instance_buffer(device, Self::INITIAL_INSTANCE_CAPACITY);
        Self { pipeline,
               geometry_cache: SvgGeometryCache::new(Self::MAX_CPU_MESH_BYTES,
                                                     Self::MAX_CPU_MESHES),
               gpu_meshes: HashMap::new(),
               prepared_draws: Vec::new(),
               item_ranges: Vec::new(),
               instances: Vec::new(),
               instance_buffer,
               instance_policy: InstanceBufferPolicy::new(Self::INITIAL_INSTANCE_CAPACITY),
               gpu_mesh_bytes: 0,
               usage_clock: 0,
               max_gpu_mesh_bytes: Self::MAX_GPU_MESH_BYTES,
               max_gpu_meshes: Self::MAX_GPU_MESHES }
    }

    #[inline]
    fn shader_source() -> &'static str {
        #[cfg(target_os = "android")]
        {
            concat!(include_str!("./shaders/android_color.wgsl"),
                    include_str!("./shaders/svg.wgsl"))
        }
        #[cfg(not(target_os = "android"))]
        {
            concat!(include_str!("./shaders/color.wgsl"), include_str!("./shaders/svg.wgsl"))
        }
    }

    fn multisample_state() -> wgpu::MultisampleState {
        crate::pipeline::multisample_state()
    }

    pub fn prepare(&mut self,
                   device: &wgpu::Device,
                   queue: &wgpu::Queue,
                   items: &[SvgRenderItem],
                   width: u32,
                   height: u32,
                   is_srgb: bool) {
        self.usage_clock = self.usage_clock
                               .wrapping_add(1);
        self.prepared_draws
            .clear();
        self.item_ranges
            .clear();
        self.instances
            .clear();
        let mut frame_meshes = HashSet::new();
        for item in items {
            let range_start = self.prepared_draws
                                  .len();
            if item.opacity > 0.0
               && item.destination
                      .width
                  > 0.0
               && item.destination
                      .height
                  > 0.0
            {
                self.prepare_item(device, item, width, height, is_srgb, &mut frame_meshes);
            }
            self.item_ranges
                .push(range_start
                      ..self.prepared_draws
                            .len());
        }

        let old_capacity = self.instance_policy
                               .capacity();
        self.instance_policy
            .record_usage(self.instances.len());
        if old_capacity
           != self.instance_policy
                  .capacity()
        {
            self.instance_buffer = create_instance_buffer(device,
                                                          self.instance_policy
                                                              .capacity());
        }
        if !self.instances
                .is_empty()
        {
            queue.write_buffer(&self.instance_buffer, 0, bytemuck::cast_slice(&self.instances));
        }
        self.evict_gpu_meshes(&frame_meshes);
    }

    fn prepare_item(&mut self,
                    device: &wgpu::Device,
                    item: &SvgRenderItem,
                    width: u32,
                    height: u32,
                    is_srgb: bool,
                    frame_meshes: &mut HashSet<usize>) {
        for node in item.scene
                        .nodes
                        .iter()
                        .filter(|node| {
                            node.visible
                            && node.geometry
                                   .is_some()
                        })
        {
            let Some(geometry) = item.scene
                                     .geometry(node)
            else {
                continue;
            };
            let node_override = item.overrides
                                    .iter()
                                    .find(|value| value.node_id == node.node_id);
            let transform = combined_transform(item, node, node_override);
            if outside_viewport(transform, geometry, width, height) {
                continue;
            }
            let physical_scale = transform_scale(transform);
            let opacity = item.opacity
                          * node_override.and_then(|value| value.opacity)
                                         .unwrap_or(node.opacity);
            if opacity <= 0.0 {
                continue;
            }
            let fill = resolved_fill(node, node_override);
            let stroke = resolved_stroke(node, node_override);
            match node.paint_order {
                SvgPaintOrder::FillAndStroke => {
                    if let Some((color, style)) = fill {
                        self.prepare_mesh(device,
                                          geometry,
                                          style,
                                          transform,
                                          color,
                                          opacity,
                                          item,
                                          width,
                                          height,
                                          is_srgb,
                                          physical_scale,
                                          frame_meshes);
                    }
                    if let Some((color, style)) = stroke {
                        self.prepare_mesh(device,
                                          geometry,
                                          style,
                                          transform,
                                          color,
                                          opacity,
                                          item,
                                          width,
                                          height,
                                          is_srgb,
                                          physical_scale,
                                          frame_meshes);
                    }
                }
                SvgPaintOrder::StrokeAndFill => {
                    if let Some((color, style)) = stroke {
                        self.prepare_mesh(device,
                                          geometry,
                                          style,
                                          transform,
                                          color,
                                          opacity,
                                          item,
                                          width,
                                          height,
                                          is_srgb,
                                          physical_scale,
                                          frame_meshes);
                    }
                    if let Some((color, style)) = fill {
                        self.prepare_mesh(device,
                                          geometry,
                                          style,
                                          transform,
                                          color,
                                          opacity,
                                          item,
                                          width,
                                          height,
                                          is_srgb,
                                          physical_scale,
                                          frame_meshes);
                    }
                }
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn prepare_mesh(&mut self,
                    device: &wgpu::Device,
                    geometry: &crate::svg::SvgGeometry,
                    style: SvgMeshStyle,
                    transform: Mat3,
                    color: SvgColor,
                    opacity: f32,
                    item: &SvgRenderItem,
                    width: u32,
                    height: u32,
                    is_srgb: bool,
                    physical_scale: f32,
                    frame_meshes: &mut HashSet<usize>) {
        let Ok(mesh) = self.geometry_cache
                           .mesh_for(geometry, style, physical_scale)
        else {
            return;
        };
        if mesh.indices
               .is_empty()
        {
            return;
        }
        let mesh_key = Arc::as_ptr(&mesh) as usize;
        frame_meshes.insert(mesh_key);
        self.ensure_gpu_mesh(device, mesh_key, &mesh);
        let instance_index = self.instances.len() as u32;
        self.instances
            .push(SvgInstance { transform_x: [transform.cols[0][0],
                                              transform.cols[1][0],
                                              transform.cols[2][0],
                                              0.0],
                                transform_y: [transform.cols[0][1],
                                              transform.cols[1][1],
                                              transform.cols[2][1],
                                              0.0],
                                color: [color.r, color.g, color.b, color.a * opacity],
                                clip_rect: item.clip_rect,
                                clip_border_radius: item.clip_border_radius,
                                viewport: [width as f32,
                                           height as f32,
                                           surface_srgb_value(is_srgb),
                                           0.0] });
        self.prepared_draws
            .push(PreparedDraw { mesh_key, instance_index });
    }

    fn ensure_gpu_mesh(&mut self, device: &wgpu::Device, key: usize, mesh: &Arc<SvgMesh>) {
        if let Some(entry) = self.gpu_meshes
                                 .get_mut(&key)
        {
            entry.last_used = self.usage_clock;
            return;
        }
        let vertices = mesh.vertices
                           .iter()
                           .copied()
                           .map(|position| SvgVertex { position })
                           .collect::<Vec<_>>();
        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("svg vertex buffer"),
            contents: bytemuck::cast_slice(&vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("svg index buffer"),
            contents: bytemuck::cast_slice(&mesh.indices),
            usage: wgpu::BufferUsages::INDEX,
        });
        let bytes = mesh.memory_bytes() as u64;
        self.gpu_mesh_bytes += bytes;
        self.gpu_meshes
            .insert(key,
                    GpuMesh { _mesh: mesh.clone(),
                              vertex_buffer,
                              index_buffer,
                              index_count: mesh.indices.len() as u32,
                              bytes,
                              last_used: self.usage_clock });
    }

    fn evict_gpu_meshes(&mut self, frame_meshes: &HashSet<usize>) {
        while self.gpu_meshes
                  .len()
              > self.max_gpu_meshes
              || self.gpu_mesh_bytes > self.max_gpu_mesh_bytes
        {
            let Some(key) = self.gpu_meshes
                                .iter()
                                .filter(|(key, _)| !frame_meshes.contains(key))
                                .min_by_key(|(_, mesh)| mesh.last_used)
                                .map(|(key, _)| *key)
            else {
                break;
            };
            if let Some(mesh) = self.gpu_meshes
                                    .remove(&key)
            {
                self.gpu_mesh_bytes = self.gpu_mesh_bytes
                                          .saturating_sub(mesh.bytes);
            }
        }
    }

    pub fn draw_item<'a>(&'a self, pass: &mut wgpu::RenderPass<'a>, item_index: usize) {
        let Some(range) = self.item_ranges
                              .get(item_index)
        else {
            return;
        };
        pass.set_pipeline(&self.pipeline);
        for draw in &self.prepared_draws[range.clone()] {
            let Some(mesh) = self.gpu_meshes
                                 .get(&draw.mesh_key)
            else {
                continue;
            };
            pass.set_vertex_buffer(0,
                                   mesh.vertex_buffer
                                       .slice(..));
            pass.set_vertex_buffer(1,
                                   self.instance_buffer
                                       .slice(..));
            pass.set_index_buffer(mesh.index_buffer
                                      .slice(..),
                                  wgpu::IndexFormat::Uint32);
            pass.draw_indexed(0..mesh.index_count, 0, draw.instance_index..draw.instance_index + 1);
        }
    }

    pub fn cpu_geometry_bytes(&self) -> u64 {
        self.geometry_cache
            .memory_bytes() as u64
    }

    pub fn gpu_geometry_bytes(&self) -> u64 {
        self.gpu_mesh_bytes
    }

    pub fn instance_buffer_bytes(&self) -> u64 {
        (self.instance_policy
             .capacity()
         * size_of::<SvgInstance>()) as u64
    }

    pub fn clear_resources(&mut self) {
        self.geometry_cache
            .clear();
        self.gpu_meshes
            .clear();
        self.gpu_mesh_bytes = 0;
    }
}

fn create_instance_buffer(device: &wgpu::Device, capacity: usize) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor { label: Some("svg instance buffer"),
                                                   size: (capacity.max(1)
                                                          * size_of::<SvgInstance>())
                                                         as u64,
                                                   usage: wgpu::BufferUsages::VERTEX
                                                          | wgpu::BufferUsages::COPY_DST,
                                                   mapped_at_creation: false })
}

fn combined_transform(item: &SvgRenderItem,
                      node: &SvgNode,
                      node_override: Option<&SvgNodeStyleOverride>)
                      -> Mat3 {
    let viewport = item.scene.viewport;
    let destination = Mat3::translate(item.destination.x, item.destination.y).mul(&Mat3::scale(
        item.destination
            .width
            / viewport
                .width
                .max(f32::EPSILON),
        item.destination
            .height
            / viewport
                .height
                .max(f32::EPSILON),
    ));
    let transform = node_override.and_then(|value| value.transform)
                                 .unwrap_or(node.transform);
    let node_transform = Mat3 { cols: [[transform.sx, transform.ky, 0.0],
                                       [transform.kx, transform.sy, 0.0],
                                       [transform.tx, transform.ty, 1.0]] };
    item.world_transform
        .mul(&destination)
        .mul(&node_transform)
}

fn resolved_fill(node: &SvgNode,
                 node_override: Option<&SvgNodeStyleOverride>)
                 -> Option<(SvgColor, SvgMeshStyle)> {
    let fill = match node_override.map(|value| value.fill) {
        Some(Some(Some(color))) => Some((color,
                                         node.fill
                                             .as_ref()
                                             .map(|fill| fill.rule)
                                             .unwrap_or(crate::svg::SvgFillRule::NonZero))),
        Some(Some(None)) => None,
        Some(None) | None => node.fill
                                 .as_ref()
                                 .map(|fill| (fill.color, fill.rule)),
    }?;
    Some((fill.0, SvgMeshStyle::Fill(fill.1)))
}

fn resolved_stroke(node: &SvgNode,
                   node_override: Option<&SvgNodeStyleOverride>)
                   -> Option<(SvgColor, SvgMeshStyle)> {
    let stroke = node.stroke
                     .as_ref()?;
    if !stroke.dash_array
              .is_empty()
    {
        return None;
    }
    let color = match node_override.map(|value| value.stroke) {
        Some(Some(Some(color))) => color,
        Some(Some(None)) => return None,
        Some(None) | None => stroke.color,
    };
    Some((color,
          SvgMeshStyle::Stroke { width: stroke.width,
                                 line_cap: stroke.line_cap,
                                 line_join: stroke.line_join,
                                 miter_limit: stroke.miter_limit }))
}

fn transform_scale(transform: Mat3) -> f32 {
    let x = transform.cols[0][0].hypot(transform.cols[0][1]);
    let y = transform.cols[1][0].hypot(transform.cols[1][1]);
    x.max(y)
     .max(f32::EPSILON)
}

fn outside_viewport(transform: Mat3,
                    geometry: &crate::svg::SvgGeometry,
                    width: u32,
                    height: u32)
                    -> bool {
    let mut min_x = f32::INFINITY;
    let mut min_y = f32::INFINITY;
    let mut max_x = f32::NEG_INFINITY;
    let mut max_y = f32::NEG_INFINITY;
    for command in geometry.commands
                           .iter()
    {
        let point = match *command {
            crate::svg::SvgPathCommand::MoveTo { x, y }
            | crate::svg::SvgPathCommand::LineTo { x, y } => Some((x, y)),
            crate::svg::SvgPathCommand::QuadraticTo { x, y, .. }
            | crate::svg::SvgPathCommand::CubicTo { x, y, .. } => Some((x, y)),
            crate::svg::SvgPathCommand::Close => None,
        };
        if let Some((x, y)) = point {
            let (x, y) = transform.transform_point(x, y);
            min_x = min_x.min(x);
            min_y = min_y.min(y);
            max_x = max_x.max(x);
            max_y = max_y.max(y);
        }
    }
    max_x < 0.0 || max_y < 0.0 || min_x > width as f32 || min_y > height as f32
}

#[cfg(target_os = "android")]
fn surface_srgb_value(_: bool) -> f32 {
    2.0
}

#[cfg(not(target_os = "android"))]
fn surface_srgb_value(is_srgb: bool) -> f32 {
    if is_srgb { 1.0 } else { 0.0 }
}

#[cfg(test)]
mod tests {
    use naga::valid::{Capabilities, ValidationFlags, Validator};

    use super::SvgPipeline;

    #[test]
    fn svg_shader_parses_and_validates() {
        let module = naga::front::wgsl::parse_str(SvgPipeline::shader_source())
            .expect("SVG WGSL should parse");
        Validator::new(ValidationFlags::all(), Capabilities::all())
            .validate(&module)
            .expect("SVG WGSL should validate");
    }

    #[test]
    fn svg_pipeline_uses_four_sample_antialiasing() {
        assert_eq!(SvgPipeline::multisample_state().count, 4);
    }
}

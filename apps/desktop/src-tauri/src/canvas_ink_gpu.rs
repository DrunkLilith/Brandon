//! One reusable bounded coverage mask: a stroke blends once even at self-crossings.
#[cfg(test)]
use super::SceneDelta;
use super::{DrawBatch, Node, bytes, color, depth_state, layer_depth, rgb};
use std::{collections::HashMap, ops::Range};

pub(super) const MASK_BUDGET: u64 = 64 * 1024 * 1024;
fn valid_mask_size(size: [u32; 2]) -> bool {
    !size.contains(&0) && u64::from(size[0]) * u64::from(size[1]) <= MASK_BUDGET
}
pub(super) use crate::canvas_render_scene::validate_scene;
pub(super) struct InkFrame<'a> {
    pub batches: &'a [DrawBatch],
    pub ids: &'a [&'a String],
    pub nodes: &'a HashMap<String, Node>,
    pub layers: &'a HashMap<String, usize>,
    pub scale: f32,
    pub screen: &'a dyn Fn(f32, f32) -> [f32; 2],
    pub size: [u32; 2],
    pub enabled: bool,
}
pub(super) struct CanvasTarget<'a> {
    pub color: &'a wgpu::TextureView,
    pub depth: &'a wgpu::TextureView,
    pub uniform: &'a wgpu::BindGroup,
    pub scissor: [u32; 4],
}
pub(super) fn color_pass<'a>(
    encoder: &'a mut wgpu::CommandEncoder,
    target: &CanvasTarget<'a>,
    clear: bool,
) -> wgpu::RenderPass<'a> {
    encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some("Canvas composed layer"),
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view: target.color,
            depth_slice: None,
            resolve_target: None,
            ops: wgpu::Operations {
                load: if clear {
                    wgpu::LoadOp::Clear(wgpu::Color {
                        r: 0.118,
                        g: 0.118,
                        b: 0.118,
                        a: 1.,
                    })
                } else {
                    wgpu::LoadOp::Load
                },
                store: wgpu::StoreOp::Store,
            },
        })],
        depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
            view: target.depth,
            depth_ops: Some(wgpu::Operations {
                load: if clear {
                    wgpu::LoadOp::Clear(1.)
                } else {
                    wgpu::LoadOp::Load
                },
                store: wgpu::StoreOp::Store,
            }),
            stencil_ops: None,
        }),
        timestamp_writes: None,
        occlusion_query_set: None,
        multiview_mask: None,
    })
}
struct Mask {
    _texture: wgpu::Texture,
    view: wgpu::TextureView,
    bind: wgpu::BindGroup,
    size: [u32; 2],
}
struct InkDraw {
    segments: Range<u32>,
    composite: Range<u32>,
    bounds: [f32; 4],
}
pub(super) struct InkGpu {
    coverage: wgpu::RenderPipeline,
    clear: wgpu::RenderPipeline,
    composite: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    mask: Option<Mask>,
    segments: wgpu::Buffer,
    composites: wgpu::Buffer,
    segment_capacity: u64,
    composite_capacity: u64,
    draws: Vec<Vec<InkDraw>>,
    pub count: usize,
    pub segment_count: usize,
}
impl InkGpu {
    pub(super) fn new(
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        uniform: &wgpu::BindGroupLayout,
    ) -> Self {
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Ink coverage mask"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: false },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            }],
        });
        let mask_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[Some(uniform)],
            immediate_size: 0,
        });
        let composite_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[Some(uniform), Some(&layout)],
            immediate_size: 0,
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Canvas ink union"),
            source: wgpu::ShaderSource::Wgsl(include_str!("canvas_ink.wgsl").into()),
        });
        let max = wgpu::BlendComponent {
            src_factor: wgpu::BlendFactor::One,
            dst_factor: wgpu::BlendFactor::One,
            operation: wgpu::BlendOperation::Max,
        };
        let coverage = pipeline(
            device,
            &shader,
            (&mask_layout, "mask_vertex", "mask_fragment"),
            wgpu::TextureFormat::R8Unorm,
            Some(wgpu::BlendState {
                color: max,
                alpha: max,
            }),
            false,
            false,
        );
        let clear = pipeline(
            device,
            &shader,
            (&mask_layout, "composite_vertex", "clear_fragment"),
            wgpu::TextureFormat::R8Unorm,
            None,
            false,
            true,
        );
        let composite = pipeline(
            device,
            &shader,
            (&composite_layout, "composite_vertex", "composite_fragment"),
            format,
            Some(wgpu::BlendState::ALPHA_BLENDING),
            true,
            true,
        );
        Self {
            coverage,
            clear,
            composite,
            layout,
            mask: None,
            segments: buffer(device, "Ink segments", 4096),
            composites: buffer(device, "Ink layers", 4096),
            segment_capacity: 4096,
            composite_capacity: 4096,
            draws: Vec::new(),
            count: 0,
            segment_count: 0,
        }
    }
    pub(super) fn mask_bytes(&self) -> u64 {
        self.mask
            .as_ref()
            .map_or(0, |m| u64::from(m.size[0]) * u64::from(m.size[1]))
    }
    pub(super) fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        frame: InkFrame<'_>,
    ) -> Result<(), String> {
        self.draws.clear();
        self.count = 0;
        self.segment_count = 0;
        let mut segments = Vec::new();
        let mut composites = Vec::new();
        for batch in frame.batches {
            let mut draws = Vec::new();
            if frame.enabled {
                for id in &frame.ids[batch.nodes.clone()] {
                    let Some(node) = frame.nodes.get(*id) else {
                        continue;
                    };
                    let Some(ink) = &node.ink else { continue };
                    let start = (segments.len() / 8) as u32;
                    for s in &ink.segments {
                        let a = (frame.screen)(node.x + s[0], node.y + s[1]);
                        let b = (frame.screen)(node.x + s[2], node.y + s[3]);
                        segments.extend([
                            a[0],
                            a[1],
                            b[0],
                            b[1],
                            s[4] * frame.scale,
                            s[5] * frame.scale,
                            0.,
                            0.,
                        ]);
                    }
                    let p = (frame.screen)(node.x, node.y);
                    let bounds = [
                        p[0],
                        p[1],
                        node.width * frame.scale,
                        node.height * frame.scale,
                    ];
                    let at = (composites.len() / 16) as u32;
                    let mut fill = color(rgb(Some(&ink.color), 0xdad8e0));
                    fill[3] = ink.opacity;
                    composites.extend(bounds);
                    composites.extend(fill);
                    composites.extend([0.; 4]);
                    composites.extend([
                        0.,
                        0.,
                        0.,
                        layer_depth(*frame.layers.get(*id).unwrap_or(&0), frame.nodes.len()),
                    ]);
                    draws.push(InkDraw {
                        segments: start..(segments.len() / 8) as u32,
                        composite: at..at + 1,
                        bounds,
                    });
                    self.count += 1;
                    self.segment_count += ink.segments.len();
                }
            }
            self.draws.push(draws);
        }
        if self.count == 0 {
            self.mask = None;
            return Ok(());
        }
        self.resize(device, frame.size)?;
        upload(
            device,
            queue,
            &mut self.segments,
            &mut self.segment_capacity,
            &segments,
            "Ink segments",
        );
        upload(
            device,
            queue,
            &mut self.composites,
            &mut self.composite_capacity,
            &composites,
            "Ink layers",
        );
        Ok(())
    }
    pub(super) fn encode_batch(
        &self,
        batch: usize,
        encoder: &mut wgpu::CommandEncoder,
        target: &CanvasTarget<'_>,
    ) {
        let Some(mask) = &self.mask else { return };
        let Some(draws) = self.draws.get(batch) else {
            return;
        };
        for draw in draws {
            let [vx, vy, vw, vh] = target.scissor;
            let [x, y, w, h] = draw.bounds;
            let left = (x.floor().max(vx as f32)) as u32;
            let top = (y.floor().max(vy as f32)) as u32;
            let right = ((x + w).ceil().min((vx + vw) as f32).max(left as f32)) as u32;
            let bottom = ((y + h).ceil().min((vy + vh) as f32).max(top as f32)) as u32;
            if right <= left || bottom <= top {
                continue;
            }
            {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("Ink union coverage"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &mask.view,
                        depth_slice: None,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Load,
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                    multiview_mask: None,
                });
                pass.set_scissor_rect(left, top, right - left, bottom - top);
                pass.set_bind_group(0, target.uniform, &[]);
                pass.set_pipeline(&self.clear);
                pass.set_vertex_buffer(0, self.composites.slice(..));
                pass.draw(0..6, draw.composite.clone());
                pass.set_pipeline(&self.coverage);
                pass.set_vertex_buffer(0, self.segments.slice(..));
                pass.draw(0..6, draw.segments.clone());
            }
            let mut pass = color_pass(encoder, target, false);
            pass.set_scissor_rect(left, top, right - left, bottom - top);
            pass.set_pipeline(&self.composite);
            pass.set_bind_group(0, target.uniform, &[]);
            pass.set_bind_group(1, &mask.bind, &[]);
            pass.set_vertex_buffer(0, self.composites.slice(..));
            pass.draw(0..6, draw.composite.clone());
        }
    }
    fn resize(&mut self, device: &wgpu::Device, size: [u32; 2]) -> Result<(), String> {
        if self.mask.as_ref().is_some_and(|m| m.size == size) {
            return Ok(());
        }
        if !valid_mask_size(size) {
            return Err("Canvas ink mask exceeds 64 MiB budget".into());
        }
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Canvas reusable ink coverage"),
            size: wgpu::Extent3d {
                width: size[0],
                height: size[1],
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let view = texture.create_view(&Default::default());
        let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &self.layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&view),
            }],
        });
        self.mask = Some(Mask {
            _texture: texture,
            view,
            bind,
            size,
        });
        Ok(())
    }
}
fn buffer(device: &wgpu::Device, label: &str, size: u64) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some(label),
        size,
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}
fn upload(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    target: &mut wgpu::Buffer,
    capacity: &mut u64,
    values: &[f32],
    label: &str,
) {
    let data = bytes(values);
    if data.len() as u64 > *capacity {
        *capacity = (data.len() as u64).next_power_of_two();
        *target = buffer(device, label, *capacity)
    }
    queue.write_buffer(target, 0, &data);
}
fn pipeline(
    device: &wgpu::Device,
    shader: &wgpu::ShaderModule,
    entries: (&wgpu::PipelineLayout, &str, &str),
    format: wgpu::TextureFormat,
    blend: Option<wgpu::BlendState>,
    depth: bool,
    wide: bool,
) -> wgpu::RenderPipeline {
    const NARROW: [wgpu::VertexAttribute; 2] = wgpu::vertex_attr_array![0=>Float32x4,1=>Float32x4];
    const WIDE: [wgpu::VertexAttribute; 4] =
        wgpu::vertex_attr_array![0=>Float32x4,1=>Float32x4,2=>Float32x4,3=>Float32x4];
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("Canvas ink"),
        layout: Some(entries.0),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some(entries.1),
            compilation_options: Default::default(),
            buffers: &[Some(wgpu::VertexBufferLayout {
                array_stride: if wide { 64 } else { 32 },
                step_mode: wgpu::VertexStepMode::Instance,
                attributes: if wide { &WIDE } else { &NARROW },
            })],
        },
        fragment: Some(wgpu::FragmentState {
            module: shader,
            entry_point: Some(entries.2),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend,
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        primitive: Default::default(),
        depth_stencil: if depth { Some(depth_state(true)) } else { None },
        multisample: Default::default(),
        multiview_mask: None,
        cache: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn node(id: &str, count: usize) -> Node {
        let mut n:Node=serde_json::from_value(serde_json::json!({"id":id,"x":0,"y":0,"width":100,"height":100,"kind":"text","ink":{"segments":[[20,20,80,80,3,3]],"color":"#123456","opacity":0.24}})).unwrap();
        n.ink
            .as_mut()
            .unwrap()
            .segments
            .resize(count, [20., 20., 80., 80., 3., 3.]);
        n
    }
    fn delta(nodes: Vec<Node>, removed: Vec<String>, reset: bool) -> SceneDelta {
        SceneDelta {
            nodes,
            removed,
            reset,
            order: None,
            edges: None,
            edge_updates: Vec::new(),
        }
    }
    #[test]
    fn prepared_ink_requires_bounded_finite_local_geometry_and_inert_paint() {
        let n = node("ink", 1);
        assert!(n.ink.as_ref().unwrap().valid_for(&n));
        for change in 0..7 {
            let mut bad = n.clone();
            let ink = bad.ink.as_mut().unwrap();
            match change {
                0 => ink.segments.clear(),
                1 => ink.segments[0][0] = f32::NAN,
                2 => ink.segments[0][4] = -1.,
                3 => ink.segments[0][2] = 500.,
                4 => ink.opacity = 0.5,
                5 => ink.color = "url(script)".into(),
                _ => bad.kind = "file".into(),
            };
            assert!(!bad.ink.as_ref().unwrap().valid_for(&bad));
        }
        assert!(
            validate_scene(
                &HashMap::new(),
                &delta(vec![node("valid", 1), node("valid", 1)], vec![], false)
            )
            .is_err()
        );
    }
    #[test]
    fn incremental_ink_budget_counts_replacements_removals_reset_and_stroke_count() {
        let current: HashMap<_, _> = (0..48)
            .map(|i| {
                let n = node(&i.to_string(), 2048);
                (n.id.clone(), n)
            })
            .collect();
        assert!(
            validate_scene(&current, &delta(vec![node("extra", 2048)], vec![], false)).is_err()
        );
        assert!(validate_scene(&current, &delta(vec![node("0", 2048)], vec![], false)).is_ok());
        assert!(
            validate_scene(
                &current,
                &delta(vec![node("extra", 2048)], vec!["0".into()], false)
            )
            .is_ok()
        );
        assert!(validate_scene(&current, &delta(vec![node("extra", 2048)], vec![], true)).is_ok());
        assert_eq!(current.len(), 48);
        assert!(
            validate_scene(
                &HashMap::new(),
                &delta(
                    (0..4097).map(|i| node(&i.to_string(), 1)).collect(),
                    vec![],
                    true
                )
            )
            .is_err()
        );
    }
    #[test]
    fn coverage_mask_budget_refuses_zero_oversize_and_overflow_dimensions() {
        assert!(valid_mask_size([8192, 8192]));
        assert!(!valid_mask_size([8193, 8192]));
        assert!(!valid_mask_size([0, 100]));
        assert!(!valid_mask_size([u32::MAX, u32::MAX]));
    }
}

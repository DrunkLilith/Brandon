//! Native D3D12 canvas. The WebView owns input and focused editing overlays; pixels
//! are presented to the host HWND, without a CPU texture readback or image IPC.
use glyphon::{
    Attrs, Buffer, Cache, Color, ColorMode, Family, FontSystem, Metrics, Resolution, Shaping,
    SwashCache, TextArea, TextAtlas, TextBounds, TextRenderer, Viewport,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, Mutex},
    time::Instant,
};
use tauri::{State, WebviewWindow};
use wgpu::util::DeviceExt;

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Node {
    pub id: String,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub fill: Option<String>,
    #[serde(default)]
    pub border: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Edge {
    pub from_node: String,
    pub to_node: String,
}

#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SceneDelta {
    #[serde(default)]
    pub reset: bool,
    #[serde(default)]
    pub nodes: Vec<Node>,
    #[serde(default)]
    pub removed: Vec<String>,
    pub order: Option<Vec<String>>,
    pub edges: Option<Vec<Edge>>,
}

#[derive(Clone, Copy, Debug, Deserialize)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}
impl Rect {
    fn intersects(self, other: Self) -> bool {
        self.x <= other.x + other.width
            && self.x + self.width >= other.x
            && self.y <= other.y + other.height
            && self.y + self.height >= other.y
    }
}
#[derive(Clone, Copy, Deserialize)]
pub struct Camera {
    pub x: f32,
    pub y: f32,
    pub zoom: f32,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Frame {
    pub viewport: Rect,
    pub camera: Camera,
    pub pixel_ratio: f32,
    #[serde(default)]
    pub selected: Vec<String>,
    pub editing: Option<String>,
}

#[derive(Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Stats {
    pub adapter: String,
    pub backend: String,
    pub total_nodes: usize,
    pub visible_nodes: usize,
    pub visible_edges: usize,
    pub frame_cpu_ms: f64,
    pub scene_update_ms: f64,
    pub frames: u64,
    pub text_buffers: usize,
    pub surface_recoveries: u64,
}

#[derive(Default)]
struct SpatialIndex {
    buckets: HashMap<(i32, i32), Vec<String>>,
    oversized: Vec<String>,
}
impl SpatialIndex {
    fn cells(rect: Rect) -> (i32, i32, i32, i32) {
        (
            (rect.x / 512.).floor() as i32,
            (rect.y / 512.).floor() as i32,
            ((rect.x + rect.width) / 512.).floor() as i32,
            ((rect.y + rect.height) / 512.).floor() as i32,
        )
    }
    fn rebuild(&mut self, nodes: &HashMap<String, Node>) {
        self.buckets.clear();
        self.oversized.clear();
        for node in nodes.values() {
            let (left, top, right, bottom) = Self::cells(node.rect());
            if (i64::from(right) - i64::from(left) + 1) * (i64::from(bottom) - i64::from(top) + 1)
                > 64
            {
                self.oversized.push(node.id.clone());
                continue;
            }
            for x in left..=right {
                for y in top..=bottom {
                    self.buckets
                        .entry((x, y))
                        .or_default()
                        .push(node.id.clone());
                }
            }
        }
    }
    fn query(&self, rect: Rect, nodes: &HashMap<String, Node>) -> HashSet<String> {
        let (left, top, right, bottom) = Self::cells(rect);
        if (i64::from(right) - i64::from(left) + 1) * (i64::from(bottom) - i64::from(top) + 1)
            > 100_000
        {
            return nodes
                .values()
                .filter(|node| rect.intersects(node.rect()))
                .map(|node| node.id.clone())
                .collect();
        }
        let mut candidates: HashSet<String> = self.oversized.iter().cloned().collect();
        for x in left..=right {
            for y in top..=bottom {
                if let Some(ids) = self.buckets.get(&(x, y)) {
                    candidates.extend(ids.iter().cloned());
                }
            }
        }
        candidates.retain(|id| {
            nodes
                .get(id)
                .is_some_and(|node| rect.intersects(node.rect()))
        });
        candidates
    }
}
impl Node {
    fn rect(&self) -> Rect {
        Rect {
            x: self.x,
            y: self.y,
            width: self.width,
            height: self.height,
        }
    }
}

#[derive(Clone, Default)]
pub struct GpuCanvasState {
    inner: Arc<Mutex<Option<Renderer>>>,
}

fn bytes(values: &[f32]) -> Vec<u8> {
    values
        .iter()
        .flat_map(|value| value.to_ne_bytes())
        .collect()
}
fn color(hex: u32) -> [f32; 4] {
    [
        ((hex >> 16) & 255) as f32 / 255.,
        ((hex >> 8) & 255) as f32 / 255.,
        (hex & 255) as f32 / 255.,
        1.,
    ]
}
fn rgb(value: Option<&str>, fallback: u32) -> u32 {
    value
        .and_then(|value| value.strip_prefix('#'))
        .filter(|value| value.len() == 6)
        .and_then(|value| u32::from_str_radix(value, 16).ok())
        .unwrap_or(fallback)
}
fn layer_depth(index: usize, total: usize) -> f32 {
    0.9 - index as f32 / total.max(1) as f32 * 0.8
}
fn depth_state(write: bool) -> wgpu::DepthStencilState {
    wgpu::DepthStencilState {
        format: wgpu::TextureFormat::Depth32Float,
        depth_write_enabled: Some(write),
        depth_compare: Some(wgpu::CompareFunction::LessEqual),
        stencil: Default::default(),
        bias: Default::default(),
    }
}
fn depth_view(device: &wgpu::Device, config: &wgpu::SurfaceConfiguration) -> wgpu::TextureView {
    device
        .create_texture(&wgpu::TextureDescriptor {
            label: Some("Canvas occlusion"),
            size: wgpu::Extent3d {
                width: config.width,
                height: config.height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth32Float,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        })
        .create_view(&Default::default())
}
fn instance(output: &mut Vec<f32>, rect: [f32; 4], fill: u32, border: u32, style: [f32; 4]) {
    output.extend(rect);
    output.extend(color(fill));
    output.extend(color(border));
    output.extend(style);
}

struct Renderer {
    surface: wgpu::Surface<'static>,
    instance: wgpu::Instance,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    depth: wgpu::TextureView,
    pipeline: wgpu::RenderPipeline,
    uniform: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    instances: wgpu::Buffer,
    capacity: u64,
    fonts: FontSystem,
    swash: SwashCache,
    atlas: TextAtlas,
    text_renderer: TextRenderer,
    text_viewport: Viewport,
    nodes: HashMap<String, Node>,
    order: Vec<String>,
    layers: HashMap<String, usize>,
    edges: Vec<Edge>,
    spatial: SpatialIndex,
    buffers: HashMap<String, Buffer>,
    stats: Stats,
    window: Arc<WebviewWindow>,
}

impl Renderer {
    async fn new(window: WebviewWindow) -> Result<Self, String> {
        let window = Arc::new(window);
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let surface = instance
            .create_surface(Arc::clone(&window))
            .map_err(|error| error.to_string())?;
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                compatible_surface: Some(&surface),
                power_preference: wgpu::PowerPreference::HighPerformance,
                force_fallback_adapter: false,
                ..Default::default()
            })
            .await
            .map_err(|error| error.to_string())?;
        let info = adapter.get_info();
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("Brandon native canvas"),
                ..Default::default()
            })
            .await
            .map_err(|error| error.to_string())?;
        let size = window.inner_size().map_err(|error| error.to_string())?;
        let capabilities = surface.get_capabilities(&adapter);
        let format = capabilities
            .formats
            .iter()
            .copied()
            .find(|format| !format.is_srgb())
            .unwrap_or(capabilities.formats[0]);
        let mut config = surface
            .get_default_config(&adapter, size.width.max(1), size.height.max(1))
            .ok_or("GPU surface is unsupported")?;
        config.format = format;
        config.present_mode = wgpu::PresentMode::Fifo;
        config.desired_maximum_frame_latency = 2;
        surface.configure(&device, &config);
        let uniform = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Canvas dimensions"),
            contents: &bytes(&[size.width as f32, size.height as f32, 0., 0.]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let bind_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &bind_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform.as_entire_binding(),
            }],
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Canvas cards and grid"),
            source: wgpu::ShaderSource::Wgsl(include_str!("canvas.wgsl").into()),
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[Some(&bind_layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Canvas instanced rectangles"), layout: Some(&layout),
            vertex: wgpu::VertexState { module: &shader, entry_point: Some("vertex"), compilation_options: Default::default(), buffers: &[Some(wgpu::VertexBufferLayout { array_stride: 64, step_mode: wgpu::VertexStepMode::Instance, attributes: &wgpu::vertex_attr_array![0 => Float32x4, 1 => Float32x4, 2 => Float32x4, 3 => Float32x4] })] },
            fragment: Some(wgpu::FragmentState { module: &shader, entry_point: Some("fragment"), compilation_options: Default::default(), targets: &[Some(wgpu::ColorTargetState { format, blend: Some(wgpu::BlendState::ALPHA_BLENDING), write_mask: wgpu::ColorWrites::ALL })] }),
            primitive: Default::default(), depth_stencil: Some(depth_state(true)), multisample: Default::default(), multiview_mask: None, cache: None,
        });
        let instances = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Canvas instances"),
            size: 64 * 1024,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let cache = Cache::new(&device);
        let text_viewport = Viewport::new(&device, &cache);
        let mut atlas = TextAtlas::with_color_mode(&device, &queue, &cache, format, ColorMode::Web);
        let text_renderer = TextRenderer::new(
            &mut atlas,
            &device,
            Default::default(),
            Some(depth_state(false)),
        );
        let depth = depth_view(&device, &config);
        Ok(Self {
            surface,
            instance,
            device,
            queue,
            config,
            depth,
            pipeline,
            uniform,
            bind_group,
            instances,
            capacity: 64 * 1024,
            fonts: FontSystem::new(),
            swash: SwashCache::new(),
            atlas,
            text_renderer,
            text_viewport,
            nodes: HashMap::new(),
            order: Vec::new(),
            layers: HashMap::new(),
            edges: Vec::new(),
            spatial: SpatialIndex::default(),
            buffers: HashMap::new(),
            stats: Stats {
                adapter: info.name,
                backend: format!("{:?}", info.backend),
                ..Default::default()
            },
            window,
        })
    }

    fn update(&mut self, delta: SceneDelta) -> Result<(), String> {
        let start = Instant::now();
        if delta.nodes.len() > 100_000 {
            return Err("Canvas prototype limit: 100,000 objects".into());
        }
        if delta.reset {
            self.nodes.clear();
            self.order.clear();
            self.buffers.clear();
        }
        for id in delta.removed {
            self.nodes.remove(&id);
            self.buffers.remove(&id);
            self.order.retain(|item| item != &id);
        }
        for node in delta.nodes {
            if ![node.x, node.y, node.width, node.height]
                .iter()
                .all(|value| value.is_finite() && value.abs() <= 10_000_000.)
                || node.width <= 0.
                || node.height <= 0.
            {
                return Err("Invalid canvas object geometry".into());
            }
            if !self.nodes.contains_key(&node.id) {
                self.order.push(node.id.clone());
            }
            if self.nodes.get(&node.id).is_none_or(|old| {
                old.text != node.text || old.width != node.width || old.height != node.height
            }) {
                self.buffers.remove(&node.id);
            }
            self.nodes.insert(node.id.clone(), node);
        }
        if let Some(order) = delta.order {
            if self.order != order {
                self.buffers.clear();
            }
            self.order = order;
        }
        self.layers = self
            .order
            .iter()
            .enumerate()
            .map(|(i, id)| (id.clone(), i))
            .collect();
        if let Some(edges) = delta.edges {
            self.edges = edges;
        }
        self.spatial.rebuild(&self.nodes);
        self.stats.total_nodes = self.nodes.len();
        self.stats.scene_update_ms = start.elapsed().as_secs_f64() * 1000.;
        Ok(())
    }

    fn render(&mut self, frame: Frame) -> Result<Stats, String> {
        let start = Instant::now();
        let size = self
            .window
            .inner_size()
            .map_err(|error| error.to_string())?;
        if size.width == 0 || size.height == 0 {
            return Ok(self.stats.clone());
        }
        if size.width != self.config.width || size.height != self.config.height {
            self.config.width = size.width;
            self.config.height = size.height;
            self.surface.configure(&self.device, &self.config);
            self.depth = depth_view(&self.device, &self.config);
        }
        let ratio = frame.pixel_ratio;
        let zoom = frame.camera.zoom;
        if !ratio.is_finite()
            || !(0.25..=8.).contains(&ratio)
            || !zoom.is_finite()
            || !(0.05..=10.).contains(&zoom)
        {
            return Err("Invalid camera scale".into());
        }
        let vp = frame.viewport;
        if ![
            vp.x,
            vp.y,
            vp.width,
            vp.height,
            frame.camera.x,
            frame.camera.y,
        ]
        .iter()
        .all(|value| value.is_finite() && value.abs() <= 10_000_000.)
            || vp.width <= 0.
            || vp.height <= 0.
        {
            return Err("Invalid canvas viewport".into());
        }
        let world = Rect {
            x: (-vp.width / 2. - frame.camera.x) / zoom,
            y: (-vp.height / 2. - frame.camera.y) / zoom,
            width: vp.width / zoom,
            height: vp.height / zoom,
        };
        let visible = self.spatial.query(world, &self.nodes);
        let selected: HashSet<&str> = frame.selected.iter().map(String::as_str).collect();
        let offset = (
            (vp.x + vp.width / 2. + frame.camera.x) * ratio,
            (vp.y + vp.height / 2. + frame.camera.y) * ratio,
        );
        let scale = zoom * ratio;
        let screen = |x: f32, y: f32| [offset.0 + x * scale, offset.1 + y * scale];
        let mut shapes = Vec::<f32>::with_capacity((visible.len() + 1) * 16);
        instance(
            &mut shapes,
            [
                vp.x * ratio,
                vp.y * ratio,
                vp.width * ratio,
                vp.height * ratio,
            ],
            0x1a1a1a,
            0x464646,
            [
                -(vp.width / 2. + frame.camera.x) * ratio,
                2.,
                24. * scale,
                -(vp.height / 2. + frame.camera.y) * ratio,
            ],
        );
        let mut edge_count = 0;
        for edge in &self.edges {
            let (Some(a), Some(b)) = (
                self.nodes.get(&edge.from_node),
                self.nodes.get(&edge.to_node),
            ) else {
                continue;
            };
            let (x1, y1, x2, y2) = (
                a.x + a.width / 2.,
                a.y + a.height / 2.,
                b.x + b.width / 2.,
                b.y + b.height / 2.,
            );
            if !world.intersects(Rect {
                x: x1.min(x2),
                y: y1.min(y2),
                width: (x2 - x1).abs().max(1.),
                height: (y2 - y1).abs().max(1.),
            }) {
                continue;
            }
            let a = screen(x1, y1);
            let b = screen(x2, y2);
            instance(
                &mut shapes,
                [a[0], a[1], b[0], b[1]],
                0x817aab,
                0x817aab,
                [1.5 * ratio, 1., 0., 0.],
            );
            edge_count += 1;
        }
        let ids: Vec<&String> = self
            .order
            .iter()
            .filter(|id| visible.contains(*id))
            .collect();
        for id in &ids {
            let Some(node) = self.nodes.get(*id) else {
                continue;
            };
            let position = screen(node.x, node.y);
            instance(
                &mut shapes,
                [
                    position[0],
                    position[1],
                    node.width * scale,
                    node.height * scale,
                ],
                rgb(
                    node.fill.as_deref(),
                    if node.kind == "group" {
                        0x222128
                    } else {
                        0x28282b
                    },
                ),
                if selected.contains(id.as_str()) {
                    0xaca1ee
                } else {
                    rgb(node.border.as_deref(), 0x4a484f)
                },
                [
                    8. * scale.min(2.),
                    0.,
                    if selected.contains(id.as_str()) {
                        2. * ratio
                    } else {
                        ratio
                    },
                    layer_depth(*self.layers.get(*id).unwrap_or(&0), self.order.len()),
                ],
            );
            if selected.contains(id.as_str()) {
                instance(
                    &mut shapes,
                    [
                        position[0] + node.width * scale - 5. * ratio,
                        position[1] + node.height * scale - 5. * ratio,
                        10. * ratio,
                        10. * ratio,
                    ],
                    0xb2a6f2,
                    0xe7e0ff,
                    [
                        5. * ratio,
                        0.,
                        ratio,
                        layer_depth(*self.layers.get(*id).unwrap_or(&0), self.order.len())
                            - 0.000001,
                    ],
                );
            }
            if zoom < 0.5 || node.width < 30. || node.height < 30. {
                continue;
            }
            if !self.buffers.contains_key(*id) {
                let mut buffer = Buffer::new(&mut self.fonts, Metrics::new(14., 21.));
                buffer.set_size(
                    Some((node.width - 28.).max(1.)),
                    Some((node.height - 28.).max(1.)),
                );
                buffer.set_text(
                    &node.text,
                    &Attrs::new()
                        .family(Family::Name("Segoe UI"))
                        .metadata(*self.layers.get(*id).unwrap_or(&0)),
                    Shaping::Advanced,
                    None,
                );
                buffer.shape_until_scroll(&mut self.fonts, false);
                self.buffers.insert((*id).clone(), buffer);
            }
        }
        self.text_viewport.update(
            &self.queue,
            Resolution {
                width: size.width,
                height: size.height,
            },
        );
        let text_areas: Vec<TextArea<'_>> = ids
            .iter()
            .filter_map(|id| {
                if zoom < 0.5 || frame.editing.as_ref() == Some(*id) {
                    return None;
                }
                let node = self.nodes.get(*id)?;
                if node.width < 30. || node.height < 30. {
                    return None;
                }
                let buffer = self.buffers.get(*id)?;
                let position = screen(node.x + 14., node.y + 14.);
                Some(TextArea {
                    buffer,
                    left: position[0],
                    top: position[1],
                    scale,
                    bounds: TextBounds {
                        left: (vp.x * ratio).max(position[0]) as i32,
                        top: (vp.y * ratio).max(position[1]) as i32,
                        right: ((vp.x + vp.width) * ratio)
                            .min(position[0] + (node.width - 28.) * scale)
                            as i32,
                        bottom: ((vp.y + vp.height) * ratio)
                            .min(position[1] + (node.height - 28.) * scale)
                            as i32,
                    },
                    default_color: Color::rgb(218, 216, 224),
                    custom_glyphs: &[],
                })
            })
            .collect();
        self.text_renderer
            .prepare_with_depth(
                &self.device,
                &self.queue,
                &mut self.fonts,
                &mut self.atlas,
                &self.text_viewport,
                text_areas,
                &mut self.swash,
                |metadata| layer_depth(metadata, self.order.len()) - 0.000001,
            )
            .map_err(|error| error.to_string())?;
        let texture = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(texture)
            | wgpu::CurrentSurfaceTexture::Suboptimal(texture) => texture,
            wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded => {
                return Ok(self.stats.clone());
            }
            wgpu::CurrentSurfaceTexture::Outdated => {
                self.surface.configure(&self.device, &self.config);
                self.stats.surface_recoveries += 1;
                return Ok(self.stats.clone());
            }
            wgpu::CurrentSurfaceTexture::Lost => {
                self.surface = self
                    .instance
                    .create_surface(Arc::clone(&self.window))
                    .map_err(|error| error.to_string())?;
                self.surface.configure(&self.device, &self.config);
                self.stats.surface_recoveries += 1;
                return Ok(self.stats.clone());
            }
            wgpu::CurrentSurfaceTexture::Validation => {
                return Err("Native surface validation failed".into());
            }
        };
        let data = bytes(&shapes);
        if data.len() as u64 > self.capacity {
            self.capacity = (data.len() as u64).next_power_of_two();
            self.instances = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("Canvas instances"),
                size: self.capacity,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
        }
        self.queue.write_buffer(&self.instances, 0, &data);
        self.queue.write_buffer(
            &self.uniform,
            0,
            &bytes(&[size.width as f32, size.height as f32, 0., 0.]),
        );
        let view = texture.texture.create_view(&Default::default());
        let mut encoder = self.device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Canvas frame"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.118,
                            g: 0.118,
                            b: 0.118,
                            a: 1.,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.depth,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            let left = (vp.x * ratio).clamp(0., size.width as f32) as u32;
            let top = (vp.y * ratio).clamp(0., size.height as f32) as u32;
            let right = ((vp.x + vp.width) * ratio).clamp(left as f32, size.width as f32) as u32;
            let bottom = ((vp.y + vp.height) * ratio).clamp(top as f32, size.height as f32) as u32;
            if right > left && bottom > top {
                pass.set_scissor_rect(left, top, right - left, bottom - top);
                pass.set_pipeline(&self.pipeline);
                pass.set_bind_group(0, &self.bind_group, &[]);
                pass.set_vertex_buffer(0, self.instances.slice(..));
                pass.draw(0..6, 0..(shapes.len() / 16) as u32);
                self.text_renderer
                    .render(&self.atlas, &self.text_viewport, &mut pass)
                    .map_err(|error| error.to_string())?;
            }
        }
        self.queue.submit(Some(encoder.finish()));
        self.queue.present(texture);
        self.atlas.trim();
        if self.buffers.len() > 6000 {
            self.buffers.retain(|id, _| visible.contains(id));
        }
        self.stats.visible_nodes = ids.len();
        self.stats.visible_edges = edge_count;
        self.stats.frames += 1;
        self.stats.text_buffers = self.buffers.len();
        self.stats.frame_cpu_ms = start.elapsed().as_secs_f64() * 1000.;
        Ok(self.stats.clone())
    }
}

#[tauri::command]
pub async fn canvas_gpu_start(
    window: WebviewWindow,
    state: State<'_, GpuCanvasState>,
) -> Result<Stats, String> {
    let inner = Arc::clone(&state.inner);
    let target = window.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        let mut current = inner.lock().map_err(|error| error.to_string())?;
        if current.is_none() {
            *current = Some(tauri::async_runtime::block_on(Renderer::new(target))?);
        }
        Ok::<_, String>(
            current
                .as_ref()
                .ok_or("Renderer unavailable")?
                .stats
                .clone(),
        )
    })
    .await
    .map_err(|error| error.to_string())??;
    window
        .set_background_color(Some(tauri::window::Color(0, 0, 0, 0)))
        .map_err(|error| error.to_string())?;
    Ok(result)
}

#[tauri::command]
pub async fn canvas_gpu_scene(
    delta: SceneDelta,
    state: State<'_, GpuCanvasState>,
) -> Result<(), String> {
    let inner = Arc::clone(&state.inner);
    tauri::async_runtime::spawn_blocking(move || {
        inner
            .lock()
            .map_err(|error| error.to_string())?
            .as_mut()
            .ok_or("Renderer is not running")?
            .update(delta)
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
pub async fn canvas_gpu_frame(
    frame: Frame,
    state: State<'_, GpuCanvasState>,
) -> Result<Stats, String> {
    let inner = Arc::clone(&state.inner);
    tauri::async_runtime::spawn_blocking(move || {
        inner
            .lock()
            .map_err(|error| error.to_string())?
            .as_mut()
            .ok_or("Renderer is not running")?
            .render(frame)
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
pub async fn canvas_gpu_stop(
    window: WebviewWindow,
    state: State<'_, GpuCanvasState>,
) -> Result<(), String> {
    window
        .set_background_color(Some(tauri::window::Color(30, 30, 30, 255)))
        .map_err(|error| error.to_string())?;
    let inner = Arc::clone(&state.inner);
    tauri::async_runtime::spawn_blocking(move || {
        inner.lock().map_err(|error| error.to_string())?.take();
        Ok::<_, String>(())
    })
    .await
    .map_err(|error| error.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn spatial_query_handles_negative_coordinates_large_groups_and_boundaries() {
        let nodes: HashMap<_, _> = [
            Node {
                id: "negative".into(),
                x: -600.,
                y: -600.,
                width: 200.,
                height: 200.,
                text: String::new(),
                kind: String::new(),
                fill: None,
                border: None,
            },
            Node {
                id: "group".into(),
                x: -100_000.,
                y: -100_000.,
                width: 200_000.,
                height: 200_000.,
                text: String::new(),
                kind: "group".into(),
                fill: None,
                border: None,
            },
        ]
        .into_iter()
        .map(|node| (node.id.clone(), node))
        .collect();
        let mut spatial = SpatialIndex::default();
        spatial.rebuild(&nodes);
        let hits = spatial.query(
            Rect {
                x: -400.,
                y: -400.,
                width: 10.,
                height: 10.,
            },
            &nodes,
        );
        assert!(hits.contains("negative"));
        assert!(hits.contains("group"));
        assert_eq!(
            spatial
                .query(
                    Rect {
                        x: 10.,
                        y: 10.,
                        width: 10.,
                        height: 10.
                    },
                    &nodes
                )
                .len(),
            1
        );
    }
}

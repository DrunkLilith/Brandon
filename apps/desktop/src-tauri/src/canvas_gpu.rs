//! Native D3D12 canvas. The WebView owns input and focused editing overlays; pixels
//! are presented to the host HWND, without a CPU texture readback or image IPC.
use glyphon::cosmic_text::{Align, Wrap};
use glyphon::{
    Attrs, Buffer, Cache, Color, ColorMode, Family, FontSystem, Metrics, Resolution, Shaping,
    SwashCache, TextArea, TextAtlas, TextBounds, TextRenderer, Viewport,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    ops::Range,
    sync::{Arc, Mutex},
    time::Instant,
};
use tauri::{State, WebviewWindow};
use wgpu::util::DeviceExt;
#[path = "canvas_raster_gpu.rs"]
mod raster;
use super::{VaultState, attachment_commands::AttachmentState, current_vault};

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
    #[serde(default)]
    pub style: Option<NodeStyle>,
    #[serde(default)]
    image: Option<RasterRef>,
}

#[derive(Clone, Debug, Deserialize)]
struct RasterRef {
    path: String,
    bounds: Rect,
}
impl RasterRef {
    fn valid_for(&self, node: &Node) -> bool {
        let b = self.bounds;
        node.kind == "file"
            && !self.path.is_empty()
            && self.path.len() <= 4096
            && [b.x, b.y, b.width, b.height].iter().all(|v| v.is_finite())
            && b.x >= 0.
            && b.y >= 0.
            && b.width > 0.
            && b.height > 0.
            && b.x + b.width <= node.width + 0.01
            && b.y + b.height <= node.height + 0.01
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct NodeStyle {
    shape: u32,
    radius: f32,
    font_size: f32,
    align: String,
    text_color: String,
    stroke_width: f32,
    bounds: Rect,
    #[serde(default)]
    center_text: bool,
}
impl NodeStyle {
    fn valid_for(&self, node: &Node) -> bool {
        let b = self.bounds;
        matches!(self.shape, 0 | 3 | 4 | 5 | 6 | 7)
            && [12., 16., 24., 32.].contains(&self.font_size)
            && [1., 2., 4.].contains(&self.stroke_width)
            && matches!(self.align.as_str(), "left" | "center" | "right")
            && self.radius.is_finite()
            && (0.0..=20.0).contains(&self.radius)
            && [b.x, b.y, b.width, b.height].iter().all(|v| v.is_finite())
            && b.x >= 0.
            && b.y >= 0.
            && b.width > 0.
            && b.height > 0.
            && b.x + b.width <= node.width + 0.01
            && b.y + b.height <= node.height + 0.01
            && self
                .text_color
                .strip_prefix('#')
                .is_some_and(|s| s.len() == 6 && s.bytes().all(|b| b.is_ascii_hexdigit()))
    }
}
impl Node {
    fn text_rect(&self) -> Rect {
        self.style.as_ref().map_or(
            Rect {
                x: 20.,
                y: 20.,
                width: (self.width - 40.).max(1.),
                height: (self.height - 40.).max(1.),
            },
            |s| s.bounds,
        )
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Edge {
    id: String,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    segments: Vec<[f32; 4]>,
    heads: Vec<[f32; 4]>,
    color: String,
    stroke_width: f32,
    dashed: bool,
    label: String,
    label_rect: Rect,
}
impl Edge {
    fn valid(&self) -> bool {
        !self.id.is_empty()
            && self.id.len() <= 4096
            && !self.segments.is_empty()
            && self.segments.len() <= 4
            && self.heads.len() <= 4
            && [1., 2., 4.].contains(&self.stroke_width)
            && self.label.len() <= 4096
            && self
                .color
                .strip_prefix('#')
                .is_some_and(|s| s.len() == 6 && s.bytes().all(|b| b.is_ascii_hexdigit()))
            && [
                self.x,
                self.y,
                self.width,
                self.height,
                self.label_rect.x,
                self.label_rect.y,
                self.label_rect.width,
                self.label_rect.height,
            ]
            .iter()
            .all(|v| v.is_finite() && v.abs() <= 30_000_000.)
            && self.width > 0.
            && self.height > 0.
            && (1.0..=300.0).contains(&self.label_rect.width)
            && (1.0..=100.0).contains(&self.label_rect.height)
            && self.segments.iter().chain(&self.heads).all(|s| {
                s.iter().all(|v| v.is_finite() && v.abs() <= 30_000_000.)
                    && (s[2] - s[0]).hypot(s[3] - s[1]) > 0.0001
            })
    }
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
    #[serde(default)]
    pub edge_updates: Vec<Edge>,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq)]
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
    #[serde(default)]
    pub selected_edge: Option<String>,
    #[serde(default)]
    pub locked: Vec<String>,
    #[serde(default)]
    pub overlay_text: bool,
    #[serde(default)]
    pub overlay_nodes: bool,
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
    pub edge_labels: usize,
    pub draw_batches: usize,
    pub translucent_batches: usize,
    pub raster_images: usize,
    pub raster_bytes: usize,
    pub raster_limited: usize,
    pub missing_images: Vec<String>,
    pub raster_paths: Vec<String>,
    pub frame_cpu_ms: f64,
    pub scene_update_ms: f64,
    pub frames: u64,
    pub text_buffers: usize,
    pub surface_recoveries: u64,
}

#[derive(Debug, PartialEq)]
struct DrawBatch {
    shapes: Range<u32>,
    nodes: Range<usize>,
    edges: bool,
    translucent: bool,
}
/// Interleave glyphs only at translucent cuts. Opaque scenes keep one batch.
fn composition_batches(prelude: u32, spans: &[(u32, u32, bool)], split: bool) -> Vec<DrawBatch> {
    let mut batches = Vec::new();
    let mut current = DrawBatch {
        shapes: 0..prelude,
        nodes: 0..0,
        edges: true,
        translucent: false,
    };
    for (index, &(start, end, alpha)) in spans.iter().enumerate() {
        if split && alpha {
            if !current.shapes.is_empty() || !current.nodes.is_empty() {
                batches.push(current);
            }
            batches.push(DrawBatch {
                shapes: start..end,
                nodes: index..index + 1,
                edges: false,
                translucent: true,
            });
            current = DrawBatch {
                shapes: end..end,
                nodes: index + 1..index + 1,
                edges: false,
                translucent: false,
            };
        } else {
            current.shapes.end = end;
            current.nodes.end = index + 1;
        }
    }
    if !current.shapes.is_empty() || !current.nodes.is_empty() {
        batches.push(current);
    }
    batches
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
        .filter(|value| value.len() == 6 || value.len() == 8)
        .and_then(|value| value.get(..6))
        .and_then(|value| u32::from_str_radix(value, 16).ok())
        .unwrap_or(fallback)
}
fn opacity(value: Option<&str>) -> f32 {
    value
        .and_then(|value| value.strip_prefix('#'))
        .filter(|value| value.len() == 8)
        .and_then(|value| value.get(6..))
        .and_then(|value| u8::from_str_radix(value, 16).ok())
        .map_or(1., |value| f32::from(value) / 255.)
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
    instance_alpha(output, rect, fill, border, style, 1.);
}
fn instance_alpha(
    output: &mut Vec<f32>,
    rect: [f32; 4],
    fill: u32,
    border: u32,
    style: [f32; 4],
    alpha: f32,
) {
    output.extend(rect);
    let mut fill = color(fill);
    fill[3] = alpha;
    output.extend(fill);
    output.extend(color(border));
    output.extend(style);
}

struct Renderer {
    session_id: String,
    root: String,
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
    text_renderers: Vec<TextRenderer>,
    rasters: raster::RasterGpu,
    text_viewport: Viewport,
    nodes: HashMap<String, Node>,
    order: Vec<String>,
    layers: HashMap<String, usize>,
    edges: Vec<Edge>,
    spatial: SpatialIndex,
    buffers: HashMap<String, Buffer>,
    edge_buffers: HashMap<String, Buffer>,
    stats: Stats,
    window: Arc<WebviewWindow>,
}

impl Renderer {
    async fn new(window: WebviewWindow, session_id: String, root: String) -> Result<Self, String> {
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
        let rasters = raster::RasterGpu::new(&device, format, &bind_layout);
        Ok(Self {
            session_id,
            root,
            rasters,
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
            text_renderers: vec![text_renderer],
            text_viewport,
            nodes: HashMap::new(),
            order: Vec::new(),
            layers: HashMap::new(),
            edges: Vec::new(),
            spatial: SpatialIndex::default(),
            buffers: HashMap::new(),
            edge_buffers: HashMap::new(),
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
        if delta
            .edges
            .as_ref()
            .is_some_and(|edges| edges.len() > 100_000)
            || delta.edge_updates.len() > 100_000
            || delta.edges.is_some() && !delta.edge_updates.is_empty()
            || delta.reset && !delta.edge_updates.is_empty()
        {
            return Err("Invalid canvas edge delta".into());
        }
        let mut edge_ids = HashSet::new();
        if let Some(edges) = &delta.edges {
            for edge in edges {
                if !edge.valid() || !edge_ids.insert(edge.id.as_str()) {
                    return Err("Invalid prepared canvas edge".into());
                }
            }
        }
        edge_ids.clear();
        let existing_edge_ids: HashSet<_> =
            self.edges.iter().map(|edge| edge.id.as_str()).collect();
        for edge in &delta.edge_updates {
            if !edge.valid()
                || !edge_ids.insert(edge.id.as_str())
                || !existing_edge_ids.contains(edge.id.as_str())
            {
                return Err("Invalid prepared canvas edge update".into());
            }
        }
        if delta.reset {
            self.nodes.clear();
            self.order.clear();
            self.buffers.clear();
            self.edges.clear();
            self.edge_buffers.clear();
            self.rasters.reset();
        }
        for id in delta.removed {
            self.nodes.remove(&id);
            self.buffers.remove(&id);
            self.order.retain(|item| item != &id);
        }
        for node in delta.nodes {
            if node
                .image
                .as_ref()
                .is_some_and(|image| !image.valid_for(&node))
            {
                return Err("Invalid raster reference".into());
            }
            if ![node.x, node.y, node.width, node.height]
                .iter()
                .all(|value| value.is_finite() && value.abs() <= 10_000_000.)
                || node.width <= 0.
                || node.height <= 0.
            {
                return Err("Invalid canvas object geometry".into());
            }
            if node
                .style
                .as_ref()
                .is_some_and(|style| !style.valid_for(&node))
            {
                return Err("Invalid canvas object style".into());
            }
            if !self.nodes.contains_key(&node.id) {
                self.order.push(node.id.clone());
            }
            if self.nodes.get(&node.id).is_none_or(|old| {
                old.text != node.text
                    || old.width != node.width
                    || old.height != node.height
                    || old.style != node.style
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
            self.edge_buffers.clear();
        } else if !delta.edge_updates.is_empty() {
            let mut changes: HashMap<_, _> = delta
                .edge_updates
                .into_iter()
                .map(|edge| (edge.id.clone(), edge))
                .collect();
            for edge in &mut self.edges {
                if let Some(next) = changes.remove(&edge.id) {
                    if next.label != edge.label
                        || next.label_rect.width != edge.label_rect.width
                        || next.label_rect.height != edge.label_rect.height
                    {
                        self.edge_buffers.remove(&edge.id);
                    }
                    *edge = next;
                }
            }
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
        let locked: HashSet<&str> = frame.locked.iter().map(String::as_str).collect();
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
        let mut visible_edge_ids = HashSet::new();
        for edge in &self.edges {
            if !world.intersects(Rect {
                x: edge.x,
                y: edge.y,
                width: edge.width,
                height: edge.height,
            }) {
                continue;
            }
            let color = if frame.selected_edge.as_deref() == Some(edge.id.as_str()) {
                0xb8a9ef
            } else {
                rgb(Some(&edge.color), 0x817aab)
            };
            let width = (edge.stroke_width * scale).max(0.75 * ratio);
            let mut phase = 0.;
            for segment in &edge.segments {
                let a = screen(segment[0], segment[1]);
                let b = screen(segment[2], segment[3]);
                instance(
                    &mut shapes,
                    [a[0], a[1], b[0], b[1]],
                    color,
                    color,
                    [width, 1., if edge.dashed { 14. * scale } else { 0. }, phase],
                );
                phase += (segment[2] - segment[0]).hypot(segment[3] - segment[1]) * scale;
            }
            for segment in &edge.heads {
                let a = screen(segment[0], segment[1]);
                let b = screen(segment[2], segment[3]);
                instance(
                    &mut shapes,
                    [a[0], a[1], b[0], b[1]],
                    color,
                    color,
                    [width, 1., 0., 0.],
                );
            }
            if zoom >= 0.5 && !edge.label.is_empty() {
                visible_edge_ids.insert(edge.id.clone());
                let r = edge.label_rect;
                let p = screen(r.x, r.y);
                instance(
                    &mut shapes,
                    [p[0], p[1], r.width * scale, r.height * scale],
                    0x211f28,
                    if frame.selected_edge.as_deref() == Some(edge.id.as_str()) {
                        0xb8a9ef
                    } else {
                        0x54485f
                    },
                    [4. * scale, 0., scale, 0.979],
                );
                if !self.edge_buffers.contains_key(&edge.id) {
                    let mut buffer = Buffer::new(&mut self.fonts, Metrics::new(14., 20.));
                    buffer.set_size(Some((r.width - 16.).max(1.)), Some((r.height - 8.).max(1.)));
                    buffer.set_wrap(Wrap::None);
                    buffer.set_text(
                        &edge.label,
                        &Attrs::new()
                            .family(Family::Name("Segoe UI"))
                            .metadata(usize::MAX),
                        Shaping::Advanced,
                        Some(Align::Center),
                    );
                    buffer.shape_until_scroll(&mut self.fonts, false);
                    self.edge_buffers.insert(edge.id.clone(), buffer);
                }
            }
            edge_count += 1;
        }
        let prelude = (shapes.len() / 16) as u32;
        let mut spans = Vec::new();
        let ids: Vec<&String> = self
            .order
            .iter()
            .filter(|id| visible.contains(*id))
            .collect();
        self.rasters
            .visible(&self.nodes, &ids, zoom >= 0.5 && !frame.overlay_nodes);
        for id in &ids {
            let Some(node) = self.nodes.get(*id) else {
                continue;
            };
            if frame.overlay_nodes {
                let at = (shapes.len() / 16) as u32;
                spans.push((at, at, false));
                continue;
            }
            let shape_start = (shapes.len() / 16) as u32;
            let position = screen(node.x, node.y);
            instance_alpha(
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
                    node.style
                        .as_ref()
                        .map_or(8. * scale.min(2.), |s| s.radius * scale),
                    node.style.as_ref().map_or(0., |s| s.shape as f32),
                    if selected.contains(id.as_str()) {
                        (2. * ratio).max(
                            node.style
                                .as_ref()
                                .map_or(ratio, |s| s.stroke_width * scale),
                        )
                    } else {
                        node.style.as_ref().map_or(ratio, |s| {
                            if s.shape == 6 {
                                0.
                            } else {
                                s.stroke_width * scale
                            }
                        })
                    },
                    layer_depth(*self.layers.get(*id).unwrap_or(&0), self.order.len()),
                ],
                opacity(node.fill.as_deref()),
            );
            if zoom >= 0.5
                && let Some(image) = &node.image
            {
                let r = image.bounds;
                let p = screen(node.x + r.x, node.y + r.y);
                instance(
                    &mut shapes,
                    [p[0], p[1], r.width * scale, r.height * scale],
                    0x202024,
                    0x202024,
                    [
                        4. * scale,
                        0.,
                        0.,
                        layer_depth(*self.layers.get(*id).unwrap_or(&0), self.order.len())
                            - 0.00000025,
                    ],
                );
            }
            if selected.contains(id.as_str()) && !locked.contains(id.as_str()) {
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
            let alpha = opacity(node.fill.as_deref());
            spans.push((
                shape_start,
                (shapes.len() / 16) as u32,
                alpha > 0. && alpha < 1. || node.image.is_some() && alpha < 1.,
            ));
            if zoom < 0.5
                || node.width < 30.
                || node.height < 30.
                || node.style.as_ref().is_some_and(|s| s.shape == 7)
            {
                continue;
            }
            if !self.buffers.contains_key(*id) {
                let font_size = node.style.as_ref().map_or(16., |s| s.font_size);
                let rect = node.text_rect();
                let mut buffer =
                    Buffer::new(&mut self.fonts, Metrics::new(font_size, font_size * 1.55));
                buffer.set_size(Some(rect.width), Some(rect.height));
                buffer.set_text(
                    &node.text,
                    &Attrs::new()
                        .family(Family::Name("Segoe UI"))
                        .metadata(*self.layers.get(*id).unwrap_or(&0)),
                    Shaping::Advanced,
                    node.style.as_ref().map(|s| match s.align.as_str() {
                        "center" => Align::Center,
                        "right" => Align::Right,
                        _ => Align::Left,
                    }),
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
        let batches = composition_batches(prelude, &spans, zoom >= 0.5 && !frame.overlay_nodes);
        self.text_renderers.truncate(batches.len());
        while self.text_renderers.len() < batches.len() {
            self.text_renderers.push(TextRenderer::new(
                &mut self.atlas,
                &self.device,
                Default::default(),
                Some(depth_state(false)),
            ));
        }
        let total_layers = self.order.len();
        self.rasters.prepare(
            &self.device,
            &self.queue,
            raster::RasterFrame {
                batches: &batches,
                ids: &ids,
                nodes: &self.nodes,
                layers: &self.layers,
                scale,
                screen: &screen,
            },
        );
        for (batch_index, batch) in batches.iter().enumerate() {
            let mut text_areas = Vec::new();
            // Edge labels precede transparent text nodes even in an opaque batch.
            if batch.edges {
                for edge in &self.edges {
                    if !visible_edge_ids.contains(&edge.id) {
                        continue;
                    }
                    let Some(buffer) = self.edge_buffers.get(&edge.id) else {
                        continue;
                    };
                    let r = edge.label_rect;
                    let p = screen(r.x + 8., r.y + 4.);
                    text_areas.push(TextArea {
                        buffer,
                        left: p[0],
                        top: p[1],
                        scale,
                        bounds: TextBounds {
                            left: (vp.x * ratio).max(p[0]) as i32,
                            top: (vp.y * ratio).max(p[1]) as i32,
                            right: ((vp.x + vp.width) * ratio).min(p[0] + (r.width - 16.) * scale)
                                as i32,
                            bottom: ((vp.y + vp.height) * ratio).min(p[1] + (r.height - 8.) * scale)
                                as i32,
                        },
                        default_color: Color::rgb(222, 214, 232),
                        custom_glyphs: &[],
                    });
                }
            }
            text_areas.extend(ids[batch.nodes.clone()].iter().filter_map(|id| {
                if zoom < 0.5
                    || frame.overlay_text
                    || frame.overlay_nodes
                    || frame.editing.as_ref() == Some(*id)
                {
                    return None;
                }
                let node = self.nodes.get(*id)?;
                if node.width < 30. || node.height < 30. {
                    return None;
                }
                let buffer = self.buffers.get(*id)?;
                let rect = node.text_rect();
                let content_height = buffer
                    .layout_runs()
                    .map(|run| run.line_top + run.line_height)
                    .fold(0., f32::max);
                let center_offset = if node.style.as_ref().is_some_and(|s| s.center_text) {
                    ((rect.height - content_height) / 2.).max(0.)
                } else {
                    0.
                };
                let position = screen(node.x + rect.x, node.y + rect.y + center_offset);
                let text_color = rgb(node.style.as_ref().map(|s| s.text_color.as_str()), 0xdad8e0);
                Some(TextArea {
                    buffer,
                    left: position[0],
                    top: position[1],
                    scale,
                    bounds: TextBounds {
                        left: (vp.x * ratio).max(position[0]) as i32,
                        top: (vp.y * ratio).max(position[1]) as i32,
                        right: ((vp.x + vp.width) * ratio).min(position[0] + rect.width * scale)
                            as i32,
                        bottom: ((vp.y + vp.height) * ratio).min(position[1] + rect.height * scale)
                            as i32,
                    },
                    default_color: Color::rgb(
                        (text_color >> 16) as u8,
                        (text_color >> 8) as u8,
                        text_color as u8,
                    ),
                    custom_glyphs: &[],
                })
            }));
            self.text_renderers[batch_index]
                .prepare_with_depth(
                    &self.device,
                    &self.queue,
                    &mut self.fonts,
                    &mut self.atlas,
                    &self.text_viewport,
                    text_areas,
                    &mut self.swash,
                    |metadata| {
                        if metadata == usize::MAX {
                            0.978
                        } else {
                            layer_depth(metadata, total_layers) - 0.000001
                        }
                    },
                )
                .map_err(|error| error.to_string())?;
        }
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
                for (batch_index, batch) in batches.iter().enumerate() {
                    pass.set_pipeline(&self.pipeline);
                    pass.set_bind_group(0, &self.bind_group, &[]);
                    pass.set_vertex_buffer(0, self.instances.slice(..));
                    pass.draw(0..6, batch.shapes.clone());
                    self.rasters
                        .render(batch_index, &self.bind_group, &mut pass);
                    self.text_renderers[batch_index]
                        .render(&self.atlas, &self.text_viewport, &mut pass)
                        .map_err(|error| error.to_string())?;
                }
            }
        }
        self.queue.submit(Some(encoder.finish()));
        self.queue.present(texture);
        self.atlas.trim();
        if self.buffers.len() > 6000 {
            self.buffers.retain(|id, _| visible.contains(id));
        }
        if self.edge_buffers.len() > 6000 {
            self.edge_buffers
                .retain(|id, _| visible_edge_ids.contains(id));
        }
        self.stats.visible_nodes = ids.len();
        self.stats.visible_edges = edge_count;
        self.stats.edge_labels = visible_edge_ids.len();
        self.stats.draw_batches = batches.len();
        self.stats.translucent_batches = batches.iter().filter(|batch| batch.translucent).count();
        self.stats.raster_images = self.rasters.rendered;
        self.stats.raster_bytes = self.rasters.bytes;
        self.stats.missing_images = self.rasters.missing();
        self.stats.raster_paths = self.rasters.paths();
        self.stats.raster_limited = self.rasters.limited;
        self.stats.frames += 1;
        self.stats.text_buffers = self.buffers.len();
        self.stats.frame_cpu_ms = start.elapsed().as_secs_f64() * 1000.;
        Ok(self.stats.clone())
    }
}

#[tauri::command]
pub async fn canvas_gpu_start(
    window: WebviewWindow,
    root: String,
    session_id: String,
    vault_state: State<'_, VaultState>,
    state: State<'_, GpuCanvasState>,
) -> Result<Stats, String> {
    if window.label() != "main"
        || session_id.is_empty()
        || session_id.len() > 128
        || current_vault(&vault_state)?.root().to_string_lossy() != root
    {
        return Err("Stale canvas scope".into());
    }
    let inner = Arc::clone(&state.inner);
    let target = window.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        let mut current = inner.lock().map_err(|error| error.to_string())?;
        if current
            .as_ref()
            .is_none_or(|renderer| renderer.session_id != session_id || renderer.root != root)
        {
            current.take();
            *current = Some(tauri::async_runtime::block_on(Renderer::new(
                target, session_id, root,
            ))?);
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
    session_id: String,
    delta: SceneDelta,
    state: State<'_, GpuCanvasState>,
) -> Result<(), String> {
    let inner = Arc::clone(&state.inner);
    tauri::async_runtime::spawn_blocking(move || {
        let mut locked = inner.lock().map_err(|error| error.to_string())?;
        let renderer = locked
            .as_mut()
            .filter(|renderer| renderer.session_id == session_id)
            .ok_or("Stale renderer session")?;
        renderer.update(delta)
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
pub async fn canvas_gpu_frame(
    session_id: String,
    frame: Frame,
    state: State<'_, GpuCanvasState>,
) -> Result<Stats, String> {
    let inner = Arc::clone(&state.inner);
    tauri::async_runtime::spawn_blocking(move || {
        let mut locked = inner.lock().map_err(|error| error.to_string())?;
        let renderer = locked
            .as_mut()
            .filter(|renderer| renderer.session_id == session_id)
            .ok_or("Stale renderer session")?;
        renderer.render(frame)
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
pub async fn canvas_gpu_stop(
    window: WebviewWindow,
    session_id: Option<String>,
    state: State<'_, GpuCanvasState>,
) -> Result<(), String> {
    let inner = Arc::clone(&state.inner);
    let stopped = tauri::async_runtime::spawn_blocking(move || {
        let mut locked = inner.lock().map_err(|error| error.to_string())?;
        if locked.as_ref().is_some_and(|renderer| {
            session_id
                .as_ref()
                .is_none_or(|id| &renderer.session_id == id)
        }) {
            locked.take();
            return Ok::<_, String>(true);
        }
        Ok(false)
    })
    .await
    .map_err(|error| error.to_string())??;
    if stopped {
        window
            .set_background_color(Some(tauri::window::Color(30, 30, 30, 255)))
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RasterRequest {
    root: String,
    session_id: String,
    relative_path: String,
    force: bool,
}
#[tauri::command]
pub async fn canvas_gpu_image(
    window: WebviewWindow,
    request: RasterRequest,
    vault_state: State<'_, VaultState>,
    jobs: State<'_, AttachmentState>,
    state: State<'_, GpuCanvasState>,
) -> Result<String, String> {
    let RasterRequest {
        root,
        session_id,
        relative_path,
        force,
    } = request;
    let vault = current_vault(&vault_state)?;
    if window.label() != "main" || vault.root().to_string_lossy() != root {
        return Err("Stale canvas scope".into());
    }
    {
        let mut locked = state.inner.lock().map_err(|_| "Renderer unavailable")?;
        let renderer = locked
            .as_mut()
            .filter(|r| {
                r.session_id == session_id && r.root == root && r.rasters.wanted(&relative_path)
            })
            .ok_or("Stale or invisible raster reference")?;
        if force {
            renderer.rasters.invalidate(&relative_path);
        } else if renderer.rasters.loaded(&relative_path) {
            return Ok("cached".into());
        }
    }
    let Ok(permit) = Arc::clone(&jobs.previews).try_acquire_owned() else {
        return Ok("busy".into());
    };
    let path = relative_path.clone();
    let raster = tauri::async_runtime::spawn_blocking(move || {
        let _permit = permit;
        vault
            .attachment_raster(&path)
            .map_err(|_| "Превью недоступно; оригинал сохранён".to_owned())
    })
    .await
    .map_err(|_| "Preview worker unavailable")??;
    let current = vault_state.vault.read().map_err(|_| "Vault unavailable")?;
    if current
        .as_ref()
        .is_none_or(|vault| vault.root().to_string_lossy() != root)
    {
        return Err("Stale canvas scope".into());
    }
    let mut locked = state.inner.lock().map_err(|_| "Renderer unavailable")?;
    let renderer = locked
        .as_mut()
        .filter(|r| {
            r.session_id == session_id && r.root == root && r.rasters.wanted(&relative_path)
        })
        .ok_or("Stale or invisible raster reference")?;
    renderer
        .rasters
        .upload(&renderer.device, &renderer.queue, relative_path, raster)?;
    Ok("loaded".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn raster_reference_cannot_escape_card_bounds_or_attach_to_a_non_file_node() {
        let mut node: Node = serde_json::from_value(
            serde_json::json!({"id":"image","kind":"file","x":0,"y":0,"width":300,"height":230}),
        )
        .unwrap();
        let valid = RasterRef {
            path: "Attachments/Photo.png".into(),
            bounds: Rect {
                x: 8.,
                y: 8.,
                width: 284.,
                height: 180.,
            },
        };
        assert!(valid.valid_for(&node));
        node.kind = "text".into();
        assert!(!valid.valid_for(&node));
        node.kind = "file".into();
        for bounds in [
            Rect {
                x: -1.,
                ..valid.bounds
            },
            Rect {
                width: 1000.,
                ..valid.bounds
            },
            Rect {
                height: 0.,
                ..valid.bounds
            },
        ] {
            assert!(
                !RasterRef {
                    bounds,
                    ..valid.clone()
                }
                .valid_for(&node)
            );
        }
    }
    #[test]
    fn opaque_composition_keeps_one_batch_and_source_order() {
        assert_eq!(
            composition_batches(1, &[(1, 2, false), (2, 3, false), (3, 4, false)], true),
            vec![DrawBatch {
                shapes: 0..4,
                nodes: 0..3,
                edges: true,
                translucent: false
            }]
        );
    }
    #[test]
    fn translucent_cuts_interleave_lower_glyphs_fill_own_label_and_upper_glyphs() {
        let spans = [(1, 2, false), (2, 3, true), (3, 4, true), (4, 5, false)];
        assert_eq!(
            composition_batches(1, &spans, true),
            vec![
                DrawBatch {
                    shapes: 0..2,
                    nodes: 0..1,
                    edges: true,
                    translucent: false
                },
                DrawBatch {
                    shapes: 2..3,
                    nodes: 1..2,
                    edges: false,
                    translucent: true
                },
                DrawBatch {
                    shapes: 3..4,
                    nodes: 2..3,
                    edges: false,
                    translucent: true
                },
                DrawBatch {
                    shapes: 4..5,
                    nodes: 3..4,
                    edges: false,
                    translucent: false
                }
            ]
        );
        assert_eq!(
            composition_batches(1, &spans, false),
            vec![DrawBatch {
                shapes: 0..5,
                nodes: 0..4,
                edges: true,
                translucent: false
            }]
        );
    }
    #[test]
    fn translucent_first_last_and_bounded_dom_overlay_do_not_duplicate_or_drop_ranges() {
        let batches = composition_batches(3, &[(3, 4, true), (4, 5, false), (5, 6, true)], true);
        assert_eq!(
            batches
                .iter()
                .flat_map(|b| b.shapes.clone())
                .collect::<Vec<_>>(),
            (0..6).collect::<Vec<_>>()
        );
        assert_eq!(
            batches
                .iter()
                .flat_map(|b| b.nodes.clone())
                .collect::<Vec<_>>(),
            (0..3).collect::<Vec<_>>()
        );
        assert_eq!(batches.iter().filter(|b| b.edges).count(), 1);
        assert_eq!(
            composition_batches(3, &[(3, 3, false), (3, 3, false)], false),
            vec![DrawBatch {
                shapes: 0..3,
                nodes: 0..2,
                edges: true,
                translucent: false
            }]
        );
    }
    #[test]
    fn shared_connector_geometry_fixtures_are_consumed_without_a_second_anchor_resolver() {
        let fixtures: Vec<serde_json::Value> = serde_json::from_str(include_str!(
            "../../../../tests/desktop/fixtures/r3a-edge-geometry.json"
        ))
        .unwrap();
        for fixture in fixtures {
            let edges: Vec<Edge> = serde_json::from_value(fixture["prepared"].clone()).unwrap();
            assert!(edges.iter().all(Edge::valid));
            let edge = &edges[0];
            match fixture["name"].as_str().unwrap() {
                "default" => {
                    assert_eq!(edge.segments, vec![[100., 50., 300., 50.]]);
                    assert_eq!(edge.heads.len(), 2);
                    assert!(!edge.dashed)
                }
                "explicit" => {
                    assert_eq!(edge.segments, vec![[50., 100., 350., 0.]]);
                    assert_eq!(edge.stroke_width, 4.);
                    assert!(edge.dashed);
                    assert_eq!(edge.label, "Semantic label")
                }
                "loop" => assert_eq!(edge.segments.len(), 4),
                _ => panic!("Unknown fixture"),
            }
        }
    }
    #[test]
    fn malformed_prepared_connectors_fail_before_geometry_or_font_allocation() {
        let fixtures: serde_json::Value = serde_json::from_str(include_str!(
            "../../../../tests/desktop/fixtures/r3a-edge-geometry.json"
        ))
        .unwrap();
        let source = fixtures[0]["prepared"][0].clone();
        for (key, value) in [
            ("segments", serde_json::json!([])),
            ("heads", serde_json::json!([[0, 0, 0, 0]])),
            ("strokeWidth", serde_json::json!(10000)),
            ("color", serde_json::json!("url(x)")),
            ("label", serde_json::json!("x".repeat(4097))),
            (
                "labelRect",
                serde_json::json!({"x":0,"y":0,"width":1000000,"height":30}),
            ),
        ] {
            let mut malformed = source.clone();
            malformed[key] = value;
            let edge: Edge = serde_json::from_value(malformed).unwrap();
            assert!(!edge.valid(), "{key}")
        }
    }
    #[test]
    fn primitive_style_dto_has_bounded_font_geometry_and_backward_compatible_legacy_nodes() {
        let mut node: Node = serde_json::from_value(
            serde_json::json!({"id":"n","x":0,"y":0,"width":260,"height":180}),
        )
        .unwrap();
        assert!(node.style.is_none());
        assert_eq!(node.text_rect().x, 20.);
        for shape in [0, 3, 4, 5, 6, 7] {
            let style:NodeStyle=serde_json::from_value(serde_json::json!({"shape":shape,"radius":0,"fontSize":24,"align":"right","textColor":"#ffeedd","strokeWidth":4,"bounds":{"x":65,"y":90,"width":130,"height":60}})).unwrap();
            assert!(style.valid_for(&node));
            node.style = Some(style);
            assert_eq!(node.text_rect().width, 130.);
        }
    }
    #[test]
    fn malformed_primitive_style_is_refused_before_font_shaping() {
        let node: Node = serde_json::from_value(
            serde_json::json!({"id":"n","x":0,"y":0,"width":260,"height":180}),
        )
        .unwrap();
        let source = serde_json::json!({"shape":3,"radius":0,"fontSize":16,"align":"left","textColor":"#ffeedd","strokeWidth":2,"bounds":{"x":20,"y":20,"width":220,"height":140}});
        for (key, value) in [
            ("shape", serde_json::json!(2)),
            ("fontSize", serde_json::json!(1000000)),
            ("align", serde_json::json!("script")),
            ("textColor", serde_json::json!("url(x)")),
            ("strokeWidth", serde_json::json!(0)),
            (
                "bounds",
                serde_json::json!({"x":-1,"y":20,"width":220,"height":140}),
            ),
        ] {
            let mut value_source = source.clone();
            value_source[key] = value;
            let style: NodeStyle = serde_json::from_value(value_source).unwrap();
            assert!(!style.valid_for(&node));
        }
    }
    #[test]
    fn rgba_group_fill_preserves_lines_behind_its_background_without_changing_border_alpha() {
        assert_eq!(rgb(Some("#322b3e33"), 0), 0x322b3e);
        assert!((opacity(Some("#322b3e33")) - 0.2).abs() < f32::EPSILON);
        assert_eq!(opacity(Some("#322b3e")), 1.);
        assert_eq!(rgb(Some("#🧠abcd"), 7), 7);
        let mut data = Vec::new();
        instance_alpha(
            &mut data,
            [0., 0., 100., 100.],
            0x322b3e,
            0x9381ba,
            [0.; 4],
            0.2,
        );
        assert_eq!(data[7], 0.2);
        assert_eq!(data[11], 1.);
    }
    #[test]
    fn locked_selection_frame_is_backward_compatible_and_keeps_render_policy_separate_from_scene() {
        let value = serde_json::json!({"viewport":{"x":0,"y":0,"width":900,"height":600},"camera":{"x":0,"y":0,"zoom":1},"pixelRatio":1,"selected":["child"],"editing":null});
        let old: Frame = serde_json::from_value(value.clone()).unwrap();
        assert!(old.locked.is_empty());
        assert!(!old.overlay_text);
        let mut value = value;
        value["locked"] = serde_json::json!(["child"]);
        value["overlayText"] = serde_json::json!(true);
        let new: Frame = serde_json::from_value(value).unwrap();
        assert_eq!(new.locked, new.selected);
        assert!(new.overlay_text);
    }
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
                style: None,
                image: None,
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
                style: None,
                image: None,
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

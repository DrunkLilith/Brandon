//! Bounded native raster cache. Original files and all decoding stay in Rust.
use super::{DrawBatch, Node, Rect, bytes, depth_state, instance};
use knowledge_vault::AttachmentRaster;
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    ops::Range,
};

pub(super) const PIXEL_BUDGET: usize = 64 * 1024 * 1024;
const MAX_TEXTURES: usize = 128;
const MAX_VISIBLE_PATHS: usize = 64;
struct CachedRaster {
    bind: wgpu::BindGroup,
    width: u32,
    height: u32,
    bytes: usize,
    used: u64,
}
struct ImageDraw {
    path: String,
    instances: Range<u32>,
}
pub(super) struct RasterFrame<'a> {
    pub batches: &'a [DrawBatch],
    pub ids: &'a [&'a String],
    pub nodes: &'a HashMap<String, Node>,
    pub layers: &'a HashMap<String, usize>,
    pub scale: f32,
    pub screen: &'a dyn Fn(f32, f32) -> [f32; 2],
}
pub(super) struct RasterGpu {
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    instances: wgpu::Buffer,
    capacity: u64,
    textures: HashMap<String, CachedRaster>,
    wanted: Vec<String>,
    draws: Vec<Vec<ImageDraw>>,
    pub(super) bytes: usize,
    pub(super) rendered: usize,
    pub(super) limited: usize,
    clock: u64,
}
pub(super) fn fit(bounds: Rect, width: u32, height: u32) -> Rect {
    let scale = (bounds.width / width as f32).min(bounds.height / height as f32);
    let w = width as f32 * scale;
    let h = height as f32 * scale;
    Rect {
        x: bounds.x + (bounds.width - w) / 2.,
        y: bounds.y + (bounds.height - h) / 2.,
        width: w,
        height: h,
    }
}
fn eviction<'a>(
    items: impl Iterator<Item = (&'a str, u64)>,
    wanted: &HashSet<&str>,
) -> Option<String> {
    items
        .filter(|(path, _)| !wanted.contains(path))
        .min_by_key(|(_, used)| *used)
        .map(|(path, _)| path.to_owned())
}
impl RasterGpu {
    pub(super) fn new(
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        uniform: &wgpu::BindGroupLayout,
    ) -> Self {
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Canvas raster texture"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Canvas raster filtering"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Canvas native raster"),
            source: wgpu::ShaderSource::Wgsl(include_str!("canvas_image.wgsl").into()),
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[Some(uniform), Some(&layout)],
            immediate_size: 0,
        });
        let pipeline=device.create_render_pipeline(&wgpu::RenderPipelineDescriptor{label:Some("Canvas native raster"),layout:Some(&pipeline_layout),vertex:wgpu::VertexState{module:&shader,entry_point:Some("vertex"),compilation_options:Default::default(),buffers:&[Some(wgpu::VertexBufferLayout{array_stride:64,step_mode:wgpu::VertexStepMode::Instance,attributes:&wgpu::vertex_attr_array![0=>Float32x4,1=>Float32x4,2=>Float32x4,3=>Float32x4]})]},fragment:Some(wgpu::FragmentState{module:&shader,entry_point:Some("fragment"),compilation_options:Default::default(),targets:&[Some(wgpu::ColorTargetState{format,blend:Some(wgpu::BlendState::ALPHA_BLENDING),write_mask:wgpu::ColorWrites::ALL})]}),primitive:Default::default(),depth_stencil:Some(depth_state(true)),multisample:Default::default(),multiview_mask:None,cache:None});
        let instances = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Canvas image instances"),
            size: 65536,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        Self {
            pipeline,
            layout,
            sampler,
            instances,
            capacity: 65536,
            textures: HashMap::new(),
            wanted: Vec::new(),
            draws: Vec::new(),
            bytes: 0,
            rendered: 0,
            limited: 0,
            clock: 0,
        }
    }
    pub(super) fn reset(&mut self) {
        self.textures.clear();
        self.wanted.clear();
        self.bytes = 0;
        self.draws.clear();
    }
    pub(super) fn visible(
        &mut self,
        nodes: &HashMap<String, Node>,
        ids: &[&String],
        enabled: bool,
    ) {
        self.clock += 1;
        self.wanted.clear();
        let mut distinct = HashSet::new();
        if enabled {
            for id in ids.iter().rev() {
                if let Some(image) = nodes.get(*id).and_then(|node| node.image.as_ref())
                    && distinct.insert(image.path.as_str())
                    && self.wanted.len() < MAX_VISIBLE_PATHS
                {
                    self.wanted.push(image.path.clone());
                }
            }
        }
        self.limited = distinct.len().saturating_sub(MAX_VISIBLE_PATHS);
        for path in &self.wanted {
            if let Some(texture) = self.textures.get_mut(path) {
                texture.used = self.clock;
            }
        }
    }
    pub(super) fn wanted(&self, path: &str) -> bool {
        self.wanted.iter().any(|p| p == path)
    }
    pub(super) fn loaded(&self, path: &str) -> bool {
        self.textures.contains_key(path)
    }
    pub(super) fn paths(&self) -> Vec<String> {
        self.wanted.clone()
    }
    pub(super) fn missing(&self) -> Vec<String> {
        self.wanted
            .iter()
            .filter(|p| !self.textures.contains_key(*p))
            .cloned()
            .collect()
    }
    pub(super) fn invalidate(&mut self, path: &str) {
        if let Some(old) = self.textures.remove(path) {
            self.bytes = self.bytes.saturating_sub(old.bytes);
        }
    }
    pub(super) fn upload(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        path: String,
        raster: AttachmentRaster,
    ) -> Result<(), String> {
        if !self.wanted(&path)
            || raster.width == 0
            || raster.height == 0
            || raster.width > 640
            || raster.height > 400
            || raster.rgba.len() != raster.width as usize * raster.height as usize * 4
        {
            return Err("Invalid or stale raster preview".into());
        }
        self.invalidate(&path);
        let wanted: HashSet<_> = self.wanted.iter().map(String::as_str).collect();
        while self.bytes + raster.rgba.len() > PIXEL_BUDGET || self.textures.len() >= MAX_TEXTURES {
            let Some(victim) = eviction(
                self.textures.iter().map(|(p, t)| (p.as_str(), t.used)),
                &wanted,
            ) else {
                return Err("Raster cache budget exhausted".into());
            };
            let old = self
                .textures
                .remove(&victim)
                .ok_or("Raster cache changed")?;
            self.bytes -= old.bytes;
        }
        let size = wgpu::Extent3d {
            width: raster.width,
            height: raster.height,
            depth_or_array_layers: 1,
        };
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Vault raster preview"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &raster.rgba,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(raster.width * 4),
                rows_per_image: Some(raster.height),
            },
            size,
        );
        let view = texture.create_view(&Default::default());
        let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Vault raster preview"),
            layout: &self.layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
            ],
        });
        self.bytes += raster.rgba.len();
        self.textures.insert(
            path,
            CachedRaster {
                bind,
                width: raster.width,
                height: raster.height,
                bytes: raster.rgba.len(),
                used: self.clock,
            },
        );
        Ok(())
    }
    pub(super) fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        frame: RasterFrame<'_>,
    ) {
        self.draws.clear();
        self.rendered = 0;
        if self.wanted.is_empty() {
            return;
        }
        let RasterFrame {
            batches,
            ids,
            nodes,
            layers,
            scale,
            screen,
        } = frame;
        let mut instances = Vec::new();
        for batch in batches {
            let mut by_path: BTreeMap<&str, Vec<&Node>> = BTreeMap::new();
            for id in &ids[batch.nodes.clone()] {
                if let Some(node) = nodes.get(*id)
                    && let Some(image) = &node.image
                    && self.wanted(&image.path)
                    && self.loaded(&image.path)
                {
                    by_path.entry(&image.path).or_default().push(node);
                }
            }
            let mut draws = Vec::new();
            for (path, nodes_for_image) in by_path {
                let Some(texture) = self.textures.get(path) else {
                    continue;
                };
                let start = (instances.len() / 16) as u32;
                for node in nodes_for_image {
                    let Some(image) = &node.image else { continue };
                    let rect = fit(image.bounds, texture.width, texture.height);
                    let p = screen(node.x + rect.x, node.y + rect.y);
                    let stage = screen(node.x + image.bounds.x, node.y + image.bounds.y);
                    let depth =
                        super::layer_depth(*layers.get(&node.id).unwrap_or(&0), layers.len())
                            - 0.0000005;
                    instance(
                        &mut instances,
                        [p[0], p[1], rect.width * scale, rect.height * scale],
                        0,
                        0,
                        [4. * scale, 0., 0., depth],
                    );
                    let at = instances.len() - 16;
                    instances[at + 4..at + 8].copy_from_slice(&[
                        stage[0],
                        stage[1],
                        image.bounds.width * scale,
                        image.bounds.height * scale,
                    ]);
                    self.rendered += 1;
                }
                draws.push(ImageDraw {
                    path: path.to_owned(),
                    instances: start..(instances.len() / 16) as u32,
                });
            }
            self.draws.push(draws);
        }
        let data = bytes(&instances);
        if data.len() as u64 > self.capacity {
            self.capacity = (data.len() as u64).next_power_of_two();
            self.instances = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("Canvas image instances"),
                size: self.capacity,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
        }
        if !data.is_empty() {
            queue.write_buffer(&self.instances, 0, &data);
        }
    }
    pub(super) fn render<'a>(
        &'a self,
        batch: usize,
        uniform: &'a wgpu::BindGroup,
        pass: &mut wgpu::RenderPass<'a>,
    ) {
        let Some(draws) = self.draws.get(batch) else {
            return;
        };
        if draws.is_empty() {
            return;
        }
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, uniform, &[]);
        pass.set_vertex_buffer(0, self.instances.slice(..));
        for draw in draws {
            if let Some(texture) = self.textures.get(&draw.path) {
                pass.set_bind_group(1, &texture.bind, &[]);
                pass.draw(0..6, draw.instances.clone());
            }
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fitting_preserves_aspect_and_stays_in_stage() {
        let b = Rect {
            x: 8.,
            y: 8.,
            width: 300.,
            height: 200.,
        };
        assert_eq!(
            fit(b, 100, 100),
            Rect {
                x: 58.,
                y: 8.,
                width: 200.,
                height: 200.
            }
        );
        assert_eq!(
            fit(b, 600, 200),
            Rect {
                x: 8.,
                y: 58.,
                width: 300.,
                height: 100.
            }
        );
    }
    #[test]
    fn eviction_never_drops_a_current_visible_asset() {
        let wanted = HashSet::from(["a", "b"]);
        assert_eq!(
            eviction([("a", 0), ("b", 1), ("old", 9)].into_iter(), &wanted),
            Some("old".to_owned())
        );
        assert_eq!(eviction([("a", 0)].into_iter(), &wanted), None);
        const { assert!(MAX_VISIBLE_PATHS * 640 * 400 * 4 <= PIXEL_BUDGET) };
    }
}

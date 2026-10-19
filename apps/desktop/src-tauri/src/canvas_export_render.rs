//! Bounded offscreen CPU composition. No window capture, URL loader or filesystem access.
use crate::canvas_render_scene::{Edge, Node, Rect, SceneDelta, validate_scene};
use cosmic_text::{
    Align, Attrs, Buffer, Color as TextColor, Family, FontSystem, Metrics, Shaping, SwashCache,
    Wrap,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeSet, HashMap},
    sync::atomic::{AtomicBool, Ordering},
};
use tiny_skia::{
    Color, FillRule, IntSize, Paint, Path, PathBuilder, Pixmap, PixmapPaint, Rect as DrawRect,
    Stroke, StrokeDash, Transform,
};

pub const MAX_NODES: usize = 2000;
pub const MAX_EDGES: usize = 4000;
pub const MAX_RASTER_BYTES: usize = 64 * 1024 * 1024;
const MAX_TEXT: usize = 2 * 1024 * 1024;
const MAX_GLYPHS: usize = 250000;
#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportScene {
    pub version: u32,
    pub bounds: Rect,
    pub scale: u32,
    pub transparent: bool,
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
}
#[derive(Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct RenderReport {
    pub width: u32,
    pub height: u32,
    pub nodes: usize,
    pub edges: usize,
    pub ink_strokes: usize,
    pub glyphs: usize,
    pub missing_glyphs: usize,
    pub font_families: Vec<String>,
    pub clipped_text: Vec<String>,
    pub missing_rasters: Vec<String>,
}
pub struct Rendered {
    pub png: Vec<u8>,
    pub preview: Vec<u8>,
    pub report: RenderReport,
}
pub struct RasterPixels {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}
fn cancelled(cancel: &AtomicBool) -> Result<(), String> {
    if cancel.load(Ordering::Relaxed) {
        Err("Экспорт отменён".into())
    } else {
        Ok(())
    }
}
fn paint(value: &str, opacity: f32) -> Paint<'static> {
    let rgba = parse_color(value);
    let mut paint = Paint::default();
    paint.set_color_rgba8(
        rgba[0],
        rgba[1],
        rgba[2],
        (f32::from(rgba[3]) * opacity).round() as u8,
    );
    paint.anti_alias = true;
    paint
}
fn parse_color(s: &str) -> [u8; 4] {
    let value = s.trim_start_matches('#');
    let number = u32::from_str_radix(value, 16).unwrap_or(0);
    if value.len() == 8 {
        [
            (number >> 24) as u8,
            (number >> 16) as u8,
            (number >> 8) as u8,
            number as u8,
        ]
    } else {
        [(number >> 16) as u8, (number >> 8) as u8, number as u8, 255]
    }
}
fn valid_color(s: &str) -> bool {
    matches!(s.len(), 7 | 9) && s.starts_with('#') && s[1..].bytes().all(|b| b.is_ascii_hexdigit())
}
pub fn dimensions(scene: &ExportScene) -> Result<[u32; 2], String> {
    let b = scene.bounds;
    if scene.version != 1
        || ![b.x, b.y, b.width, b.height]
            .iter()
            .all(|v| v.is_finite() && v.abs() <= 10_000_000.)
        || b.width <= 0.
        || b.height <= 0.
        || ![1, 2].contains(&scene.scale)
    {
        return Err("Некорректные параметры PNG".into());
    }
    let width = (b.width * scene.scale as f32).ceil() as u32;
    let height = (b.height * scene.scale as f32).ceil() as u32;
    if width == 0
        || height == 0
        || width > 4096
        || height > 4096
        || u64::from(width) * u64::from(height) > 16 * 1024 * 1024
    {
        return Err("PNG превышает 4096 px по стороне или 16 Ми пикселей. Выберите меньший масштаб; автоматического уменьшения нет.".into());
    }
    Ok([width, height])
}
pub fn validate(scene: &ExportScene) -> Result<(), String> {
    let [width, height] = dimensions(scene)?;
    if scene.nodes.len() > MAX_NODES || scene.edges.len() > MAX_EDGES {
        return Err("Слишком много объектов или связей для одного PNG".into());
    }
    validate_scene(
        &HashMap::new(),
        &SceneDelta {
            reset: true,
            nodes: scene.nodes.clone(),
            ..Default::default()
        },
    )?;
    let mut text = 0usize;
    let mut work = 0f64;
    let mut edge_ids = BTreeSet::new();
    for node in &scene.nodes {
        if node.id.is_empty()
            || node.id.len() > 4096
            || !matches!(node.kind.as_str(), "text" | "group" | "file" | "link")
            || node.fill.as_deref().is_some_and(|s| !valid_color(s))
            || node.border.as_deref().is_some_and(|s| !valid_color(s))
        {
            return Err("Некорректный объект PNG".into());
        }
        if node.text.len() > 65536 {
            return Err("Текст одного объекта превышает 64 КиБ".into());
        }
        text += node.text.len();
        if let Some(r) = scene.bounds.intersection(Rect {
            x: node.x,
            y: node.y,
            width: node.width,
            height: node.height,
        }) {
            work += f64::from(r.width * scene.scale as f32).min(f64::from(width))
                * f64::from(r.height * scene.scale as f32).min(f64::from(height));
        }
    }
    if text > MAX_TEXT || work > 256. * 1024. * 1024. {
        return Err("Композиция превышает бюджет текста или закраски PNG".into());
    }
    for edge in &scene.edges {
        if !edge.valid() || !edge_ids.insert(&edge.id) {
            return Err("Некорректная связь PNG".into());
        }
        text += edge.label.len();
    }
    if text > MAX_TEXT {
        return Err("Текст PNG превышает 2 МиБ".into());
    }
    Ok(())
}
fn shape(node: &Node) -> Option<Path> {
    let (w, h) = (node.width, node.height);
    let style = node.style.as_ref();
    let kind = style.map_or(0, |s| s.shape);
    let mut p = PathBuilder::new();
    match kind {
        3 => p.push_oval(DrawRect::from_xywh(0., 0., w, h)?),
        4 => {
            p.move_to(w / 2., 0.);
            p.line_to(w, h / 2.);
            p.line_to(w / 2., h);
            p.line_to(0., h / 2.);
            p.close();
        }
        5 => {
            p.move_to(w / 2., 0.);
            p.line_to(w, h);
            p.line_to(0., h);
            p.close();
        }
        7 => {
            p.move_to(0., h / 2.);
            p.line_to(w, h / 2.);
        }
        _ => {
            let r = style.map_or(8., |s| s.radius).min(w / 2.).min(h / 2.);
            if r <= 0. {
                p.push_rect(DrawRect::from_xywh(0., 0., w, h)?);
            } else {
                let k = 0.5522848 * r;
                p.move_to(r, 0.);
                p.line_to(w - r, 0.);
                p.cubic_to(w - r + k, 0., w, r - k, w, r);
                p.line_to(w, h - r);
                p.cubic_to(w, h - r + k, w - r + k, h, w - r, h);
                p.line_to(r, h);
                p.cubic_to(r - k, h, 0., h - r + k, 0., h - r);
                p.line_to(0., r);
                p.cubic_to(0., r - k, r - k, 0., r, 0.);
                p.close();
            }
        }
    }
    p.finish()
}
fn ink_path(node: &Node) -> Option<Path> {
    let ink = node.ink.as_ref()?;
    let mut path = PathBuilder::new();
    for &[ax, ay, bx, by, ra, rb] in &ink.segments {
        let d = (bx - ax).hypot(by - ay);
        if d > 1e-9 {
            let nx = -(by - ay) / d;
            let ny = (bx - ax) / d;
            path.move_to(ax + nx * ra, ay + ny * ra);
            path.line_to(ax - nx * ra, ay - ny * ra);
            path.line_to(bx - nx * rb, by - ny * rb);
            path.line_to(bx + nx * rb, by + ny * rb);
            path.close();
        }
        path.push_circle(ax, ay, ra);
        path.push_circle(bx, by, rb);
    }
    path.finish()
}
fn transform(scene: &ExportScene, x: f32, y: f32) -> Transform {
    let s = scene.scale as f32;
    Transform::from_row(
        s,
        0.,
        0.,
        s,
        (x - scene.bounds.x) * s,
        (y - scene.bounds.y) * s,
    )
}
fn draw_shape(pixmap: &mut Pixmap, node: &Node, scene: &ExportScene) {
    if let Some(ink) = &node.ink {
        if let Some(path) = ink_path(node) {
            pixmap.fill_path(
                &path,
                &paint(&ink.color, ink.opacity),
                FillRule::Winding,
                transform(scene, node.x, node.y),
                None,
            );
        }
        return;
    }
    let Some(path) = shape(node) else { return };
    let kind = node.style.as_ref().map_or(0, |s| s.shape);
    let t = transform(scene, node.x, node.y);
    if kind != 6 && kind != 7 {
        pixmap.fill_path(
            &path,
            &paint(node.fill.as_deref().unwrap_or("#2b2732"), 1.),
            FillRule::Winding,
            t,
            None,
        );
    }
    if kind != 6 {
        pixmap.stroke_path(
            &path,
            &paint(node.border.as_deref().unwrap_or("#51465f"), 1.),
            &Stroke {
                width: node.style.as_ref().map_or(1., |s| s.stroke_width),
                ..Default::default()
            },
            t,
            None,
        );
    }
}
fn blend(pixmap: &mut Pixmap, x: i32, y: i32, color: TextColor, clip: DrawRect) {
    if x < 0
        || y < 0
        || x >= pixmap.width() as i32
        || y >= pixmap.height() as i32
        || x as f32 + 0.5 < clip.left()
        || y as f32 + 0.5 < clip.top()
        || x as f32 + 0.5 >= clip.right()
        || y as f32 + 0.5 >= clip.bottom()
    {
        return;
    }
    let a = u32::from(color.a());
    if a == 0 {
        return;
    }
    let index = (y as usize * pixmap.width() as usize + x as usize) * 4;
    let dst = &mut pixmap.data_mut()[index..index + 4];
    for (d, s) in dst[..3].iter_mut().zip([color.r(), color.g(), color.b()]) {
        *d = ((u32::from(s) * a + u32::from(*d) * (255 - a) + 127) / 255) as u8;
    }
    dst[3] = (a + (u32::from(dst[3]) * (255 - a) + 127) / 255) as u8;
}
struct TextRenderer {
    fonts: FontSystem,
    cache: SwashCache,
    families: BTreeSet<String>,
    report: RenderReport,
}
impl TextRenderer {
    fn draw(
        &mut self,
        pixmap: &mut Pixmap,
        node: &Node,
        scene: &ExportScene,
        cancel: &AtomicBool,
        wrap: bool,
    ) -> Result<(), String> {
        if node.ink.is_some()
            || node.style.as_ref().is_some_and(|s| s.shape == 7)
            || node.text.is_empty()
        {
            return Ok(());
        }
        let rect = node.text_rect();
        let size = node.style.as_ref().map_or(16., |s| s.font_size);
        let mut buffer = Buffer::new(&mut self.fonts, Metrics::new(size, size * 1.55));
        buffer.set_size(Some(rect.width), Some(rect.height));
        if !wrap {
            buffer.set_wrap(Wrap::None)
        }
        buffer.set_text(
            &node.text,
            &Attrs::new().family(Family::Name("Segoe UI")),
            Shaping::Advanced,
            node.style.as_ref().map(|s| match s.align.as_str() {
                "center" => Align::Center,
                "right" => Align::Right,
                _ => Align::Left,
            }),
        );
        buffer.shape_until_scroll(&mut self.fonts, false);
        let height = buffer
            .layout_runs()
            .map(|r| r.line_top + r.line_height)
            .fold(0., f32::max);
        let offset = if node.style.as_ref().is_some_and(|s| s.center_text) {
            ((rect.height - height) / 2.).max(0.)
        } else {
            0.
        };
        let scale = scene.scale as f32;
        let x = (node.x + rect.x - scene.bounds.x) * scale;
        let y = (node.y + rect.y + offset - scene.bounds.y) * scale;
        let clip = DrawRect::from_xywh(
            x,
            y,
            rect.width * scale,
            (rect.height - offset).max(0.0001) * scale,
        )
        .ok_or("Некорректная текстовая область")?;
        let rgba = parse_color(
            node.style
                .as_ref()
                .map_or("#dad8e0", |s| s.text_color.as_str()),
        );
        let base = TextColor::rgba(rgba[0], rgba[1], rgba[2], 255);
        let mut clipped = false;
        let mut last_line = None;
        for run in buffer.layout_runs() {
            cancelled(cancel)?;
            last_line = Some((
                run.line_i,
                run.glyphs.iter().map(|g| g.end).max().unwrap_or(0),
                run.text.len(),
            ));
            if run.line_top + run.line_height > rect.height + 0.01 || run.line_w > rect.width + 0.01
            {
                clipped = true
            }
            for glyph in run.glyphs {
                self.report.glyphs += 1;
                if self.report.glyphs > MAX_GLYPHS {
                    return Err("PNG превышает лимит 250000 глифов".into());
                }
                if glyph.glyph_id == 0 {
                    self.report.missing_glyphs += 1
                }
                if let Some(face) = self.fonts.db().face(glyph.font_id)
                    && let Some((family, _)) = face.families.first()
                {
                    self.families.insert(family.clone());
                }
                let physical = glyph.physical((x, y + run.line_y * scale), scale);
                let color = glyph.color_opt.unwrap_or(base);
                if !self.cache.image_cache.contains_key(&physical.cache_key) {
                    self.cache.get_image(&mut self.fonts, physical.cache_key);
                    if self.cache.image_cache.len() > 8192
                        || self
                            .cache
                            .image_cache
                            .values()
                            .filter_map(Option::as_ref)
                            .map(|i| i.data.len())
                            .sum::<usize>()
                            > 32 * 1024 * 1024
                    {
                        self.cache.image_cache.clear();
                    }
                }
                self.cache.with_pixels(
                    &mut self.fonts,
                    physical.cache_key,
                    color,
                    |dx, dy, color| blend(pixmap, physical.x + dx, physical.y + dy, color, clip),
                );
            }
        }
        if let Some((line, end, len)) = last_line {
            clipped |= end < len
                || buffer
                    .lines
                    .iter()
                    .skip(line + 1)
                    .any(|line| !line.text().trim().is_empty())
        } else {
            clipped |= !node.text.trim().is_empty()
        }
        if clipped {
            self.report.clipped_text.push(node.id.clone());
        }
        Ok(())
    }
}
fn raster_pixmap(raster: &RasterPixels) -> Result<Pixmap, String> {
    if raster.width == 0
        || raster.height == 0
        || raster.width > 4096
        || raster.height > 4096
        || u64::from(raster.width) * u64::from(raster.height) * 4 != raster.rgba.len() as u64
    {
        return Err("Некорректные пиксели изображения".into());
    }
    let mut rgba = raster.rgba.clone();
    for p in rgba.as_chunks_mut::<4>().0 {
        for i in 0..3 {
            p[i] = ((u16::from(p[i]) * u16::from(p[3]) + 127) / 255) as u8;
        }
    }
    Pixmap::from_vec(
        rgba,
        IntSize::from_wh(raster.width, raster.height).ok_or("Размер изображения")?,
    )
    .ok_or_else(|| "Не удалось подготовить изображение".into())
}
pub fn render(
    scene: &ExportScene,
    images: &HashMap<String, RasterPixels>,
    cancel: &AtomicBool,
) -> Result<Rendered, String> {
    validate(scene)?;
    cancelled(cancel)?;
    if images.len() > 64 || images.values().map(|r| r.rgba.len()).sum::<usize>() > MAX_RASTER_BYTES
    {
        return Err("Изображения PNG превышают бюджет 64 МиБ / 64 файла".into());
    }
    let [width, height] = dimensions(scene)?;
    let mut pixmap = Pixmap::new(width, height).ok_or("Не удалось выделить память PNG")?;
    if !scene.transparent {
        pixmap.fill(Color::from_rgba8(28, 28, 28, 255));
    }
    let mut text = TextRenderer {
        fonts: FontSystem::new(),
        cache: SwashCache::new(),
        families: BTreeSet::new(),
        report: RenderReport {
            width,
            height,
            nodes: scene.nodes.len(),
            edges: scene.edges.len(),
            ink_strokes: scene.nodes.iter().filter(|n| n.ink.is_some()).count(),
            ..Default::default()
        },
    };
    for edge in &scene.edges {
        cancelled(cancel)?;
        let mut builder = PathBuilder::new();
        for (i, s) in edge.segments.iter().enumerate() {
            if i == 0 {
                builder.move_to(s[0], s[1]);
            }
            builder.line_to(s[2], s[3]);
        }
        let stroke = Stroke {
            width: edge.stroke_width,
            dash: if edge.dashed {
                StrokeDash::new(vec![8., 6.], 0.)
            } else {
                None
            },
            ..Default::default()
        };
        if let Some(path) = builder.finish() {
            pixmap.stroke_path(
                &path,
                &paint(&edge.color, 1.),
                &stroke,
                transform(scene, 0., 0.),
                None,
            );
        }
        let mut builder = PathBuilder::new();
        for s in &edge.heads {
            builder.move_to(s[0], s[1]);
            builder.line_to(s[2], s[3]);
        }
        if let Some(path) = builder.finish() {
            pixmap.stroke_path(
                &path,
                &paint(&edge.color, 1.),
                &Stroke {
                    width: edge.stroke_width,
                    ..Default::default()
                },
                transform(scene, 0., 0.),
                None,
            );
        }
        if !edge.label.is_empty() {
            let r = edge.label_rect;
            let label = Node {
                id: format!("edge:{}", edge.id),
                x: r.x,
                y: r.y,
                width: r.width,
                height: r.height,
                text: edge.label.clone(),
                kind: "text".into(),
                fill: Some("#211f28".into()),
                border: Some("#54485f".into()),
                style: Some(crate::canvas_render_scene::NodeStyle {
                    shape: 0,
                    radius: 4.,
                    font_size: 14.,
                    align: "center".into(),
                    text_color: "#ded6e8".into(),
                    stroke_width: 1.,
                    bounds: Rect {
                        x: 8.,
                        y: 4.,
                        width: (r.width - 16.).max(1.),
                        height: (r.height - 8.).max(1.),
                    },
                    center_text: false,
                }),
                image: None,
                ink: None,
            };
            draw_shape(&mut pixmap, &label, scene);
            text.draw(&mut pixmap, &label, scene, cancel, false)?;
        }
    }
    for node in &scene.nodes {
        cancelled(cancel)?;
        draw_shape(&mut pixmap, node, scene);
        if let Some(image) = &node.image {
            let r = image.bounds;
            let x = (node.x + r.x - scene.bounds.x) * scene.scale as f32;
            let y = (node.y + r.y - scene.bounds.y) * scene.scale as f32;
            let w = r.width * scene.scale as f32;
            let h = r.height * scene.scale as f32;
            if let Some(rect) = DrawRect::from_xywh(x, y, w, h) {
                pixmap.fill_rect(rect, &paint("#202024", 1.), Transform::identity(), None);
            }
            if let Some(raster) = images.get(&image.path) {
                let image_pixmap = raster_pixmap(raster)?;
                pixmap.draw_pixmap(
                    0,
                    0,
                    image_pixmap.as_ref(),
                    &PixmapPaint {
                        quality: tiny_skia::FilterQuality::Bicubic,
                        ..Default::default()
                    },
                    Transform::from_row(
                        w / raster.width as f32,
                        0.,
                        0.,
                        h / raster.height as f32,
                        x,
                        y,
                    ),
                    None,
                );
            } else {
                text.report.missing_rasters.push(node.id.clone());
                let px = (r.width / 4.).min(8.);
                let py = (r.height / 4.).min(8.);
                let placeholder = Node {
                    id: node.id.clone(),
                    x: node.x + r.x,
                    y: node.y + r.y,
                    width: r.width,
                    height: r.height,
                    text: "Изображение недоступно".into(),
                    kind: "text".into(),
                    fill: None,
                    border: None,
                    image: None,
                    ink: None,
                    style: Some(crate::canvas_render_scene::NodeStyle {
                        shape: 6,
                        radius: 0.,
                        font_size: 12.,
                        align: "center".into(),
                        text_color: "#b5a8c2".into(),
                        stroke_width: 1.,
                        bounds: Rect {
                            x: px,
                            y: py,
                            width: r.width - px * 2.,
                            height: r.height - py * 2.,
                        },
                        center_text: true,
                    }),
                };
                text.draw(&mut pixmap, &placeholder, scene, cancel, true)?;
            }
        }
        text.draw(&mut pixmap, node, scene, cancel, true)?;
    }
    cancelled(cancel)?;
    text.report.font_families = text.families.into_iter().collect();
    let preview_scale = (1000. / width as f32).min(750. / height as f32).min(1.);
    let mut preview = Pixmap::new(
        (width as f32 * preview_scale).ceil() as u32,
        (height as f32 * preview_scale).ceil() as u32,
    )
    .ok_or("Не удалось подготовить preview")?;
    preview.draw_pixmap(
        0,
        0,
        pixmap.as_ref(),
        &PixmapPaint {
            quality: tiny_skia::FilterQuality::Bicubic,
            ..Default::default()
        },
        Transform::from_scale(preview_scale, preview_scale),
        None,
    );
    let png = pixmap
        .encode_png()
        .map_err(|_| "Не удалось кодировать PNG")?;
    let preview = preview
        .encode_png()
        .map_err(|_| "Не удалось кодировать preview")?;
    cancelled(cancel)?;
    Ok(Rendered {
        png,
        preview,
        report: text.report,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn node(id: &str, x: f32, y: f32, w: f32, h: f32, fill: &str, text: &str) -> Node {
        serde_json::from_value(serde_json::json!({"id":id,"kind":"text","x":x,"y":y,"width":w,"height":h,"fill":fill,"border":"#00000000","text":text})).unwrap()
    }
    fn scene(nodes: Vec<Node>) -> ExportScene {
        ExportScene {
            version: 1,
            bounds: Rect {
                x: 0.,
                y: 0.,
                width: 400.,
                height: 200.,
            },
            scale: 1,
            transparent: false,
            nodes,
            edges: vec![],
        }
    }
    fn pixels(png: &[u8]) -> Pixmap {
        Pixmap::decode_png(png).unwrap()
    }
    fn pixel(png: &[u8], x: u32, y: u32) -> [u8; 4] {
        let p = pixels(png).pixel(x, y).unwrap().demultiply();
        [p.red(), p.green(), p.blue(), p.alpha()]
    }
    fn render_fixture(scene: &ExportScene) -> Rendered {
        render(scene, &HashMap::new(), &AtomicBool::new(false)).unwrap()
    }
    #[test]
    fn refuses_unknown_geometry_unbounded_output_text_and_cancel_before_work() {
        let original = scene(vec![]);
        assert!(validate(&original).is_ok());
        let mut bad = original.clone();
        bad.scale = 3;
        assert!(validate(&bad).is_err());
        bad.scale = 2;
        bad.bounds.width = 3000.;
        assert!(dimensions(&bad).is_err());
        bad.bounds.width = f32::NAN;
        assert!(dimensions(&bad).is_err());
        let mut bad = scene(vec![node("x", 0., 0., 100., 100., "url(script)", "")]);
        assert!(validate(&bad).is_err());
        bad.nodes[0].fill = Some("#ffffff".into());
        bad.nodes[0].text = "x".repeat(65537);
        assert!(validate(&bad).is_err());
        assert!(
            render(&original, &HashMap::new(), &AtomicBool::new(true))
                .err()
                .unwrap()
                .contains("отменён")
        );
    }
    #[test]
    fn source_order_alpha_clipping_and_transparent_background_are_real_png_pixels() {
        let lower = node("a", -50., -50., 250., 300., "#ff0000", "");
        let mut group = node("b", 50., 0., 200., 200., "#0000ff33", "");
        group.kind = "group".into();
        let output = render_fixture(&scene(vec![lower, group]));
        assert_eq!([output.report.width, output.report.height], [400, 200]);
        assert_eq!(pixel(&output.png, 100, 100), [204, 0, 51, 255]);
        assert_eq!(pixel(&output.png, 300, 100), [28, 28, 28, 255]);
        assert_eq!(
            [
                pixels(&output.preview).width(),
                pixels(&output.preview).height()
            ],
            [400, 200]
        );
        let mut transparent = scene(vec![]);
        transparent.transparent = true;
        assert_eq!(
            pixel(&render_fixture(&transparent).png, 10, 10),
            [0, 0, 0, 0]
        );
    }
    #[test]
    fn marker_union_does_not_darken_itself_and_pen_pressure_changes_coverage() {
        let mut stroke = node("ink", 0., 0., 220., 100., "#00000000", "");
        stroke.ink = Some(crate::canvas_render_scene::PreparedInk {
            segments: vec![[20., 40., 180., 40., 8., 8.]],
            color: "#ffff00".into(),
            opacity: 0.24,
        });
        let one = render_fixture(&scene(vec![stroke.clone()]));
        stroke
            .ink
            .as_mut()
            .unwrap()
            .segments
            .extend([[180., 40., 20., 40., 8., 8.], [20., 40., 180., 40., 8., 8.]]);
        let repeated = render_fixture(&scene(vec![stroke.clone()]));
        assert_eq!(pixel(&one.png, 100, 40), pixel(&repeated.png, 100, 40));
        assert_eq!(pixel(&one.png, 20, 40), pixel(&repeated.png, 20, 40));
        let twice = render_fixture(&scene(vec![
            stroke.clone(),
            Node {
                id: "other".into(),
                ..stroke.clone()
            },
        ]));
        assert!(pixel(&twice.png, 100, 40)[0] > pixel(&one.png, 100, 40)[0]);
        stroke.ink.as_mut().unwrap().segments = vec![[20., 40., 180., 40., 8., 2.]];
        stroke.ink.as_mut().unwrap().opacity = 1.;
        assert_ne!(
            pixel(&render_fixture(&scene(vec![stroke.clone()])).png, 30, 45),
            [28, 28, 28, 255]
        );
        assert_eq!(
            pixel(&render_fixture(&scene(vec![stroke])).png, 170, 45),
            [28, 28, 28, 255]
        );
    }
    #[test]
    fn raster_alpha_is_premultiplied_once_and_missing_raster_is_reported() {
        let mut n = node("photo", 20., 20., 100., 100., "#292531", "");
        n.kind = "file".into();
        n.image = Some(crate::canvas_render_scene::RasterRef {
            path: "Attachments/Photo.png".into(),
            bounds: Rect {
                x: 0.,
                y: 0.,
                width: 100.,
                height: 100.,
            },
        });
        let images = HashMap::from([(
            "Attachments/Photo.png".into(),
            RasterPixels {
                width: 1,
                height: 1,
                rgba: vec![0, 255, 0, 128],
            },
        )]);
        let output = render(&scene(vec![n.clone()]), &images, &AtomicBool::new(false)).unwrap();
        assert_eq!(pixel(&output.png, 60, 60), [16, 144, 18, 255]);
        assert!(output.report.missing_rasters.is_empty());
        assert_eq!(
            render_fixture(&scene(vec![n])).report.missing_rasters,
            ["photo"]
        );
    }
    #[test]
    fn shared_unicode_layout_rasterizes_text_and_reports_real_clipping_and_font_use() {
        let n = node(
            "text",
            10.,
            10.,
            200.,
            90.,
            "#30283c",
            &"Заметки → Доска. ".repeat(100),
        );
        let output = render_fixture(&scene(vec![n]));
        assert!(output.report.glyphs > 10);
        assert!(!output.report.font_families.is_empty());
        assert_eq!(output.report.clipped_text, ["text"]);
        let image = pixels(&output.png);
        let painted = image
            .pixels()
            .iter()
            .filter(|p| p.red() > 150 && p.green() > 150 && p.blue() > 150)
            .count();
        assert!(painted > 10);
    }
    #[test]
    fn export_fixture_contains_shapes_text_ink_images_and_saves_only_opt_in_qa_artifacts() {
        let mut s = scene(vec![]);
        s.bounds = Rect {
            x: 0.,
            y: 0.,
            width: 1280.,
            height: 900.,
        };
        s.nodes.push(node(
            "title",
            30.,
            20.,
            1100.,
            100.,
            "#00000000",
            "Проект · следующий шаг\nPNG без захвата окна",
        ));
        for (i, (title, color, shape)) in [
            ("1 · Материалы\nЗаметки и источники", "#40364c", 0),
            ("2 · Решение\nСвязанные идеи", "#294339", 3),
            ("3 · Действие\nОдна задача", "#403c28", 4),
        ]
        .into_iter()
        .enumerate()
        {
            let mut n = node(
                &format!("card{i}"),
                40. + i as f32 * 410.,
                180.,
                360.,
                220.,
                color,
                title,
            );
            n.border = Some("#a99abd".into());
            n.style = Some(crate::canvas_render_scene::NodeStyle {
                shape,
                radius: 8.,
                font_size: 24.,
                align: "center".into(),
                text_color: "#eee5fa".into(),
                stroke_width: 2.,
                bounds: Rect {
                    x: shape as f32 * 16. + 20.,
                    y: shape as f32 * 8. + 20.,
                    width: 320. - shape as f32 * 32.,
                    height: 180. - shape as f32 * 16.,
                },
                center_text: true,
            });
            s.nodes.push(n);
        }
        let mut note = node(
            "note",
            40.,
            470.,
            380.,
            300.,
            "#30283d",
            "Заметка проекта\n\n• План и ссылки\n• Не копия базы задач\n• Исходники не меняются",
        );
        note.border = Some("#8d78ad".into());
        s.nodes.push(note);
        let mut image = node(
            "photo",
            470.,
            470.,
            340.,
            270.,
            "#292531",
            "Фото · локальный источник",
        );
        image.kind = "file".into();
        image.image = Some(crate::canvas_render_scene::RasterRef {
            path: "photo.png".into(),
            bounds: Rect {
                x: 12.,
                y: 12.,
                width: 316.,
                height: 190.,
            },
        });
        image.style = Some(crate::canvas_render_scene::NodeStyle {
            shape: 0,
            radius: 8.,
            font_size: 16.,
            align: "left".into(),
            text_color: "#eee5fa".into(),
            stroke_width: 1.,
            bounds: Rect {
                x: 12.,
                y: 212.,
                width: 316.,
                height: 42.,
            },
            center_text: false,
        });
        s.nodes.push(image);
        s.nodes.push(node("limitations",850.,470.,390.,300.,"#263b40","Переносимость PNG\n\nРисунок остаётся картинкой.\nСсылки, задачи, слои и заметки\nне станут интерактивными.\n\nПодробности — в report.json."));
        let mut ink = node("marker", 0., 0., 1280., 900., "#00000000", "");
        ink.ink = Some(crate::canvas_render_scene::PreparedInk {
            segments: vec![
                [60., 140., 1180., 140., 9., 9.],
                [1180., 140., 60., 140., 9., 9.],
            ],
            color: "#d6b669".into(),
            opacity: 0.24,
        });
        s.nodes.push(ink);
        let mut pen = node("pen", 0., 0., 1280., 900., "#00000000", "");
        pen.ink = Some(crate::canvas_render_scene::PreparedInk {
            segments: vec![
                [40., 820., 200., 800., 5., 2.],
                [200., 800., 390., 830., 2., 5.],
            ],
            color: "#a997cc".into(),
            opacity: 1.,
        });
        s.nodes.push(pen);
        let mut rgba = Vec::new();
        for y in 0..48 {
            for x in 0..64 {
                rgba.extend([
                    ((x * 4) as u8),
                    ((y * 5) as u8),
                    if (x / 8 + y / 8) % 2 == 0 { 180 } else { 90 },
                    255,
                ]);
            }
        }
        let output = render(
            &s,
            &HashMap::from([(
                "photo.png".into(),
                RasterPixels {
                    width: 64,
                    height: 48,
                    rgba,
                },
            )]),
            &AtomicBool::new(false),
        )
        .unwrap();
        assert_eq!(output.report.ink_strokes, 2);
        assert!(output.report.glyphs > 100);
        assert!(output.report.missing_rasters.is_empty());
        assert_eq!(pixels(&output.png).width(), 1280);
        if std::env::var("BRANDON_EXPORT_FIXTURE").as_deref() == Ok("1") {
            let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .ancestors()
                .nth(3)
                .unwrap()
                .join("target/qa/canvas-export");
            std::fs::create_dir_all(&root).unwrap();
            std::fs::write(root.join("render.png"), &output.png).unwrap();
            std::fs::write(root.join("preview.png"), &output.preview).unwrap();
            std::fs::write(
                root.join("render-report.json"),
                serde_json::to_vec_pretty(&output.report).unwrap(),
            )
            .unwrap();
        }
    }
}

//! Prepared render data shared by live GPU composition and bounded CPU export.
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

#[derive(Clone, Debug, Deserialize, Serialize)]
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
    pub(crate) image: Option<RasterRef>,
    #[serde(default)]
    pub(crate) ink: Option<PreparedInk>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct RasterRef {
    pub(crate) path: String,
    pub(crate) bounds: Rect,
}
impl RasterRef {
    pub(crate) fn valid_for(&self, node: &Node) -> bool {
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

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct NodeStyle {
    pub(crate) shape: u32,
    pub(crate) radius: f32,
    pub(crate) font_size: f32,
    pub(crate) align: String,
    pub(crate) text_color: String,
    pub(crate) stroke_width: f32,
    pub(crate) bounds: Rect,
    #[serde(default)]
    pub(crate) center_text: bool,
}
impl NodeStyle {
    pub(crate) fn valid_for(&self, node: &Node) -> bool {
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
    pub(crate) fn text_rect(&self) -> Rect {
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

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Edge {
    pub(crate) id: String,
    pub(crate) x: f32,
    pub(crate) y: f32,
    pub(crate) width: f32,
    pub(crate) height: f32,
    pub(crate) segments: Vec<[f32; 4]>,
    pub(crate) heads: Vec<[f32; 4]>,
    pub(crate) color: String,
    pub(crate) stroke_width: f32,
    pub(crate) dashed: bool,
    pub(crate) label: String,
    pub(crate) label_rect: Rect,
}
impl Edge {
    pub(crate) fn valid(&self) -> bool {
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

#[derive(Default, Deserialize, Serialize)]
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

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}
impl Rect {
    pub(crate) fn intersection(self, other: Self) -> Option<Self> {
        let x = self.x.max(other.x);
        let y = self.y.max(other.y);
        let right = (self.x + self.width).min(other.x + other.width);
        let bottom = (self.y + self.height).min(other.y + other.height);
        (right > x && bottom > y).then_some(Self {
            x,
            y,
            width: right - x,
            height: bottom - y,
        })
    }
    pub(crate) fn intersects(self, other: Self) -> bool {
        self.x <= other.x + other.width
            && self.x + self.width >= other.x
            && self.y <= other.y + other.height
            && self.y + self.height >= other.y
    }
}

pub(crate) fn validate_scene(
    current: &HashMap<String, Node>,
    delta: &SceneDelta,
) -> Result<(), String> {
    let mut counts: HashMap<&str, usize> = if delta.reset {
        HashMap::new()
    } else {
        current
            .iter()
            .map(|(id, node)| {
                (
                    id.as_str(),
                    node.ink.as_ref().map_or(0, |s| s.segments.len()),
                )
            })
            .collect()
    };
    for id in &delta.removed {
        counts.remove(id.as_str());
    }
    let mut ids = HashSet::new();
    for node in &delta.nodes {
        if !ids.insert(node.id.as_str()) {
            return Err("Duplicate canvas node delta".into());
        }
        if node.ink.as_ref().is_some_and(|s| !s.valid_for(node)) {
            return Err("Invalid prepared canvas ink".into());
        }
        if ![node.x, node.y, node.width, node.height]
            .iter()
            .all(|v| v.is_finite() && v.abs() <= 10000000.)
            || node.width <= 0.
            || node.height <= 0.
        {
            return Err("Invalid canvas object geometry".into());
        }
        if node.style.as_ref().is_some_and(|s| !s.valid_for(node)) {
            return Err("Invalid canvas object style".into());
        }
        if node.image.as_ref().is_some_and(|s| !s.valid_for(node)) {
            return Err("Invalid raster reference".into());
        }
        counts.insert(&node.id, node.ink.as_ref().map_or(0, |s| s.segments.len()));
    }
    if counts.len() > 100000
        || counts.values().filter(|&&n| n > 0).count() > 4096
        || counts.values().sum::<usize>() > 100000
    {
        return Err("Canvas ink/object budget exceeded".into());
    }
    Ok(())
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub(crate) struct PreparedInk {
    pub segments: Vec<[f32; 6]>,
    pub color: String,
    pub opacity: f32,
}
impl PreparedInk {
    pub(crate) fn valid_for(&self, node: &Node) -> bool {
        let e = node.width.max(node.height) * 0.000001 + 0.05;
        node.kind == "text"
            && node.image.is_none()
            && !self.segments.is_empty()
            && self.segments.len() <= 2048
            && [1., 0.24].contains(&self.opacity)
            && self
                .color
                .strip_prefix('#')
                .is_some_and(|s| s.len() == 6 && s.bytes().all(|b| b.is_ascii_hexdigit()))
            && self.segments.iter().all(|s| {
                s.iter().all(|n| n.is_finite())
                    && s[4] > 0.
                    && s[5] > 0.
                    && [(s[0], s[1], s[4]), (s[2], s[3], s[5])]
                        .iter()
                        .all(|&(x, y, r)| {
                            x - r >= -e
                                && y - r >= -e
                                && x + r <= node.width + e
                                && y + r <= node.height + e
                        })
            })
    }
}

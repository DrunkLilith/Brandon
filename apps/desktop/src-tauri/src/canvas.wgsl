struct View { size: vec2<f32>, _padding: vec2<f32> }
@group(0) @binding(0) var<uniform> view: View;

struct Vertex {
  @builtin(position) position: vec4<f32>,
  @location(0) local: vec2<f32>,
  @location(1) @interpolate(flat) dimensions: vec2<f32>,
  @location(2) @interpolate(flat) fill: vec4<f32>,
  @location(3) @interpolate(flat) border: vec4<f32>,
  @location(4) @interpolate(flat) style: vec4<f32>,
}

@vertex fn vertex(
  @builtin(vertex_index) vertex_index: u32,
  @location(0) rect: vec4<f32>,
  @location(1) fill: vec4<f32>,
  @location(2) border: vec4<f32>,
  @location(3) style: vec4<f32>,
) -> Vertex {
  var corners = array<vec2<f32>, 6>(vec2(0., 0.), vec2(1., 0.), vec2(0., 1.), vec2(0., 1.), vec2(1., 0.), vec2(1., 1.));
  let corner = corners[vertex_index];
  var size = rect.zw;
  var position = rect.xy + corner * size;
  if (style.y > 0.5 && style.y < 1.5) {
    let delta = rect.zw - rect.xy;
    let length = max(length(delta), 0.001);
    let normal = vec2(-delta.y, delta.x) / length;
    size = vec2(length, style.x);
    position = rect.xy + delta * corner.x + normal * (corner.y - 0.5) * style.x;
  }
  var output: Vertex;
  let depth = select(select(0.98, 0.999, style.y > 1.5), style.w, style.y < 0.5);
  output.position = vec4(position.x / view.size.x * 2. - 1., 1. - position.y / view.size.y * 2., depth, 1.);
  output.local = corner * size;
  output.dimensions = size;
  output.fill = fill;
  output.border = border;
  output.style = style;
  return output;
}

@fragment fn fragment(input: Vertex) -> @location(0) vec4<f32> {
  if (input.style.y > 1.5) {
    let grid = max(input.style.z, 8.);
    let point = abs(fract((input.local + vec2(input.style.x, input.style.w)) / grid) * grid - grid * 0.5);
    let dot = 1. - smoothstep(0.5, 1.3, length(point));
    return vec4(mix(input.fill.rgb, input.border.rgb, dot), 1.);
  }
  if (input.style.y > 0.5) { return input.fill; }
  let radius = min(input.style.x, min(input.dimensions.x, input.dimensions.y) * 0.5);
  let q = abs(input.local - input.dimensions * 0.5) - input.dimensions * 0.5 + radius;
  let distance = length(max(q, vec2(0.))) + min(max(q.x, q.y), 0.) - radius;
  let aa = max(fwidth(distance), 0.65);
  let alpha = 1. - smoothstep(-aa, aa, distance);
  if (alpha < 0.01) { discard; }
  let border_mix = smoothstep(-input.style.z - aa, -input.style.z + aa, distance);
  let color = mix(input.fill, input.border, border_mix);
  return vec4(color.rgb, color.a * alpha);
}

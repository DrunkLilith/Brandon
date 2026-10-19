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
  var depth = style.w;
  if (style.y > 0.5 && style.y < 1.5) { depth = 0.98; }
  if (style.y > 1.5 && style.y < 2.5) { depth = 0.999; }
  output.position = vec4(position.x / view.size.x * 2. - 1., 1. - position.y / view.size.y * 2., depth, 1.);
  output.local = corner * size;
  output.dimensions = size;
  output.fill = fill;
  output.border = border;
  output.style = style;
  return output;
}

fn segment_distance(p:vec2<f32>,a:vec2<f32>,b:vec2<f32>)->f32{
  let edge=b-a;let q=p-a;return length(q-edge*clamp(dot(q,edge)/max(dot(edge,edge),0.0001),0.,1.));
}
fn cross2(a:vec2<f32>,b:vec2<f32>)->f32{return a.x*b.y-a.y*b.x;}
@fragment fn fragment(input: Vertex) -> @location(0) vec4<f32> {
  let shape=input.style.y;
  if (shape > 1.5 && shape < 2.5) {
    let grid = max(input.style.z, 8.);
    let point = abs(fract((input.local + vec2(input.style.x, input.style.w)) / grid) * grid - grid * 0.5);
    let dot = 1. - smoothstep(0.5, 1.3, length(point));
    return vec4(mix(input.fill.rgb, input.border.rgb, dot), 1.);
  }
  if (shape > 0.5 && shape < 1.5) {
    if(input.style.z>0. && ((input.local.x+input.style.w)%input.style.z)>input.style.z*(8./14.)){discard;}
    return input.fill;
  }
  if (shape > 5.5 && shape < 6.5 && input.style.z <= 0.) { discard; }
  let radius = min(input.style.x, min(input.dimensions.x, input.dimensions.y) * 0.5);
  let q = abs(input.local - input.dimensions * 0.5) - input.dimensions * 0.5 + radius;
  var distance = length(max(q, vec2(0.))) + min(max(q.x, q.y), 0.) - radius;
  let half_size=max(input.dimensions*0.5,vec2(0.0001));
  if(shape>2.5 && shape<3.5){
    let p=(input.local-half_size)/half_size;
    distance=(dot(p,p)-1.)/max(length(2.*p/half_size),0.0001);
    if(dot(p,p)<0.25){distance=-min(half_size.x,half_size.y)*0.5;}
  }else if(shape>3.5 && shape<4.5){
    let p=abs(input.local-half_size)/half_size;
    distance=(p.x+p.y-1.)/length(1./half_size);
  }else if(shape>4.5 && shape<5.5){
    let a=vec2(half_size.x,0.);let b=input.dimensions;let c=vec2(0.,input.dimensions.y);let p=input.local;
    let inside=cross2(b-a,p-a)>=0. && cross2(c-b,p-b)>=0. && cross2(a-c,p-c)>=0.;
    let d=min(segment_distance(p,a,b),min(segment_distance(p,b,c),segment_distance(p,c,a)));
    distance=select(d,-d,inside);
  }else if(shape>6.5 && shape<7.5){
    distance=abs(input.local.y-half_size.y)-input.style.z*0.5;
  }
  let aa = max(fwidth(distance), 0.65);
  let alpha = 1. - smoothstep(-aa, aa, distance);
  if (alpha < 0.01) { discard; }
  if(shape>5.5 && shape<6.5 && distance < -input.style.z-aa){discard;}
  let border_mix = smoothstep(-input.style.z - aa, -input.style.z + aa, distance);
  var color = mix(input.fill, input.border, border_mix);
  if(shape>6.5 && shape<7.5){color=input.border;}
  // A fully transparent fill must not become an invisible depth occluder.
  if(color.a * alpha < 0.01){discard;}
  return vec4(color.rgb, color.a * alpha);
}

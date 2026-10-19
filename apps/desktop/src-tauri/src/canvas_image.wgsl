struct View{size:vec2<f32>,padding:vec2<f32>}
@group(0) @binding(0) var<uniform> view:View;
@group(1) @binding(0) var raster:texture_2d<f32>;
@group(1) @binding(1) var raster_sampler:sampler;
struct Vertex{@builtin(position) position:vec4<f32>,@location(0) uv:vec2<f32>,@location(1) @interpolate(flat) stage:vec4<f32>,@location(2) @interpolate(flat) radius:f32}
@vertex fn vertex(@builtin(vertex_index) index:u32,@location(0) rect:vec4<f32>,@location(1) stage:vec4<f32>,@location(3) style:vec4<f32>)->Vertex{
  var corners=array<vec2<f32>,6>(vec2(0.,0.),vec2(1.,0.),vec2(0.,1.),vec2(0.,1.),vec2(1.,0.),vec2(1.,1.));let corner=corners[index];let p=rect.xy+corner*rect.zw;
  var out:Vertex;out.position=vec4(p.x/view.size.x*2.-1.,1.-p.y/view.size.y*2.,style.w,1.);out.uv=corner;out.stage=stage;out.radius=style.x;return out;
}
@fragment fn fragment(input:Vertex)->@location(0) vec4<f32>{
  let radius=min(input.radius,min(input.stage.z,input.stage.w)*.5);let q=abs(input.position.xy-input.stage.xy-input.stage.zw*.5)-input.stage.zw*.5+radius;let distance=length(max(q,vec2(0.)))+min(max(q.x,q.y),0.)-radius;let mask=1.-smoothstep(-.65,.65,distance);if(mask<.01){discard;}
  let pixel=textureSample(raster,raster_sampler,input.uv);return vec4(pixel.rgb,pixel.a*mask);
}

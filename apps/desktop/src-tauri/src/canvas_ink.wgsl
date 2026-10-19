struct View{size:vec2<f32>,padding:vec2<f32>}
@group(0) @binding(0) var<uniform> view:View;
@group(1) @binding(0) var coverage:texture_2d<f32>;
struct MaskVertex{@builtin(position) position:vec4<f32>,@location(0) @interpolate(flat) ends:vec4<f32>,@location(1) @interpolate(flat) radii:vec2<f32>}
fn corner(i:u32)->vec2<f32>{var p=array<vec2<f32>,6>(vec2(0.,0.),vec2(1.,0.),vec2(0.,1.),vec2(0.,1.),vec2(1.,0.),vec2(1.,1.));return p[i];}
@vertex fn mask_vertex(@builtin(vertex_index) index:u32,@location(0) ends:vec4<f32>,@location(1) params:vec4<f32>)->MaskVertex{
  let lo=min(ends.xy-params.x,ends.zw-params.y)-1.;let hi=max(ends.xy+params.x,ends.zw+params.y)+1.;let p=mix(lo,hi,corner(index));
  var out:MaskVertex;out.position=vec4(p.x/view.size.x*2.-1.,1.-p.y/view.size.y*2.,0.,1.);out.ends=ends;out.radii=params.xy;return out;
}
fn segment_distance(p:vec2<f32>,a:vec2<f32>,b:vec2<f32>)->f32{let e=b-a;return length(p-a-e*clamp(dot(p-a,e)/max(dot(e,e),.000001),0.,1.));}
fn cross2(a:vec2<f32>,b:vec2<f32>)->f32{return a.x*b.y-a.y*b.x;}
@fragment fn mask_fragment(input:MaskVertex)->@location(0) f32{
  let p=input.position.xy;let a=input.ends.xy;let b=input.ends.zw;let r=input.radii;
  var d=min(length(p-a)-r.x,length(p-b)-r.y);let e=b-a;let len=length(e);
  if(len>.00001){
    let n=vec2(-e.y,e.x)/len;var q=array<vec2<f32>,4>(a+n*r.x,a-n*r.x,b-n*r.y,b+n*r.y);var edge_distance=1.e30;var inside=true;
    for(var i=0u;i<4u;i++){let next=(i+1u)%4u;edge_distance=min(edge_distance,segment_distance(p,q[i],q[next]));inside=inside&&cross2(q[next]-q[i],p-q[i])>=0.;}
    d=min(d,select(edge_distance,-edge_distance,inside));
  }
  let aa=max(fwidth(d),.65);return 1.-smoothstep(-aa,aa,d);
}
struct CompositeVertex{@builtin(position) position:vec4<f32>,@location(0) @interpolate(flat) color:vec4<f32>}
@vertex fn composite_vertex(@builtin(vertex_index) index:u32,@location(0) rect:vec4<f32>,@location(1) color:vec4<f32>,@location(3) style:vec4<f32>)->CompositeVertex{
  let p=rect.xy+rect.zw*corner(index);var out:CompositeVertex;out.position=vec4(p.x/view.size.x*2.-1.,1.-p.y/view.size.y*2.,style.w,1.);out.color=color;return out;
}
@fragment fn clear_fragment()->@location(0) f32{return 0.;}
@fragment fn composite_fragment(input:CompositeVertex)->@location(0) vec4<f32>{let alpha=textureLoad(coverage,vec2<i32>(input.position.xy),0).r*input.color.a;if(alpha<.01){discard;}return vec4(input.color.rgb,alpha);}

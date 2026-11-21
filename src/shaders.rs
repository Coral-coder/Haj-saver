// WGSL sources. Abyssal Garden's shaders (extracted to shaders/*.wgsl by tools/extract-shaders.mjs) are glued back
// together from their shared preludes here, with the quality-dependent {{HOLES}} filled in.
const NOISE: &str = include_str!("../shaders/noise.wgsl");
const FS: &str = include_str!("../shaders/fs.wgsl");
const G: &str = include_str!("../shaders/g.wgsl");
const B: &str = include_str!("../shaders/b.wgsl");

pub struct Defs { pub low: bool, pub ms: bool, pub shadow: u32, pub water: u32, pub rayn: u32 }
impl Defs {
    fn fill(&self, parts: &[&str]) -> String {
        parts.concat()
            .replace("{{LOW}}", if self.low { "true" } else { "false" })
            .replace("{{MS}}", if self.ms { "_multisampled" } else { "" })
            .replace("{{SHADOW}}", &self.shadow.to_string())
            .replace("{{WATER}}", &self.water.to_string())
            .replace("{{RAYN}}", &self.rayn.to_string())
    }
    pub fn dens(&self) -> String { self.fill(&[NOISE, include_str!("../shaders/dens.wgsl")]) }
    pub fn detail(&self) -> String { self.fill(&[NOISE, FS, include_str!("../shaders/detail.wgsl")]) }
    pub fn blit(&self) -> String { self.fill(&[FS, include_str!("../shaders/blit.wgsl")]) }
    pub fn water(&self) -> String { self.fill(&[FS, include_str!("../shaders/water.wgsl")]) }
    pub fn solid(&self) -> String { self.fill(&[G, B, include_str!("../shaders/solid.wgsl")]) }
    pub fn leaf(&self) -> String { self.fill(&[G, B, include_str!("../shaders/leaf.wgsl")]) }
    pub fn anim(&self) -> String { self.fill(&[G, B, include_str!("../shaders/anim.wgsl")]) }
    pub fn surf(&self) -> String { self.fill(&[G, B, include_str!("../shaders/surf.wgsl")]) }
    pub fn spr(&self) -> String { self.fill(&[G, B, include_str!("../shaders/spr.wgsl")]) }
    pub fn shark(&self) -> String { self.fill(&[G, B, include_str!("../shaders/shark.wgsl")]) }
    pub fn rays(&self) -> String { self.fill(&[G, FS, include_str!("../shaders/rays.wgsl")]) }
    pub fn rblur(&self) -> String { self.fill(&[FS, include_str!("../shaders/rblur.wgsl")]) }
    pub fn post(&self) -> String { self.fill(&[G, FS, include_str!("../shaders/post.wgsl")]) }
}

/// final upscale from the render resolution to the window, plus a fade to black for cuts
pub const UPSCALE: &str = r#"
@group(0) @binding(0) var t: texture_2d<f32>;
@group(0) @binding(1) var s: sampler;
@group(0) @binding(2) var<uniform> k: vec4f; // 1 / output size, fade
@vertex fn vs(@builtin(vertex_index) i: u32) -> @builtin(position) vec4f { return vec4f(f32(i32(i & 1u) * 4 - 1), f32(i32(i >> 1u) * 4 - 1), 0., 1.); }
@fragment fn fs(@builtin(position) fc: vec4f) -> @location(0) vec4f {
  let c = textureSampleLevel(t, s, fc.xy * k.xy, 0.).rgb;
  return vec4f(c * k.z, 1.);
}
"#;

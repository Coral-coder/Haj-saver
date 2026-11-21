// The renderer: abyssal-garden's source/src/main.js ported to wgpu, plus the shark, its moving shadow,
// a final upscale and dynamic resolution.
#![allow(clippy::too_many_arguments)]
use crate::math::*;
use crate::shaders::{Defs, UPSCALE};
use crate::shark::SharkMesh;
use crate::creatures::{self, PlushPose};
use crate::world::*;
use wgpu::util::DeviceExt;

pub const HDR: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;
pub const DEPTH: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
const UBN: usize = 84;
const MAX_SHARKS: usize = 8;
const MAX_PLUSH: usize = 8;
const SHARK_SLOT: usize = 256; // dynamic uniform offsets must be 256-byte aligned // floats in the scene uniform block (G in g.wgsl)

/// preset viewpoints (position, look-at); the world generator dresses the scene around them
pub const POSES: [[f64; 6]; 6] = [
    [10., 1.3, 7., -9., -31., -10.], [50., 1.3, 34., 58., -23., 8.], [-26., 2.2, -100., 0., -9., -108.],
    [-55., 1.5, -18., -60., -30., 8.], [10., 1.2, -62., 6., -22., -90.], [3., 1.6, 62., 1., -28., 100.],
];
/// 60 s camera path through the whole world (position + look-at): the generator keeps it clear, and the shark
/// uses it as his main swimming route
pub const FLY: [[f64; 6]; 25] = [
    [10., -42.2, 7., -9., -31., -10.], [7.5, -41.8, 5.5, -10., -31., -12.], [9., -40.5, 18., 17., -37., 42.], [22., -39.2, 40., 48., -36., 45.],
    [38., -39.2, 46., 57., -27., 18.], [50., -40.4, 35., 58., -24., 8.], [55., -39., 20., 60., -18., -10.], [58., -34., -12., 40., -26., -60.],
    [50., -30., -44., 10., -26., -80.], [30., -30., -70., 4., -28., -90.], [12., -38.5, -62., 6., -27., -90.], [-4., -36.5, -70., 0., -14., -100.],
    [-16., -37., -84., 0., -22., -108.], [-29., -38., -90., -44., -34., -60.], [-41., -38., -70., -56., -34., -40.], [-53., -38., -50., -60., -34., -20.],
    [-58., -38., -28., -60., -32., 0.], [-57., -38., -10., -60., -30., 16.], [-60., -38., 10., -50., -36., 40.], [-52., -37., 36., -20., -35., 44.],
    [-22., -38., 43., 4., -38., 52.], [-6., -39.2, 44.5, 4., -37., 70.], [3., -40.6, 53., 3., -35., 84.], [3., -41.5, 62., 1., -30., 100.], [4., -39., 76., 3., -22., 100.],
];
/// Catmull-Rom over the FLY table at spline parameter s: position + look-at
pub fn fly_raw(s: f64) -> [f64; 6] {
    let n = FLY.len(); let mut i = s.floor() as isize; if i > n as isize - 2 { i = n as isize - 2; } if i < 0 { i = 0; }
    let i = i as usize; let u = s - i as f64;
    let (i0, i1, i2, i3) = (i.saturating_sub(1), i, i + 1, (i + 2).min(n - 1));
    let mut o = [0.0; 6];
    for k in 0..6 {
        let (a, b, c, d) = (FLY[i0][k], FLY[i1][k], FLY[i2][k], FLY[i3][k]);
        o[k] = 0.5 * (2.0 * b + (c - a) * u + (2.0 * a - 5.0 * b + 4.0 * c - d) * u * u + (3.0 * b - a - 3.0 * c + d) * u * u * u);
    }
    o
}
/// arc-length table: lut[k] = spline parameter at fraction k/LUTN of the total path length
pub struct FlyPath { pub lut: Vec<f64>, pub len: f64 }
impl FlyPath {
    pub fn new() -> FlyPath {
        let m = 4096; let mut len = vec![0f64; m + 1]; let n = (FLY.len() - 1) as f64;
        let mut prev = fly_raw(0.0);
        for k in 1..=m { let p = fly_raw(k as f64 / m as f64 * n); len[k] = len[k - 1] + hypot3(p[0] - prev[0], p[1] - prev[1], p[2] - prev[2]); prev = p; }
        let ln = 1024; let mut lut = vec![0f64; ln + 1]; let mut j = 0;
        for (k, l) in lut.iter_mut().enumerate() {
            let d = k as f64 / ln as f64 * len[m];
            while j < m - 1 && len[j + 1] < d { j += 1; }
            *l = (j as f64 + (d - len[j]) / (len[j + 1] - len[j]).max(1e-6)) / m as f64 * n;
        }
        FlyPath { lut, len: len[m] }
    }
    /// position + look-at at fraction f (0..1) of the path length
    pub fn at(&self, f: f64) -> [f64; 6] {
        let ln = self.lut.len() - 1; let s = clamp(f, 0.0, 1.0) * ln as f64; let k = (s.floor() as usize).min(ln - 1);
        fly_raw(self.lut[k] + (self.lut[k + 1] - self.lut[k]) * (s - k as f64))
    }
}
impl Default for FlyPath { fn default() -> Self { Self::new() } }

/// what the director hands the renderer each frame
#[derive(Clone, Copy, Debug)]
pub struct Cam { pub pos: V3, pub yaw: f64, pub pitch: f64, pub tanh: f64, pub fade: f64 }
/// the shark's pose and animation state for the GPU
#[derive(Clone, Copy, Default)]
pub struct SharkPose { pub pos: V3, pub fwd: V3, pub up: V3, pub scale: f64, pub a: [f64; 4], pub b: [f64; 4] }

struct Mi { v0: i32, i0: u32, n: u32 }
struct CellBuf { start: Vec<u32>, lo: Vec<f32>, hi: Vec<f32>, buf: wgpu::Buffer, n: u32 }

pub struct Quality { pub lv: u32 }
impl Quality {
    fn shadow(&self) -> u32 { if self.lv > 0 { 2048 } else { 4096 } }
    /// MSAA only on High: on integrated graphics the multisampled scene pass is the single biggest cost
    fn aa(&self) -> u32 { if self.lv > 0 { 1 } else { 4 } }
    fn water(&self) -> u32 { if self.lv > 1 { 256 } else { 512 } }
    fn rayn(&self) -> u32 { [34, 20, 12][self.lv.min(2) as usize] }
}

struct Targets {
    w: u32, h: u32, dw: u32, dh: u32,
    ms: wgpu::TextureView, dp: wgpu::TextureView, res: wgpu::TextureView, ray: wgpu::TextureView, ray2: wgpu::TextureView, ldr: wgpu::TextureView,
    bloom: Vec<wgpu::TextureView>, bg_blur: wgpu::BindGroup, bg_rays: wgpu::BindGroup, bg_post: wgpu::BindGroup, bloom_bg: Vec<wgpu::BindGroup>, bg_up: wgpu::BindGroup,
}

pub struct Renderer {
    pub device: wgpu::Device, pub queue: wgpu::Queue, out_fmt: wgpu::TextureFormat, q: Quality,
    ubo: wgpu::Buffer, ubo_ss: wgpu::Buffer, ubo_w: wgpu::Buffer, ubo_up: wgpu::Buffer, ub: [f32; UBN], ub_ss: [f32; UBN],
    bg0: wgpu::BindGroup, bg0_ss: wgpu::BindGroup, s_clamp: wgpu::Sampler, s_rep: wgpu::Sampler, s_cmp: wgpu::Sampler,
    t_shadow: wgpu::TextureView, t_water: wgpu::TextureView, t_vol: wgpu::TextureView, t_ss: wgpu::TextureView,
    water_views: Vec<wgpu::TextureView>, water_bg: Vec<wgpu::BindGroup>, bg_water: wgpu::BindGroup,
    p_terrain: wgpu::RenderPipeline, p_solid: wgpu::RenderPipeline, p_leaf: wgpu::RenderPipeline, p_leaf5: wgpu::RenderPipeline, p_leaf_s: wgpu::RenderPipeline, p_leaf_f: wgpu::RenderPipeline,
    p_fish: [wgpu::RenderPipeline; 3], p_manta: wgpu::RenderPipeline, p_surf: wgpu::RenderPipeline, p_back: wgpu::RenderPipeline, p_bub: wgpu::RenderPipeline, p_snow: wgpu::RenderPipeline, p_gl: wgpu::RenderPipeline,
    p_water: wgpu::RenderPipeline, p_mip16: wgpu::RenderPipeline, p_down0: wgpu::RenderPipeline, p_down: wgpu::RenderPipeline, p_up: wgpu::RenderPipeline,
    p_rays: wgpu::RenderPipeline, p_rb: wgpu::RenderPipeline, p_post: wgpu::RenderPipeline, p_upscale: wgpu::RenderPipeline,
    p_plush: wgpu::RenderPipeline, bg_plush: wgpu::BindGroup, ubo_plush: wgpu::Buffer, vb_plush: wgpu::Buffer, ib_plush: wgpu::Buffer, plush_ranges: [(u32, u32); 3],
    /// 0 normal, 1 November (autumn), 2 December (Christmas)
    pub season: u32,
    /// one fish of each species (its instance data), for the debug camera
    pub fish_probe: Vec<[f32; 8]>,
    p_shark: wgpu::RenderPipeline, p_shark_sh: wgpu::RenderPipeline, bg_shark: wgpu::BindGroup, ubo_shark: wgpu::Buffer, vb_shark: wgpu::Buffer, ib_shark: wgpu::Buffer, n_shark: u32,
    vb_mesh: wgpu::Buffer, ib_mesh: wgpu::Buffer, vb_terrain: wgpu::Buffer, ib_terrain: wgpu::Buffer, ib_solids: wgpu::Buffer, n_solid: u32,
    meshes: Vec<Mi>, ranges: [(u32, u32); 12], t_start: Vec<u32>, t_lo: Vec<f32>, t_hi: Vec<f32>,
    chunks: Vec<CellBuf>, props: Vec<CellBuf>, big: CellBuf, big_f: CellBuf, small: CellBuf,
    ib_fish: [(wgpu::Buffer, u32); 3], ib_manta: wgpu::Buffer, n_manta: u32, ib_bub: wgpu::Buffer, n_bub: u32, ib_snow: wgpu::Buffer, n_snow: u32, ib_gl: wgpu::Buffer, n_gl: u32,
    tg: Option<Targets>,
    // per-frame camera basis
    f: [f64; 12], tanh: f64, row_ord: [usize; CELLS],
    /// draw distance scale (dynamic detail)
    pub det: f64,
    /// rendered pixels (dynamic resolution)
    pub pix: f64,
    dyn_t: f64, dyn_n: u32, dyn_bad: u32, dyn_ok: u32, dyn_wait: u32,
}

/// waits for a buffer mapping on native and on the web
async fn map_read(device: &wgpu::Device, b: &wgpu::Buffer) {
    let (tx, rx) = futures_channel::oneshot::channel();
    b.slice(..).map_async(wgpu::MapMode::Read, move |r| { let _ = tx.send(r); });
    let _ = device.poll(wgpu::PollType::wait_indefinitely());
    rx.await.expect("map cancelled").expect("map failed");
}

pub type Progress<'a> = &'a mut dyn FnMut(&str, f32);

impl Renderer {
    pub async fn new(device: wgpu::Device, queue: wgpu::Queue, out_fmt: wgpu::TextureFormat, lv: u32, shark: &SharkMesh, progress: Progress<'_>) -> (Renderer, Field) {
        let q = Quality { lv };
        let defs = Defs { med: lv == 1, low: lv > 1, ms: q.aa() > 1, shadow: q.shadow(), water: q.water(), rayn: q.rayn() };
        let d = &device;
        let tu = wgpu::TextureUsages::empty();
        let tex = |w: u32, h: u32, dd: u32, format: wgpu::TextureFormat, usage: wgpu::TextureUsages, mips: u32, samples: u32| d.create_texture(&wgpu::TextureDescriptor {
            label: None, size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: dd }, mip_level_count: mips, sample_count: samples,
            dimension: if dd > 1 { wgpu::TextureDimension::D3 } else { wgpu::TextureDimension::D2 }, format, usage, view_formats: &[] });
        let _ = tu;
        let ra = wgpu::TextureUsages::RENDER_ATTACHMENT; let tb = wgpu::TextureUsages::TEXTURE_BINDING; let cd = wgpu::TextureUsages::COPY_DST;
        let lin = wgpu::FilterMode::Linear;
        let s_rep = d.create_sampler(&wgpu::SamplerDescriptor { address_mode_u: wgpu::AddressMode::Repeat, address_mode_v: wgpu::AddressMode::Repeat, mag_filter: lin, min_filter: lin, mipmap_filter: wgpu::MipmapFilterMode::Linear, anisotropy_clamp: if lv > 1 { 1 } else { 8 }, ..Default::default() });
        let s_clamp = d.create_sampler(&wgpu::SamplerDescriptor { mag_filter: lin, min_filter: lin, ..Default::default() });
        let s_cmp = d.create_sampler(&wgpu::SamplerDescriptor { compare: Some(wgpu::CompareFunction::LessEqual), mag_filter: lin, min_filter: lin, ..Default::default() });
        let sh = q.shadow();
        let t_shadow_t = tex(sh, sh, 1, DEPTH, ra | tb, 1, 1);
        let t_dummy_t = tex(1, 1, 1, DEPTH, ra | tb, 1, 1);
        let t_ss_t = tex(1024, 1024, 1, DEPTH, ra | tb, 1, 1);
        let t_water_t = tex(q.water(), q.water(), 1, HDR, ra | tb, 5, 1);
        let t_detail_t = tex(1024, 1024, 1, wgpu::TextureFormat::Rgba8Unorm, ra | tb, 11, 1);
        let t_vol_t = tex((NX / 2) as u32, (NY / 2) as u32, (NZ / 2) as u32, wgpu::TextureFormat::Rg8Unorm, tb | cd, 1, 1);
        let t_map_t = tex(512, 512, 1, wgpu::TextureFormat::Rgba8Unorm, tb | cd, 1, 1);
        let view = |t: &wgpu::Texture| t.create_view(&Default::default());
        let t_fish_t = tex(256, 128, 1, wgpu::TextureFormat::Rgba8Unorm, tb | cd, 1, 1);
        queue.write_texture(wgpu::TexelCopyTextureInfo { texture: &t_fish_t, mip_level: 0, origin: Default::default(), aspect: Default::default() }, &creatures::fish_texture(),
            wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(256 * 4), rows_per_image: None }, wgpu::Extent3d { width: 256, height: 128, depth_or_array_layers: 1 });
        let t_fish = view(&t_fish_t);
        let (t_shadow, t_dummy, t_ss, t_water, t_detail, t_vol, t_map) = (view(&t_shadow_t), view(&t_dummy_t), view(&t_ss_t), view(&t_water_t), view(&t_detail_t), view(&t_vol_t), view(&t_map_t));

        let ub_usage = wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST;
        let mk_ubo = |size: u64| d.create_buffer(&wgpu::BufferDescriptor { label: None, size, usage: ub_usage, mapped_at_creation: false });
        let (ubo, ubo_s, ubo_ss, ubo_w, ubo_up) = (mk_ubo((UBN * 4) as u64), mk_ubo((UBN * 4) as u64), mk_ubo((UBN * 4) as u64), mk_ubo(16), mk_ubo(16));

        use wgpu::ShaderStages as SS;
        let vf = SS::VERTEX | SS::FRAGMENT;
        let ent = |binding: u32, visibility: SS, ty: wgpu::BindingType| wgpu::BindGroupLayoutEntry { binding, visibility, ty, count: None };
        let tex_ty = |sample_type: wgpu::TextureSampleType, dim: wgpu::TextureViewDimension, ms: bool| wgpu::BindingType::Texture { sample_type, view_dimension: dim, multisampled: ms };
        let ff = wgpu::TextureSampleType::Float { filterable: true };
        let d2 = wgpu::TextureViewDimension::D2;
        let samp = |t: wgpu::SamplerBindingType| wgpu::BindingType::Sampler(t);
        let ubuf = wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None };
        let bgl0 = d.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor { label: None, entries: &[
            ent(0, vf, ubuf), ent(1, vf, samp(wgpu::SamplerBindingType::Filtering)), ent(2, vf, samp(wgpu::SamplerBindingType::Filtering)), ent(3, vf, samp(wgpu::SamplerBindingType::Comparison)),
            ent(4, vf, tex_ty(wgpu::TextureSampleType::Depth, d2, false)), ent(5, vf, tex_ty(ff, d2, false)), ent(6, vf, tex_ty(ff, d2, false)),
            ent(7, vf, tex_ty(ff, wgpu::TextureViewDimension::D3, false)), ent(8, vf, tex_ty(ff, d2, false)), ent(9, vf, tex_ty(wgpu::TextureSampleType::Depth, d2, false)), ent(10, SS::FRAGMENT, tex_ty(ff, d2, false)),
        ] });
        let mk_bg0 = |u: &wgpu::Buffer, shv: &wgpu::TextureView, ssv: &wgpu::TextureView| d.create_bind_group(&wgpu::BindGroupDescriptor { label: None, layout: &bgl0, entries: &[
            wgpu::BindGroupEntry { binding: 0, resource: u.as_entire_binding() }, wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&s_rep) },
            wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::Sampler(&s_clamp) }, wgpu::BindGroupEntry { binding: 3, resource: wgpu::BindingResource::Sampler(&s_cmp) },
            wgpu::BindGroupEntry { binding: 4, resource: wgpu::BindingResource::TextureView(shv) }, wgpu::BindGroupEntry { binding: 5, resource: wgpu::BindingResource::TextureView(&t_water) },
            wgpu::BindGroupEntry { binding: 6, resource: wgpu::BindingResource::TextureView(&t_detail) }, wgpu::BindGroupEntry { binding: 7, resource: wgpu::BindingResource::TextureView(&t_vol) },
            wgpu::BindGroupEntry { binding: 8, resource: wgpu::BindingResource::TextureView(&t_map) }, wgpu::BindGroupEntry { binding: 9, resource: wgpu::BindingResource::TextureView(ssv) }, wgpu::BindGroupEntry { binding: 10, resource: wgpu::BindingResource::TextureView(&t_fish) },
        ] });
        let bg0 = mk_bg0(&ubo, &t_shadow, &t_ss); let bg0s = mk_bg0(&ubo_s, &t_dummy, &t_dummy); let bg0_ss = mk_bg0(&ubo_ss, &t_dummy, &t_dummy);
        let ubuf_dyn = wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: true, min_binding_size: wgpu::BufferSize::new(96) };
        let bgl_plush = d.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor { label: None, entries: &[ent(0, vf, wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: true, min_binding_size: wgpu::BufferSize::new(80) })] });
        let bgl_shark = d.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor { label: None, entries: &[ent(0, vf, ubuf_dyn), ent(1, SS::FRAGMENT, tex_ty(ff, d2, false)), ent(2, SS::FRAGMENT, tex_ty(ff, d2, false))] });
        let pl0 = d.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: None, bind_group_layouts: &[Some(&bgl0)], immediate_size: 0 });
        let pl_plush = d.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: None, bind_group_layouts: &[Some(&bgl0), Some(&bgl_plush)], immediate_size: 0 });
        let pl_shark = d.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: None, bind_group_layouts: &[Some(&bgl0), Some(&bgl_shark)], immediate_size: 0 });
        let bgl_blit = d.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor { label: None, entries: &[ent(0, SS::FRAGMENT, tex_ty(ff, d2, false)), ent(1, SS::FRAGMENT, samp(wgpu::SamplerBindingType::Filtering))] });
        let pl_blit = d.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: None, bind_group_layouts: &[Some(&bgl_blit)], immediate_size: 0 });
        let module = |src: String, label: &str| d.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some(label), source: wgpu::ShaderSource::Wgsl(src.into()) });

        progress("Compiling shaders", 0.05); crate::yield_now().await;
        let m_blit = module(defs.blit(), "blit");
        let add = wgpu::BlendState { color: wgpu::BlendComponent { src_factor: wgpu::BlendFactor::One, dst_factor: wgpu::BlendFactor::One, operation: wgpu::BlendOperation::Add }, alpha: wgpu::BlendComponent { src_factor: wgpu::BlendFactor::One, dst_factor: wgpu::BlendFactor::One, operation: wgpu::BlendOperation::Add } };
        let pre = wgpu::BlendState { color: wgpu::BlendComponent { src_factor: wgpu::BlendFactor::One, dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha, operation: wgpu::BlendOperation::Add }, alpha: wgpu::BlendComponent { src_factor: wgpu::BlendFactor::One, dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha, operation: wgpu::BlendOperation::Add } };
        let fs_pipe = |layout: Option<&wgpu::PipelineLayout>, m: &wgpu::ShaderModule, entry: &str, format: wgpu::TextureFormat, blend: Option<wgpu::BlendState>| d.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some(entry), layout, vertex: wgpu::VertexState { module: m, entry_point: Some("vs"), buffers: &[], compilation_options: Default::default() },
            primitive: Default::default(), depth_stencil: None, multisample: Default::default(),
            fragment: Some(wgpu::FragmentState { module: m, entry_point: Some(entry), targets: &[Some(wgpu::ColorTargetState { format, blend, write_mask: wgpu::ColorWrites::ALL })], compilation_options: Default::default() }),
            multiview_mask: None, cache: None });
        let p_mip8 = fs_pipe(Some(&pl_blit), &m_blit, "fsMip", wgpu::TextureFormat::Rgba8Unorm, None);
        let p_mip16 = fs_pipe(Some(&pl_blit), &m_blit, "fsMip", HDR, None);
        let p_down0 = fs_pipe(Some(&pl_blit), &m_blit, "fsDown0", HDR, None);
        let p_down = fs_pipe(Some(&pl_blit), &m_blit, "fsDown", HDR, None);
        let p_up = fs_pipe(Some(&pl_blit), &m_blit, "fsUp", HDR, Some(add));
        let blit_bg = |v: &wgpu::TextureView| d.create_bind_group(&wgpu::BindGroupDescriptor { label: None, layout: &bgl_blit, entries: &[wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(v) }, wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&s_clamp) }] });

        // vertex layouts
        use wgpu::VertexFormat as VF;
        let at = |l: u32, o: u64, f: VF| wgpu::VertexAttribute { shader_location: l, offset: o, format: f };
        let a_solid = [at(0, 0, VF::Float32x3), at(1, 12, VF::Snorm8x4)];
        let a_inst = [at(2, 0, VF::Float32x4), at(3, 16, VF::Float32x4), at(4, 32, VF::Float32x4)];
        let a_leaf = [at(0, 0, VF::Float32x4), at(1, 16, VF::Float32x4), at(2, 32, VF::Unorm8x4)];
        let a_anim = [at(2, 0, VF::Float32x4), at(3, 16, VF::Float32x4)];
        let a_bub = [at(0, 0, VF::Float32x4), at(1, 16, VF::Float32x4)];
        let a_snow = [at(0, 0, VF::Float32x4)];
        let a_plush = [at(0, 0, VF::Float32x3), at(1, 12, VF::Float32x3), at(2, 24, VF::Unorm8x4), at(3, 28, VF::Float32x2)];
        let a_shark = [at(0, 0, VF::Float32x3), at(1, 12, VF::Float32x3), at(2, 24, VF::Float32x2), at(3, 32, VF::Float32)];
        let vl = |stride: u64, inst: bool, attributes: &'static [wgpu::VertexAttribute]| wgpu::VertexBufferLayout { array_stride: stride, step_mode: if inst { wgpu::VertexStepMode::Instance } else { wgpu::VertexStepMode::Vertex }, attributes };
        // leak the attribute arrays: they live for the program anyway
        let leak = |a: &[wgpu::VertexAttribute]| -> &'static [wgpu::VertexAttribute] { Box::leak(a.to_vec().into_boxed_slice()) };
        let (a_solid, a_inst, a_leaf, a_anim, a_bub, a_snow, a_shark, a_plush) = (leak(&a_solid), leak(&a_inst), leak(&a_leaf), leak(&a_anim), leak(&a_bub), leak(&a_snow), leak(&a_shark), leak(&a_plush));
        let vb_solid = vl(16, false, a_solid); let ib_solid = vl(48, true, a_inst); let ib_leaf = vl(36, true, a_leaf); let ib_anim = vl(32, true, a_anim);
        let ib_bub = vl(32, true, a_bub); let ib_snow = vl(16, true, a_snow); let vb_shark_l = vl(36, false, a_shark); let vb_plush_l = vl(creatures::STRIDE as u64, false, a_plush);
        let aa = q.aa();

        struct Po<'a> { strip: bool, cull: Option<wgpu::Face>, blend: Option<wgpu::BlendState>, consts: &'a [(&'a str, f64)] }
        let po = |strip: bool, cull: Option<wgpu::Face>, blend: Option<wgpu::BlendState>| Po { strip, cull, blend, consts: &[] };
        let prim = |o: &Po| wgpu::PrimitiveState { topology: if o.strip { wgpu::PrimitiveTopology::TriangleStrip } else { wgpu::PrimitiveTopology::TriangleList }, cull_mode: o.cull, ..Default::default() };
        let scene_pipe = |layout: &wgpu::PipelineLayout, m: &wgpu::ShaderModule, vs: &str, fs: &str, buffers: &[wgpu::VertexBufferLayout], o: Po| { let buffers: Vec<Option<wgpu::VertexBufferLayout>> = buffers.iter().cloned().map(Some).collect(); d.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some(vs), layout: Some(layout),
            vertex: wgpu::VertexState { module: m, entry_point: Some(vs), buffers: &buffers, compilation_options: wgpu::PipelineCompilationOptions { constants: o.consts, ..Default::default() } },
            fragment: Some(wgpu::FragmentState { module: m, entry_point: Some(fs), targets: &[Some(wgpu::ColorTargetState { format: HDR, blend: o.blend, write_mask: wgpu::ColorWrites::ALL })], compilation_options: wgpu::PipelineCompilationOptions { constants: o.consts, ..Default::default() } }),
            primitive: prim(&o),
            depth_stencil: Some(wgpu::DepthStencilState { format: DEPTH, depth_write_enabled: Some(o.blend.is_none()), depth_compare: Some(wgpu::CompareFunction::Greater), stencil: Default::default(), bias: Default::default() }),
            multisample: wgpu::MultisampleState { count: aa, ..Default::default() }, multiview_mask: None, cache: None }) };
        let shadow_pipe = |layout: &wgpu::PipelineLayout, m: &wgpu::ShaderModule, vs: &str, buffers: &[wgpu::VertexBufferLayout], o: Po| { let buffers: Vec<Option<wgpu::VertexBufferLayout>> = buffers.iter().cloned().map(Some).collect(); d.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("shadow"), layout: Some(layout),
            vertex: wgpu::VertexState { module: m, entry_point: Some(vs), buffers: &buffers, compilation_options: wgpu::PipelineCompilationOptions { constants: o.consts, ..Default::default() } },
            fragment: None, primitive: prim(&o),
            depth_stencil: Some(wgpu::DepthStencilState { format: DEPTH, depth_write_enabled: Some(true), depth_compare: Some(wgpu::CompareFunction::Less), stencil: Default::default(), bias: wgpu::DepthBiasState { constant: 2, slope_scale: 1.5, clamp: 0.0 } }),
            multisample: Default::default(), multiview_mask: None, cache: None }) };

        let m_solid = module(defs.solid(), "solid"); let m_leaf = module(defs.leaf(), "leaf"); let m_anim = module(defs.anim(), "anim");
        let m_surf = module(defs.surf(), "surf"); let m_spr = module(defs.spr(), "spr"); let m_shark = module(defs.shark(), "shark");
        let back = Some(wgpu::Face::Back);
        let p_terrain = scene_pipe(&pl0, &m_solid, "vs", "fs", &[vb_solid.clone(), ib_solid.clone()], po(false, back, None));
        let p_solid = scene_pipe(&pl0, &m_solid, "vs", "fs", &[vb_solid.clone(), ib_solid.clone()], po(false, None, None));
        let leafp = |rows: f64, far: Option<f64>| { let c: Vec<(&str, f64)> = match far { Some(f) => vec![("ROWS", rows), ("FAR", f)], None => vec![("ROWS", rows)] }; scene_pipe(&pl0, &m_leaf, "vs", "fs", &[ib_leaf.clone()], Po { strip: true, cull: None, blend: None, consts: &c }) };
        let p_leaf = leafp(9.0, None); let p_leaf5 = leafp(5.0, None); let p_leaf_s = leafp(3.0, Some(70.0)); let p_leaf_f = leafp(3.0, None);
        let fishp = |sp: f64| scene_pipe(&pl0, &m_anim, "vsFish", "fsFish", &[vb_solid.clone(), ib_anim.clone()], Po { strip: false, cull: None, blend: None, consts: &[("SP", sp)] });
        let p_fish = [fishp(0.0), fishp(1.0), fishp(2.0)];
        let p_manta = scene_pipe(&pl0, &m_anim, "vsManta", "fsManta", &[vb_solid.clone(), ib_anim.clone()], po(false, None, None));
        let p_surf = scene_pipe(&pl0, &m_surf, "vs", "fs", &[], po(true, None, None));
        let p_back = scene_pipe(&pl0, &m_surf, "vsB", "fsB", &[], po(true, None, None));
        let p_bub = scene_pipe(&pl0, &m_spr, "vsBub", "fsBub", &[ib_bub.clone()], po(true, None, Some(pre)));
        let p_snow = scene_pipe(&pl0, &m_spr, "vsSnow", "fsSnow", &[ib_snow.clone()], po(true, None, Some(add)));
        let p_gl = scene_pipe(&pl0, &m_leaf, "vsGl", "fsGl", &[ib_bub.clone()], Po { strip: true, cull: None, blend: Some(add), consts: &[("ROWS", 7.0)] });
        let m_plush = module(defs.plush(), "plush");
        let p_plush = scene_pipe(&pl_plush, &m_plush, "vs", "fs", &[vb_plush_l.clone()], po(false, None, None));
        let p_shark = scene_pipe(&pl_shark, &m_shark, "vs", "fs", &[vb_shark_l.clone()], po(false, None, None));
        let p_solid_sh = shadow_pipe(&pl0, &m_solid, "vs", &[vb_solid.clone(), ib_solid.clone()], po(false, None, None));
        let p_terr_sh = shadow_pipe(&pl0, &m_solid, "vs", &[vb_solid.clone(), ib_solid.clone()], po(false, Some(wgpu::Face::Front), None));
        let p_leaf_sh = shadow_pipe(&pl0, &m_leaf, "vs", &[ib_leaf.clone()], Po { strip: true, cull: None, blend: None, consts: &[("ROWS", 3.0)] });
        let p_shark_sh = shadow_pipe(&pl_shark, &m_shark, "vsSh", &[vb_shark_l.clone()], po(false, None, None));

        let m_water = module(defs.water(), "water");
        let p_water = fs_pipe(None, &m_water, "fs", HDR, None);
        let bg_water = d.create_bind_group(&wgpu::BindGroupDescriptor { label: None, layout: &p_water.get_bind_group_layout(0), entries: &[wgpu::BindGroupEntry { binding: 0, resource: ubo_w.as_entire_binding() }] });
        let water_views: Vec<wgpu::TextureView> = (0..5).map(|m| t_water_t.create_view(&wgpu::TextureViewDescriptor { base_mip_level: m, mip_level_count: Some(1), ..Default::default() })).collect();
        let water_bg: Vec<wgpu::BindGroup> = water_views.iter().map(|v| blit_bg(v)).collect();
        let m_rays = module(defs.rays(), "rays"); let m_post = module(defs.post(), "post"); let m_rb = module(defs.rblur(), "rblur");
        let p_rb = fs_pipe(None, &m_rb, "fs", HDR, None);
        let p_rays = fs_pipe(None, &m_rays, "fs", HDR, None);
        let p_post = fs_pipe(None, &m_post, "fs", wgpu::TextureFormat::Rgba8Unorm, None);
        let m_up = module(UPSCALE.to_string(), "upscale");
        let p_upscale = fs_pipe(None, &m_up, "fs", out_fmt, None);

        // ---------------------------------------------------------------- sun + shadow matrix
        let sun = { let v = [0.27, 1.0, -0.24]; let l = hypot3(v[0], v[1], v[2]); [v[0] / l, v[1] / l, v[2] / l] };
        let mut ub = [0f32; UBN]; let mut ubs = [0f32; UBN]; let mut ub_ss = [0f32; UBN];
        {
            let l = sun; let mut lr = [l[1], -l[0], 0.0]; let ll = hypot3(lr[0], lr[1], lr[2]); lr[0] /= ll; lr[1] /= ll;
            let lu = [l[1] * lr[2] - l[2] * lr[1], l[2] * lr[0] - l[0] * lr[2], l[0] * lr[1] - l[1] * lr[0]];
            let (r, d0, d1) = (225.0, -175.0, 125.0); let mut m = [0f64; 16];
            m[0] = lr[0] / r; m[4] = lr[1] / r; m[8] = lr[2] / r; m[12] = 0.0;
            m[1] = lu[0] / r; m[5] = lu[1] / r; m[9] = lu[2] / r; m[13] = 0.0;
            m[2] = -l[0] / (d1 - d0); m[6] = -l[1] / (d1 - d0); m[10] = -l[2] / (d1 - d0); m[14] = d1 / (d1 - d0);
            m[3] = 0.0; m[7] = 0.0; m[11] = 0.0; m[15] = 1.0;
            for i in 0..16 { ubs[16 + i] = m[i] as f32; ub[16 + i] = m[i] as f32; ubs[i] = m[i] as f32; }
            for i in 0..3 { ub[48 + i] = sun[i] as f32; ubs[48 + i] = sun[i] as f32; ub_ss[48 + i] = sun[i] as f32; }
            ubs[32] = 0.0; ubs[33] = -20.0; ubs[34] = 0.0; ubs[35] = 0.0; ubs[58] = 1.0;
            ub_ss[58] = 1.0;
            queue.write_buffer(&ubo_s, 0, bytemuck::cast_slice(&ubs));
        }
        // the shadow pass draws with vp = sm (the original sets vp in its shadow uniform block the same way)

        // ---------------------------------------------------------------- world generation
        progress("Sculpting the seabed", 0.15); crate::yield_now().await;
        let mut dens: Vec<f32>;
        {
            let prims = prim_buffer();
            let m = module(defs.dens(), "dens");
            let p = d.create_compute_pipeline(&wgpu::ComputePipelineDescriptor { label: None, layout: None, module: &m, entry_point: Some("main"), compilation_options: Default::default(), cache: None });
            let size = (NX * NY * NZ * 4) as u64;
            let mut u = [0u32; 12]; u[0] = NX as u32; u[1] = NY as u32; u[2] = NZ as u32;
            u[4] = (OX as f32).to_bits(); u[5] = (OY as f32).to_bits(); u[6] = (OZ as f32).to_bits(); u[7] = (VS as f32).to_bits();
            u[8] = MESA_N as u32; u[9] = ARCHES.len() as u32; u[10] = HOLES.len() as u32; u[11] = CLEARS.len() as u32;
            let bu = d.create_buffer_init(&wgpu::util::BufferInitDescriptor { label: None, contents: bytemuck::cast_slice(&u), usage: wgpu::BufferUsages::UNIFORM });
            let bp = d.create_buffer_init(&wgpu::util::BufferInitDescriptor { label: None, contents: bytemuck::cast_slice(&prims), usage: wgpu::BufferUsages::STORAGE });
            let bd = d.create_buffer(&wgpu::BufferDescriptor { label: None, size, usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC, mapped_at_creation: false });
            let br = d.create_buffer(&wgpu::BufferDescriptor { label: None, size, usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
            let bg = d.create_bind_group(&wgpu::BindGroupDescriptor { label: None, layout: &p.get_bind_group_layout(0), entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: bu.as_entire_binding() }, wgpu::BindGroupEntry { binding: 1, resource: bd.as_entire_binding() }, wgpu::BindGroupEntry { binding: 2, resource: bp.as_entire_binding() }] });
            {
                let mut e = d.create_command_encoder(&Default::default());
                {
                    let mut c = e.begin_compute_pass(&Default::default());
                    c.set_pipeline(&p); c.set_bind_group(0, &bg, &[]);
                    c.dispatch_workgroups((NX / 4) as u32, (NY / 4) as u32, (NZ / 4) as u32);
                }
                queue.submit([e.finish()]);
            }
            let mut e = d.create_command_encoder(&Default::default());
            e.copy_buffer_to_buffer(&bd, 0, &br, 0, size);
            queue.submit([e.finish()]);
            map_read(d, &br).await;
            dens = bytemuck::cast_slice::<u8, f32>(&br.slice(..).get_mapped_range().unwrap()).to_vec();
            br.unmap();
        }
        progress("Carving canyons", 0.3); crate::yield_now().await;
        drop_floating_rock(&mut dens);
        let mut field = Field::new(&dens);
        let t = build_terrain(&dens);
        let fp = FlyPath::new();
        let mut fly_pts: Vec<V3> = vec![];
        let mut tt = 0.0; while tt <= 60.0 { let p = fp.at(fly_frac(tt / 60.0)); fly_pts.push([p[0], p[1], p[2]]); tt += 0.1; }
        for p in POSES { fly_pts.push([p[0], -40.0, p[2]]); }
        progress("Growing kelp", 0.45); crate::yield_now().await;
        let mut wd = make_world(&dens, &t, &fly_pts, &POSES, lv);
        drop(dens);
        field.set_obstacles(std::mem::take(&mut wd.caps), std::mem::take(&mut wd.kelp), std::mem::take(&mut wd.obst));

        progress("Building set pieces", 0.8); crate::yield_now().await;
        // mesh ids: 0-3 rocks, 4-6 sponges, 7-8 coral, 9 tube sponge, 10 whale fluke, 11 ship, 12 manta, 13 fish,
        // 14-20 rock chunks + pebbles, 21-22 coral LOD, 23-25 far LOD of the large rock chunks, 26-27 more fish
        let mut built: Vec<Built> = [mesh_chunk(1, 3, true, lv), mesh_chunk(7, 3, true, lv), mesh_chunk(13, 3, true, lv), mesh_chunk(22, 3, true, lv),
            mesh_lathe(&[[0., 0., 0.5], [0.34, 0., 0.6], [0.5, 0.2, 0.85], [0.6, 0.6, 1.], [0.55, 0.95, 1.], [0.47, 1.08, 1.], [0.38, 1.1, 0.9], [0.3, 1.0, 0.45], [0.26, 0.6, 0.15], [0., 0.35, 0.05]], 18, 0.3, 9.),
            mesh_lathe(&[[0., 0., 0.5], [0.2, 0., 0.6], [0.26, 0.5, 0.85], [0.4, 1.25, 1.], [0.4, 1.42, 1.], [0.33, 1.48, 0.9], [0.26, 1.38, 0.4], [0.18, 0.8, 0.12], [0., 0.5, 0.05]], 16, 0.35, 8.),
            mesh_lathe(&[[0., 0., 0.5], [0.5, 0., 0.7], [0.72, 0.18, 1.], [0.74, 0.38, 1.], [0.62, 0.5, 0.9], [0.48, 0.42, 0.45], [0.4, 0.2, 0.15], [0., 0.14, 0.06]], 18, 0.25, 10.),
            mesh_coral(false, lv > 1), mesh_coral(true, lv > 1), mesh_tubes()].iter().map(|m| m.build()).collect();
        built.push(mesh_fluke());
        for m in [mesh_ship(), mesh_manta(), mesh_fish(0), mesh_chunk(3, 2, false, lv), mesh_chunk(11, 2, false, lv), mesh_chunk(29, 2, false, lv), mesh_chunk(5, 3, true, lv), mesh_chunk(17, 3, true, lv), mesh_chunk(41, 3, true, lv),
            mesh_chunk(9, 1, false, lv), mesh_coral(false, true), mesh_coral(true, true), mesh_chunk(5, 2, true, lv), mesh_chunk(17, 2, true, lv), mesh_chunk(41, 2, true, lv), mesh_fish(1), mesh_fish(2)] { built.push(m.build()); }
        let (mut nv, mut ni) = (0usize, 0usize); let mut meshes = vec![];
        for m in &built { meshes.push(Mi { v0: nv as i32, i0: ni as u32, n: m.ib.len() as u32 }); nv += m.nv; ni += m.ib.len(); }
        let mut all_v = Vec::with_capacity(nv * 16); let mut all_i = Vec::with_capacity(ni);
        for m in &built { all_v.extend_from_slice(&m.vb); all_i.extend_from_slice(&m.ib); }
        let bi = |contents: &[u8], usage: wgpu::BufferUsages| d.create_buffer_init(&wgpu::util::BufferInitDescriptor { label: None, contents: if contents.is_empty() { &[0u8; 64] } else { contents }, usage: usage | wgpu::BufferUsages::COPY_DST });
        let vtx = wgpu::BufferUsages::VERTEX; let idx = wgpu::BufferUsages::INDEX;
        let vb_mesh = bi(&all_v, vtx); let ib_mesh = bi(bytemuck::cast_slice(&all_i), idx);
        let vb_terrain = bi(&t.vb, vtx); let ib_terrain = bi(bytemuck::cast_slice(&t.ib), idx);
        let n_solid = (wd.solids.len() / 12) as u32;
        let mut solid_data = wd.solids.clone(); solid_data.extend_from_slice(&[0., 0., 0., 1., 0., 0., 0., 1., 0., 0., 1., 0.]);
        let ib_solids = bi(bytemuck::cast_slice(&solid_data), vtx);
        let cb = |p: &Pack| CellBuf { start: p.start.clone(), lo: p.lo.clone(), hi: p.hi.clone(), buf: bi(&p.buf, vtx), n: p.n };
        let chunks: Vec<CellBuf> = wd.chunks.iter().map(cb).collect(); let props: Vec<CellBuf> = wd.props.iter().map(cb).collect();
        let (big, big_f, small) = (cb(&wd.big), cb(&wd.big_f), cb(&wd.small));
        let fish_probe: Vec<[f32; 8]> = wd.fish.iter().filter(|f| f.len() >= 8).map(|f| f[..8].try_into().unwrap()).collect();
        let fb = |v: &Vec<f32>| (bi(bytemuck::cast_slice(v), vtx), (v.len() / 8) as u32);
        let ib_fish = [fb(&wd.fish[0]), fb(&wd.fish[1]), fb(&wd.fish[2])];
        let (ib_manta, n_manta) = fb(&wd.mantas); let (ib_bub, n_bub) = fb(&wd.bubbles); let (ib_gl, n_gl) = fb(&wd.glints);
        let ib_snow = bi(bytemuck::cast_slice(&wd.snow), vtx); let n_snow = (wd.snow.len() / 4) as u32;
        queue.write_texture(wgpu::TexelCopyTextureInfo { texture: &t_vol_t, mip_level: 0, origin: Default::default(), aspect: Default::default() }, &wd.vol_d,
            wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(NX as u32), rows_per_image: Some((NY / 2) as u32) }, wgpu::Extent3d { width: (NX / 2) as u32, height: (NY / 2) as u32, depth_or_array_layers: (NZ / 2) as u32 });
        queue.write_texture(wgpu::TexelCopyTextureInfo { texture: &t_map_t, mip_level: 0, origin: Default::default(), aspect: Default::default() }, &wd.gm,
            wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(2048), rows_per_image: None }, wgpu::Extent3d { width: 512, height: 512, depth_or_array_layers: 1 });

        // ---------------------------------------------------------------- the shark
        let (cw, ch, crgba) = &shark.color; let (rw, rh, rrgba) = &shark.rough;
        let up_tex = |w: u32, h: u32, data: &[u8], fmt: wgpu::TextureFormat| {
            let t = tex(w, h, 1, fmt, tb | cd, 1, 1);
            queue.write_texture(wgpu::TexelCopyTextureInfo { texture: &t, mip_level: 0, origin: Default::default(), aspect: Default::default() }, data,
                wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(w * 4), rows_per_image: None }, wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 });
            view(&t)
        };
        let t_col = up_tex(*cw, *ch, crgba, wgpu::TextureFormat::Rgba8UnormSrgb); let t_rough = up_tex(*rw, *rh, rrgba, wgpu::TextureFormat::Rgba8Unorm);
        let ubo_shark = mk_ubo((SHARK_SLOT * MAX_SHARKS) as u64);
        let bg_shark = d.create_bind_group(&wgpu::BindGroupDescriptor { label: None, layout: &bgl_shark, entries: &[
            wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding { buffer: &ubo_shark, offset: 0, size: wgpu::BufferSize::new(96) }) }, wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&t_col) }, wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::TextureView(&t_rough) }] });
        let vb_shark = bi(bytemuck::cast_slice(&shark.verts), vtx); let ib_shark = bi(bytemuck::cast_slice(&shark.idx), idx); let n_shark = shark.idx.len() as u32;

        // ---------------------------------------------------------------- the other plush animals (whale, octopus, tree)
        let (vb_plush, ib_plush, plush_ranges) = {
            let ms = [creatures::whale(), creatures::octopus(), creatures::tree(7)];
            let (mut v, mut ix, mut rg) = (vec![], vec![], [(0u32, 0u32); 3]);
            for (k, m) in ms.iter().enumerate() { let base = (v.len() / creatures::STRIDE) as u32; rg[k] = (ix.len() as u32, m.i.len() as u32); v.extend_from_slice(&m.v); ix.extend(m.i.iter().map(|i| i + base)); }
            (bi(&v, vtx), bi(bytemuck::cast_slice(&ix), idx), rg)
        };
        let ubo_plush = mk_ubo((SHARK_SLOT * MAX_PLUSH) as u64);
        let bg_plush = d.create_bind_group(&wgpu::BindGroupDescriptor { label: None, layout: &bgl_plush, entries: &[wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding { buffer: &ubo_plush, offset: 0, size: wgpu::BufferSize::new(80) }) }] });

        // ---------------------------------------------------------------- one-time bakes: detail texture, shadow map
        {
            let m = module(defs.detail(), "detail");
            let p = fs_pipe(None, &m, "fs", wgpu::TextureFormat::Rgba8Unorm, None);
            let mut e = d.create_command_encoder(&Default::default());
            let dv: Vec<wgpu::TextureView> = (0..11).map(|i| t_detail_t.create_view(&wgpu::TextureViewDescriptor { base_mip_level: i, mip_level_count: Some(1), ..Default::default() })).collect();
            {
                let mut r = e.begin_render_pass(&wgpu::RenderPassDescriptor { label: None, color_attachments: &[att(&dv[0], true)], depth_stencil_attachment: None, timestamp_writes: None, occlusion_query_set: None, multiview_mask: None });
                r.set_pipeline(&p); r.draw(0..3, 0..1);
            }
            for i in 1..11 {
                let bg = blit_bg(&dv[i - 1]);
                let mut r = e.begin_render_pass(&wgpu::RenderPassDescriptor { label: None, color_attachments: &[att(&dv[i], true)], depth_stencil_attachment: None, timestamp_writes: None, occlusion_query_set: None, multiview_mask: None });
                r.set_pipeline(&p_mip8); r.set_bind_group(0, &bg, &[]); r.draw(0..3, 0..1);
            }
            {
                let mut r = e.begin_render_pass(&wgpu::RenderPassDescriptor { label: None, color_attachments: &[], depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment { view: &t_shadow, depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(1.0), store: wgpu::StoreOp::Store }), stencil_ops: None }), timestamp_writes: None, occlusion_query_set: None, multiview_mask: None });
                r.set_bind_group(0, &bg0s, &[]);
                r.set_pipeline(&p_terr_sh); r.set_vertex_buffer(0, vb_terrain.slice(..)); r.set_vertex_buffer(1, ib_solids.slice(..)); r.set_index_buffer(ib_terrain.slice(..), wgpu::IndexFormat::Uint32);
                r.draw_indexed(0..t.ib.len() as u32, 0, n_solid..n_solid + 1);
                r.set_pipeline(&p_solid_sh); r.set_vertex_buffer(0, vb_mesh.slice(..)); r.set_index_buffer(ib_mesh.slice(..), wgpu::IndexFormat::Uint32);
                for i in 0..12 { let g = wd.ranges[i]; if g.1 > 0 { let mm = &meshes[i]; r.draw_indexed(mm.i0..mm.i0 + mm.n, mm.v0, g.0..g.0 + g.1); } }
                // small rocks, sponges and corals (~35M triangles) only on High
                if lv == 0 {
                    for i in 0..NCHUNK { if chunks[i].n > 0 { r.set_vertex_buffer(1, chunks[i].buf.slice(..)); let mm = &meshes[14 + i]; r.draw_indexed(mm.i0..mm.i0 + mm.n, mm.v0, 0..chunks[i].n); } }
                    for i in 0..6 { if props[i].n > 0 { r.set_vertex_buffer(1, props[i].buf.slice(..)); let mm = &meshes[4 + i]; r.draw_indexed(mm.i0..mm.i0 + mm.n, mm.v0, 0..props[i].n); } }
                }
                if big.n > 0 { r.set_pipeline(&p_leaf_sh); r.set_vertex_buffer(0, big.buf.slice(..)); r.draw(0..6, 0..big.n); }
            }
            queue.submit([e.finish()]);
        }
        let _ = device.poll(wgpu::PollType::wait_indefinitely());
        progress("Letting the light in", 1.0); crate::yield_now().await;

        let r = Renderer {
            device, queue, out_fmt, q, ubo, ubo_ss, ubo_w, ubo_up, ub, ub_ss, bg0, bg0_ss, s_clamp, s_rep, s_cmp, t_shadow, t_water, t_vol, t_ss,
            water_views, water_bg, bg_water, p_terrain, p_solid, p_leaf, p_leaf5, p_leaf_s, p_leaf_f, p_fish, p_manta, p_surf, p_back, p_bub, p_snow, p_gl,
            p_water, p_mip16, p_down0, p_down, p_up, p_rays, p_rb, p_post, p_upscale, p_plush, bg_plush, ubo_plush, vb_plush, ib_plush, plush_ranges, season: 0, fish_probe, p_shark, p_shark_sh, bg_shark, ubo_shark, vb_shark, ib_shark, n_shark,
            vb_mesh, ib_mesh, vb_terrain, ib_terrain, ib_solids, n_solid, meshes, ranges: wd.ranges, t_start: t.start, t_lo: t.lo, t_hi: t.hi,
            chunks, props, big, big_f, small, ib_fish, ib_manta, n_manta, ib_bub, n_bub, ib_snow, n_snow, ib_gl, n_gl,
            tg: None, f: [0.0; 12], tanh: 0.6, row_ord: [0; CELLS], det: 1.0, pix: [2560.0 * 1440.0, 1280.0 * 720.0, 41e4][lv.min(2) as usize],
            dyn_t: 0.0, dyn_n: 0, dyn_bad: 0, dyn_ok: 0, dyn_wait: 3,
        };
        (r, field)
    }

    /// (re)builds the render targets for an output of dw x dh pixels; the scene is drawn at `pix` pixels
    pub fn resize(&mut self, dw: u32, dh: u32) {
        let dw = dw.max(2); let dh = dh.max(2);
        let s = (self.pix / (dw as f64 * dh as f64)).sqrt().min(1.0);
        let w = (((dw as f64 * s).round() as u32) & !1).max(2); let h = (((dh as f64 * s).round() as u32) & !1).max(2);
        if let Some(t) = &self.tg { if t.w == w && t.h == h && t.dw == dw && t.dh == dh { return; } }
        self.dyn_n = 0; self.dyn_t = -300.0; // let the new size settle before measuring again
        let d = &self.device; let aa = self.q.aa();
        let tex = |w: u32, h: u32, format: wgpu::TextureFormat, samples: u32, usage: wgpu::TextureUsages| d.create_texture(&wgpu::TextureDescriptor { label: None, size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 }, mip_level_count: 1, sample_count: samples, dimension: wgpu::TextureDimension::D2, format, usage, view_formats: &[] }).create_view(&Default::default());
        let ra = wgpu::TextureUsages::RENDER_ATTACHMENT; let tb = wgpu::TextureUsages::TEXTURE_BINDING;
        let ms = tex(if aa > 1 { w } else { 1 }, h, HDR, aa, ra);
        let dp = tex(w, h, DEPTH, aa, ra | tb);
        let res = tex(w, h, HDR, 1, ra | tb); let ray = tex(w / 2, h / 2, HDR, 1, ra | tb); let ray2 = tex(w / 2, h / 2, HDR, 1, ra | tb);
        let ldr = tex(w, h, wgpu::TextureFormat::Rgba8Unorm, 1, ra | tb);
        let bloom: Vec<wgpu::TextureView> = (0..6).map(|i| tex((w >> (i + 1)).max(1), (h >> (i + 1)).max(1), HDR, 1, ra | tb)).collect();
        let bg_blur = d.create_bind_group(&wgpu::BindGroupDescriptor { label: None, layout: &self.p_rb.get_bind_group_layout(0), entries: &[e(0, tv(&ray))] });
        let bg_rays = d.create_bind_group(&wgpu::BindGroupDescriptor { label: None, layout: &self.p_rays.get_bind_group_layout(0), entries: &[
            e(0, self.ubo.as_entire_binding()), e(1, wgpu::BindingResource::Sampler(&self.s_rep)), e(2, wgpu::BindingResource::Sampler(&self.s_cmp)), e(3, tv(&self.t_shadow)), e(4, tv(&self.t_water)), e(5, tv(&dp)), e(6, tv(&self.t_vol)), e(7, wgpu::BindingResource::Sampler(&self.s_clamp)), e(8, tv(&self.t_ss))] });
        let bg_post = d.create_bind_group(&wgpu::BindGroupDescriptor { label: None, layout: &self.p_post.get_bind_group_layout(0), entries: &[
            e(0, self.ubo.as_entire_binding()), e(1, wgpu::BindingResource::Sampler(&self.s_clamp)), e(2, tv(&res)), e(3, tv(&ray2)), e(4, tv(&bloom[0])), e(5, tv(&dp))] });
        let lay = self.p_down.get_bind_group_layout(0); let blit_bg = |v: &wgpu::TextureView| d.create_bind_group(&wgpu::BindGroupDescriptor { label: None, layout: &lay, entries: &[e(0, tv(v)), e(1, wgpu::BindingResource::Sampler(&self.s_clamp))] });
        // down: res -> b0 -> ... -> b5 ; up: b5 -> b4 (additive) ... -> b0
        let mut bloom_bg = vec![];
        for i in 0..6 { bloom_bg.push(blit_bg(if i > 0 { &bloom[i - 1] } else { &res })); }
        for i in (1..6).rev() { bloom_bg.push(blit_bg(&bloom[i])); }
        let bg_up = d.create_bind_group(&wgpu::BindGroupDescriptor { label: None, layout: &self.p_upscale.get_bind_group_layout(0), entries: &[e(0, tv(&ldr)), e(1, wgpu::BindingResource::Sampler(&self.s_clamp)), e(2, self.ubo_up.as_entire_binding())] });
        self.ub[52] = w as f32; self.ub[53] = h as f32; self.ub[54] = 1.0 / w as f32; self.ub[55] = 1.0 / h as f32;
        self.tg = Some(Targets { w, h, dw, dh, ms, dp, res, ray, ray2, ldr, bloom, bg_blur, bg_rays, bg_post, bloom_bg, bg_up });
    }

    /// Dynamic resolution and draw distance for ~60 fps, from the CPU-side frame time (ms) (main.js frame()).
    pub fn tune(&mut self, dtr: f64) {
        if dtr >= 1e3 { return; }
        self.dyn_t += dtr; if self.dyn_t <= 0.0 { return; }
        self.dyn_n += 1;
        if self.dyn_t < 1000.0 { return; }
        let a = self.dyn_t / self.dyn_n as f64; let mut k = 1.0;
        // slow for two periods in a row means fewer pixels, and twice as long before trying to go back up
        if a > 17.6 { self.dyn_ok = 0; self.dyn_bad += 1; if self.dyn_bad > 1 { k = 15.0 / a; self.dyn_bad = 0; self.dyn_wait = (self.dyn_wait * 2).min(60); } }
        // at the display's rate there is no telling if there is room, so try a little more now and then
        else { self.dyn_bad = 0; if a < 12.5 { k = 15.0 / a; } else { self.dyn_ok += 1; if self.dyn_ok > self.dyn_wait { k = 1.17; self.dyn_ok = 0; } } }
        let k = clamp(k, 0.5, 1.7);
        let (dw, dh) = self.tg.as_ref().map(|t| (t.dw, t.dh)).unwrap_or((1920, 1080));
        let top = (2560.0 * 1440.0f64).min(dw as f64 * dh as f64); let pixmin = 2.2e5f64.min(top); let mid = (top / 2.0).max(pixmin); let detmin = 0.4;
        let (pix, det) = (self.pix, self.det);
        if if k < 1.0 { pix > mid || det <= detmin } else { pix < mid || det >= 1.0 } {
            let end = if k < 1.0 { if det > detmin { mid } else { pixmin } } else if det < 1.0 { mid } else { top };
            self.pix *= k; if if k < 1.0 { self.pix < end } else { self.pix > end } { self.pix = end; }
        } else { self.det = clamp(det * k.sqrt(), detmin, 1.0); }
        self.dyn_n = 0; self.dyn_t = 0.0;
        self.resize(dw, dh);
    }
    /// true while even the lowest resolution and draw distance are too slow (the web page then drops a level)
    pub fn struggling(&self) -> bool { self.pix <= 2.3e5 && self.det <= 0.41 }
    pub fn level(&self) -> u32 { self.q.lv }
    pub fn render_size(&self) -> (u32, u32) { self.tg.as_ref().map(|t| (t.w, t.h)).unwrap_or((0, 0)) }

    /// Draws the cells of a cell-sorted buffer that are in view and within [min_d, max_d) (scaled by `det`, but not
    /// for the terrain) as merged runs, nearest rows first.
    fn draw_cells(&self, r: &mut wgpu::RenderPass, start: &[u32], lo: &[f32], hi: &[f32], mar: f64, kind: DrawKind, min_d: f64, max_d: f64) {
        let f = &self.f; let (cx, cy, cz, fx, fy, fz, rx, rz, ux, uy, uz) = (f[0], f[1], f[2], f[3], f[4], f[5], f[6], f[7], f[8], f[9], f[10]);
        let (w, h) = self.render_size(); let tanh = self.tanh;
        let ds = if matches!(kind, DrawKind::Terrain) { 1.0 } else { self.det * self.det };
        let tx = tanh * w as f64 / h as f64; let hw = CELLW * 0.5; let ex = hw + mar; let min2 = min_d * min_d * ds; let max2 = max_d * max_d * ds;
        for k in 0..CELLS {
            let rw = self.row_ord[k]; let row = rw * CELLS; let pz = OZ + (rw as f64 + 0.5) * CELLW - cz; let hz = (pz.abs() - hw).max(0.0);
            let mut run: isize = -1;
            for x in 0..=CELLS {
                let c = row + x; let mut v = false;
                if x < CELLS {
                    let (l, hh) = (lo[c] as f64, hi[c] as f64);
                    if l <= hh {
                        let px = OX + (x as f64 + 0.5) * CELLW - cx; let py = (l + hh) * 0.5 - cy; let ey = (hh - l) * 0.5 + mar; let hx = (px.abs() - hw).max(0.0); let h2 = hx * hx + hz * hz;
                        if h2 < max2 && h2 >= min2 {
                            let lim = px * fx + py * fy + pz * fz + ex * fx.abs() + ey * fy.abs() + ex * fz.abs();
                            if lim >= 0.0 && (px * rx + pz * rz).abs() - ex * (rx.abs() + rz.abs()) <= lim * tx { v = (px * ux + py * uy + pz * uz).abs() - ex * (ux.abs() + uz.abs()) - ey * uy.abs() <= lim * tanh; }
                        }
                    }
                }
                if v { if run < 0 { run = c as isize; } } else if run >= 0 {
                    let a = start[run as usize]; let n = start[c] - a;
                    if n > 0 { match &kind {
                        DrawKind::Terrain => r.draw_indexed(a * 6..(a + n) * 6, 0, self.n_solid..self.n_solid + 1),
                        DrawKind::Mesh(m) => { let mm = &self.meshes[*m]; r.draw_indexed(mm.i0..mm.i0 + mm.n, mm.v0, a..a + n) }
                        DrawKind::Strip(verts) => r.draw(0..*verts, a..a + n),
                    } }
                    run = -1;
                }
            }
        }
    }

    pub fn render(&mut self, cam: &Cam, time: f64, sharks: &[SharkPose], plush: &[PlushPose], out: &wgpu::TextureView) {
        let sk = &sharks[0];
        let tg = self.tg.as_ref().expect("resize first");
        let (w, h) = (tg.w, tg.h);
        // camera basis (right vector is horizontal)
        let [cx, cy, cz] = cam.pos; let (pitch, yaw) = (cam.pitch, cam.yaw); let tanh = cam.tanh; self.tanh = tanh;
        let cp = pitch.cos(); let fy = pitch.sin(); let rx = yaw.cos(); let rz = yaw.sin(); let fx = cp * rz; let fz = -cp * rx;
        let ux = -rz * fy; let uy = rz * fx - rx * fz; let uz = rx * fy;
        self.f = [cx, cy, cz, fx, fy, fz, rx, rz, ux, uy, uz, 0.0];
        let asp = w as f64 / h as f64; let sx = 1.0 / (tanh * asp); let sy = 1.0 / tanh; let near = 0.1;
        let m = &mut self.ub; let s = |v: f64| v as f32;
        m[0] = s(rx * sx); m[4] = 0.0; m[8] = s(rz * sx); m[12] = s(-(rx * cx + rz * cz) * sx);
        m[1] = s(ux * sy); m[5] = s(uy * sy); m[9] = s(uz * sy); m[13] = s(-(ux * cx + uy * cy + uz * cz) * sy);
        m[2] = 0.0; m[6] = 0.0; m[10] = 0.0; m[14] = s(near);
        m[3] = s(fx); m[7] = s(fy); m[11] = s(fz); m[15] = s(-(fx * cx + fy * cy + fz * cz));
        m[32] = s(cx); m[33] = s(cy); m[34] = s(cz); m[35] = s(time);
        m[36] = s(rx); m[37] = 0.0; m[38] = s(rz); m[39] = s(tanh * asp);
        m[40] = s(ux); m[41] = s(uy); m[42] = s(uz); m[43] = s(tanh);
        m[44] = s(fx); m[45] = s(fy); m[46] = s(fz); m[47] = s(near);
        let zx = cx - PURPLE[0]; let zz = cz - PURPLE[1]; let zt = clamp((44.0 - (zx * zx + zz * zz).sqrt()) / 18.0, 0.0, 1.0);
        m[51] = s(zt * zt * (3.0 - 2.0 * zt));
        let ek = clamp((-cy - 8.0) / 26.0, 0.0, 1.0);
        m[56] = s(0.62 * (0.6 + 0.4 * ek)); m[57] = 0.54; m[58] = s(self.det); m[59] = self.season as f32;
        // the shark's shadow map: an orthographic box along the sun around him
        let sun = [m[48] as f64, m[49] as f64, m[50] as f64];
        let mut ssm = [0f64; 16];
        {
            let l = sun; let mut lr = [l[1], -l[0], 0.0]; let ll = hypot3(lr[0], lr[1], lr[2]); lr[0] /= ll; lr[1] /= ll;
            let lu = [l[1] * lr[2] - l[2] * lr[1], l[2] * lr[0] - l[0] * lr[2], l[0] * lr[1] - l[1] * lr[0]];
            let r = 2.6 * sk.scale.max(0.1) * 7.4 / 2.0 + 0.5; let dd = 30.0; let c = sk.pos;
            let dot = |a: [f64; 3], b: [f64; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
            ssm[0] = lr[0] / r; ssm[4] = lr[1] / r; ssm[8] = lr[2] / r; ssm[12] = -dot(lr, c) / r;
            ssm[1] = lu[0] / r; ssm[5] = lu[1] / r; ssm[9] = lu[2] / r; ssm[13] = -dot(lu, c) / r;
            ssm[2] = -l[0] / (2.0 * dd); ssm[6] = -l[1] / (2.0 * dd); ssm[10] = -l[2] / (2.0 * dd); ssm[14] = (dot(l, c) + dd) / (2.0 * dd);
            ssm[15] = 1.0;
        }
        for i in 0..16 { m[60 + i] = ssm[i] as f32; self.ub_ss[i] = ssm[i] as f32; self.ub_ss[60 + i] = ssm[i] as f32; }
        m[76] = s(sk.pos[0]); m[77] = s(sk.pos[1]); m[78] = s(sk.pos[2]); m[79] = s(sk.scale * 3.4);
        m[80] = s(sk.fwd[0]); m[81] = s(sk.fwd[1]); m[82] = s(sk.fwd[2]); m[83] = s(sk.scale * 7.37 * 0.42);
        self.ub_ss[32] = m[32]; self.ub_ss[33] = m[33]; self.ub_ss[34] = m[34]; self.ub_ss[35] = m[35];
        self.queue.write_buffer(&self.ubo, 0, bytemuck::cast_slice(&self.ub));
        self.queue.write_buffer(&self.ubo_ss, 0, bytemuck::cast_slice(&self.ub_ss));
        self.queue.write_buffer(&self.ubo_w, 0, bytemuck::cast_slice(&[time as f32, 0.0, 0.0, 0.0]));
        self.queue.write_buffer(&self.ubo_up, 0, bytemuck::cast_slice(&[1.0 / tg.dw as f32, 1.0 / tg.dh as f32, clamp(cam.fade, 0.0, 1.0) as f32, 0.0]));
        let n_plush = plush.len().min(MAX_PLUSH);
        for (i, p) in plush.iter().take(MAX_PLUSH).enumerate() {
            let (f, up) = (p.fwd, p.up); let left = [up[1] * f[2] - up[2] * f[1], up[2] * f[0] - up[0] * f[2], up[0] * f[1] - up[1] * f[0]];
            let u: [f32; 20] = [p.pos[0], p.pos[1], p.pos[2], p.scale, f[0], f[1], f[2], p.kind as f64, up[0], up[1], up[2], 0.0, left[0], left[1], left[2], 0.0, p.a[0], p.a[1], p.a[2], 0.0].map(|v| v as f32);
            self.queue.write_buffer(&self.ubo_plush, (i * SHARK_SLOT) as u64, bytemuck::cast_slice(&u));
        }
        let n_sharks = sharks.len().min(MAX_SHARKS);
        for (i, sk) in sharks.iter().take(MAX_SHARKS).enumerate() {
            let up = sk.up; let fw = sk.fwd; let left = [up[1] * fw[2] - up[2] * fw[1], up[2] * fw[0] - up[0] * fw[2], up[0] * fw[1] - up[1] * fw[0]];
            let u: [f32; 24] = [sk.pos[0], sk.pos[1], sk.pos[2], sk.scale, fw[0], fw[1], fw[2], 0.0, up[0], up[1], up[2], 0.0, left[0], left[1], left[2], 0.0,
                sk.a[0], sk.a[1], sk.a[2], sk.a[3], sk.b[0], sk.b[1], sk.b[2], sk.b[3]].map(|v| v as f32);
            self.queue.write_buffer(&self.ubo_shark, (i * SHARK_SLOT) as u64, bytemuck::cast_slice(&u));
        }
        {
            let mut cr = clamp(((cz - OZ) / CELLW).floor(), 0.0, CELLS as f64 - 1.0) as isize; if cr < 0 { cr = 0; }
            let mut n = 0; self.row_ord[n] = cr as usize; n += 1;
            for dd in 1..CELLS as isize { if cr - dd >= 0 { self.row_ord[n] = (cr - dd) as usize; n += 1; } if cr + dd < CELLS as isize { self.row_ord[n] = (cr + dd) as usize; n += 1; } }
        }
        let mut e = self.device.create_command_encoder(&Default::default());
        // animated water texture + mips
        {
            let mut r = e.begin_render_pass(&wgpu::RenderPassDescriptor { label: None, color_attachments: &[att(&self.water_views[0], true)], depth_stencil_attachment: None, timestamp_writes: None, occlusion_query_set: None, multiview_mask: None }); r.set_pipeline(&self.p_water); r.set_bind_group(0, &self.bg_water, &[]); r.draw(0..3, 0..1);
        }
        for i in 1..5 { let mut r = e.begin_render_pass(&wgpu::RenderPassDescriptor { label: None, color_attachments: &[att(&self.water_views[i], true)], depth_stencil_attachment: None, timestamp_writes: None, occlusion_query_set: None, multiview_mask: None }); r.set_pipeline(&self.p_mip16); r.set_bind_group(0, &self.water_bg[i - 1], &[]); r.draw(0..3, 0..1); }
        // the shark's shadow
        {
            let mut r = e.begin_render_pass(&wgpu::RenderPassDescriptor { label: None, color_attachments: &[], depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment { view: &self.t_ss, depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(1.0), store: wgpu::StoreOp::Store }), stencil_ops: None }), timestamp_writes: None, occlusion_query_set: None, multiview_mask: None });
            r.set_pipeline(&self.p_shark_sh); r.set_bind_group(0, &self.bg0_ss, &[]);
            r.set_vertex_buffer(0, self.vb_shark.slice(..)); r.set_index_buffer(self.ib_shark.slice(..), wgpu::IndexFormat::Uint32);
            for i in 0..n_sharks { r.set_bind_group(1, &self.bg_shark, &[(i * SHARK_SLOT) as u32]); r.draw_indexed(0..self.n_shark, 0, 0..1); }
        }
        // scene
        {
            let aa = self.q.aa() > 1;
            let mut r = e.begin_render_pass(&wgpu::RenderPassDescriptor { label: None,
                color_attachments: &[Some(wgpu::RenderPassColorAttachment { view: if aa { &tg.ms } else { &tg.res }, depth_slice: None, resolve_target: if aa { Some(&tg.res) } else { None }, ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::BLACK), store: if aa { wgpu::StoreOp::Discard } else { wgpu::StoreOp::Store } } })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment { view: &tg.dp, depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(0.0), store: wgpu::StoreOp::Store }), stencil_ops: None }),
                timestamp_writes: None, occlusion_query_set: None, multiview_mask: None });
            r.set_bind_group(0, &self.bg0, &[]);
            r.set_pipeline(&self.p_terrain); r.set_vertex_buffer(0, self.vb_terrain.slice(..)); r.set_vertex_buffer(1, self.ib_solids.slice(..)); r.set_index_buffer(self.ib_terrain.slice(..), wgpu::IndexFormat::Uint32);
            self.draw_cells(&mut r, &self.t_start, &self.t_lo, &self.t_hi, 1.5, DrawKind::Terrain, 0.0, 240.0);
            r.set_pipeline(&self.p_solid); r.set_vertex_buffer(0, self.vb_mesh.slice(..)); r.set_index_buffer(self.ib_mesh.slice(..), wgpu::IndexFormat::Uint32);
            for i in 0..12 { let g = self.ranges[i]; if g.1 > 0 { let mm = &self.meshes[i]; r.draw_indexed(mm.i0..mm.i0 + mm.n, mm.v0, g.0..g.0 + g.1); } }
            for i in 0..NCHUNK {
                let c = &self.chunks[i]; let big = i > 2 && i < 6; if c.n == 0 { continue; }
                r.set_vertex_buffer(1, c.buf.slice(..));
                self.draw_cells(&mut r, &c.start, &c.lo, &c.hi, 3.0, DrawKind::Mesh(14 + i), 0.0, if i > 5 { 30.0 } else if big { 40.0 } else { 96.0 });
                if big { self.draw_cells(&mut r, &c.start, &c.lo, &c.hi, 3.0, DrawKind::Mesh(20 + i), 40.0, 150.0); }
            }
            for i in 0..6 {
                let p = &self.props[i]; if p.n == 0 { continue; }
                let lod = i == 3 || i == 4; r.set_vertex_buffer(1, p.buf.slice(..));
                self.draw_cells(&mut r, &p.start, &p.lo, &p.hi, 3.0, DrawKind::Mesh(4 + i), 0.0, if lod { 6.0 } else { 110.0 });
                if lod { self.draw_cells(&mut r, &p.start, &p.lo, &p.hi, 3.0, DrawKind::Mesh(18 + i), 6.0, 110.0); }
            }
            r.set_pipeline(&self.p_manta); r.set_vertex_buffer(1, self.ib_manta.slice(..)); { let mm = &self.meshes[12]; r.draw_indexed(mm.i0..mm.i0 + mm.n, mm.v0, 0..self.n_manta); }
            for (sp, mid) in [(0usize, 13usize), (1, 26), (2, 27)] {
                let (b, n) = &self.ib_fish[sp]; if *n == 0 { continue; }
                r.set_pipeline(&self.p_fish[sp]); r.set_vertex_buffer(1, b.slice(..)); let mm = &self.meshes[mid]; r.draw_indexed(mm.i0..mm.i0 + mm.n, mm.v0, 0..*n);
            }
            // the shark
            r.set_pipeline(&self.p_shark);
            r.set_vertex_buffer(0, self.vb_shark.slice(..)); r.set_index_buffer(self.ib_shark.slice(..), wgpu::IndexFormat::Uint32);
            for i in 0..n_sharks { r.set_bind_group(1, &self.bg_shark, &[(i * SHARK_SLOT) as u32]); r.draw_indexed(0..self.n_shark, 0, 0..1); }
            // whale, octopuses, tree
            if n_plush > 0 {
                r.set_pipeline(&self.p_plush); r.set_vertex_buffer(0, self.vb_plush.slice(..)); r.set_index_buffer(self.ib_plush.slice(..), wgpu::IndexFormat::Uint32);
                for (i, p) in plush.iter().take(n_plush).enumerate() {
                    let (i0, n) = self.plush_ranges[(p.kind as usize).min(2)];
                    r.set_bind_group(1, &self.bg_plush, &[(i * SHARK_SLOT) as u32]); r.draw_indexed(i0..i0 + n, 0, 0..1);
                }
            }
            // kelp and plants: 9-row blades up close, 5 rows mid-range, a decimated set far away
            if self.big.n > 0 {
                r.set_pipeline(&self.p_leaf); r.set_vertex_buffer(0, self.big.buf.slice(..)); self.draw_cells(&mut r, &self.big.start, &self.big.lo, &self.big.hi, 5.0, DrawKind::Strip(18), 0.0, 24.0);
                r.set_pipeline(&self.p_leaf5); self.draw_cells(&mut r, &self.big.start, &self.big.lo, &self.big.hi, 5.0, DrawKind::Strip(10), 24.0, 58.0);
            }
            if self.big_f.n > 0 { r.set_pipeline(&self.p_leaf_f); r.set_vertex_buffer(0, self.big_f.buf.slice(..)); self.draw_cells(&mut r, &self.big_f.start, &self.big_f.lo, &self.big_f.hi, 5.0, DrawKind::Strip(6), 58.0, 150.0); }
            if self.small.n > 0 { r.set_pipeline(&self.p_leaf_s); r.set_vertex_buffer(0, self.small.buf.slice(..)); self.draw_cells(&mut r, &self.small.start, &self.small.lo, &self.small.hi, 1.5, DrawKind::Strip(6), 0.0, 70.0); }
            r.set_pipeline(&self.p_surf); r.draw(0..4, 0..1);
            r.set_pipeline(&self.p_back); r.draw(0..4, 0..1);
            r.set_pipeline(&self.p_bub); r.set_vertex_buffer(0, self.ib_bub.slice(..)); r.draw(0..4, 0..self.n_bub);
            if self.n_gl > 0 { r.set_pipeline(&self.p_gl); r.set_vertex_buffer(0, self.ib_gl.slice(..)); r.draw(0..4, 0..self.n_gl); }
            r.set_pipeline(&self.p_snow); r.set_vertex_buffer(0, self.ib_snow.slice(..)); r.draw(0..4, 0..self.n_snow);
        }
        // light shafts
        { let mut r = e.begin_render_pass(&wgpu::RenderPassDescriptor { label: None, color_attachments: &[att(&tg.ray, true)], depth_stencil_attachment: None, timestamp_writes: None, occlusion_query_set: None, multiview_mask: None }); r.set_pipeline(&self.p_rays); r.set_bind_group(0, &tg.bg_rays, &[]); r.draw(0..3, 0..1); }
        { let mut r = e.begin_render_pass(&wgpu::RenderPassDescriptor { label: None, color_attachments: &[att(&tg.ray2, true)], depth_stencil_attachment: None, timestamp_writes: None, occlusion_query_set: None, multiview_mask: None }); r.set_pipeline(&self.p_rb); r.set_bind_group(0, &tg.bg_blur, &[]); r.draw(0..3, 0..1); }
        // bloom
        for i in 0..tg.bloom_bg.len() {
            let (target, clear) = if i < 6 { (&tg.bloom[i], true) } else { (&tg.bloom[11 - i - 1], false) };
            let mut r = e.begin_render_pass(&wgpu::RenderPassDescriptor { label: None, color_attachments: &[att(target, clear)], depth_stencil_attachment: None, timestamp_writes: None, occlusion_query_set: None, multiview_mask: None });
            r.set_pipeline(if i == 0 { &self.p_down0 } else if i < 6 { &self.p_down } else { &self.p_up }); r.set_bind_group(0, &tg.bloom_bg[i], &[]); r.draw(0..3, 0..1);
        }
        // composite, then scale to the window
        { let mut r = e.begin_render_pass(&wgpu::RenderPassDescriptor { label: None, color_attachments: &[att(&tg.ldr, true)], depth_stencil_attachment: None, timestamp_writes: None, occlusion_query_set: None, multiview_mask: None }); r.set_pipeline(&self.p_post); r.set_bind_group(0, &tg.bg_post, &[]); r.draw(0..3, 0..1); }
        { let mut r = e.begin_render_pass(&wgpu::RenderPassDescriptor { label: None, color_attachments: &[att(out, true)], depth_stencil_attachment: None, timestamp_writes: None, occlusion_query_set: None, multiview_mask: None }); r.set_pipeline(&self.p_upscale); r.set_bind_group(0, &tg.bg_up, &[]); r.draw(0..3, 0..1); }
        self.queue.submit([e.finish()]);
    }
    pub fn out_format(&self) -> wgpu::TextureFormat { self.out_fmt }
}
enum DrawKind { Terrain, Mesh(usize), Strip(u32) }
fn att(view: &wgpu::TextureView, clear: bool) -> Option<wgpu::RenderPassColorAttachment<'_>> {
    Some(wgpu::RenderPassColorAttachment { view, depth_slice: None, resolve_target: None, ops: wgpu::Operations { load: if clear { wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT) } else { wgpu::LoadOp::Load }, store: wgpu::StoreOp::Store } })
}
fn tv(v: &wgpu::TextureView) -> wgpu::BindingResource<'_> { wgpu::BindingResource::TextureView(v) }
fn e(binding: u32, resource: wgpu::BindingResource<'_>) -> wgpu::BindGroupEntry<'_> { wgpu::BindGroupEntry { binding, resource } }

/// the original's easing along the FLY path: fraction of the path length at fraction t of the flight time
pub fn fly_frac(t: f64) -> f64 { let s = clamp(t, 0.0, 1.0); s * s * (3.0 - 2.0 * s) * 0.25 + s * 0.75 }

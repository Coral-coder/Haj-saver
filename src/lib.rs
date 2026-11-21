//! Hajsaver: Blahaj swims through the Abyssal Garden.
//! One renderer (wgpu, WGSL) behind two front ends: a native Windows screensaver (src/main.rs) and a
//! WebAssembly page for GitHub Pages (src/web.rs).
pub mod creatures;
pub mod director;
pub mod math;
pub mod render;
pub mod shaders;
pub mod shark;
pub mod world;
#[cfg(target_arch = "wasm32")]
mod web;

use director::Director;
use render::{FlyPath, Renderer};
use shark::Shark;
use world::Field;

pub struct App {
    pub r: Renderer, field: Field, path: FlyPath, pub shark: Shark, pub dir: Director, pub time: f64,
    /// 0..1, multiplies the picture (start-up fade)
    pub fade_in: f64,
    /// the little sharks; now and then one of them decides to follow the camera for a while
    pub whale: creatures::Whale, pub octopi: Vec<creatures::Octopus>, trees: Vec<math::V3>,
    pub little: Vec<Shark>, follower: Option<usize>, follow_until: f64, next_follow: f64, last_cam: Option<render::Cam>,
}

impl App {
    /// lv: detail level, 0 High, 1 Medium, 2 Low
    /// season: 0 normal, 1 November (autumn colours), 2 December (Christmas trees); see season_for_month
    pub async fn new(device: wgpu::Device, queue: wgpu::Queue, out_fmt: wgpu::TextureFormat, lv: u32, seed: f64, season: u32, progress: render::Progress<'_>) -> App {
        let mesh = shark::load();
        let (mut r, field) = Renderer::new(device, queue, out_fmt, lv, &mesh, progress).await;
        r.season = season;
        let path = FlyPath::new();
        let shark = Shark::new(&path, &field, 0.02 + 0.9 * math::fract(seed));
        let dir = Director::new(&shark, &field);
        let sizes = [0.5, 0.42, 0.36, 0.33, 0.45, 0.38];
        let little: Vec<Shark> = sizes.iter().enumerate().map(|(i, &z)| Shark::little(&path, &field, math::fract(seed + 0.17 * (i + 1) as f64) * 0.95 + 0.02, z, (seed * 1e5) as u32 + i as u32 * 7919 + 1)).collect();
        let whale = creatures::Whale::new(seed, &field);
        let octopi = [[-60.0, -25.0, 4.0], [10.0, -25.0, 76.0], [6.0, -25.0, -80.0]].iter().enumerate().map(|(i, h)| creatures::Octopus::new(*h, 17 + i as u32 * 31)).collect();
        let trees = if season == 2 { creatures::tree_spots(&field, &path) } else { vec![] };
        App { r, field, path, shark, dir, time: 20.0 + seed * 100.0, fade_in: 1.0, little, whale, octopi, trees, follower: None, follow_until: 0.0, next_follow: 30.0 + 40.0 * seed, last_cam: None }
    }
    pub fn resize(&mut self, w: u32, h: u32) { self.r.resize(w, h); }
    pub fn field(&self) -> &Field { &self.field }
    /// advances the world by dt seconds (simulated in small steps) without drawing
    pub fn advance(&mut self, dt: f64) -> render::Cam {
        let mut left = dt; let mut cam = None;
        while left > 1e-9 || cam.is_none() {
            let s = left.min(1.0 / 30.0); left -= s;
            self.time += s;
            self.whale.step(s, &self.field);
            for o in &mut self.octopi { o.step(s, &self.field); }
            let mut dynamic = self.whale.spheres(); dynamic.extend(self.octopi.iter().map(|o| o.sphere()));
            self.field.set_dynamic(dynamic);
            self.shark.step(s, &self.path, &self.field);
            // the follower: one little shark at a time takes an interest in the camera, then wanders off again
            if self.time > self.next_follow && self.follower.is_none() && !self.little.is_empty() {
                // the nearest one takes an interest; if they are all far off, it arrives from just behind the camera
                let cp = self.last_cam.map(|c| c.pos).unwrap_or(self.shark.pos);
                let i = (0..self.little.len()).min_by(|&a, &b| shark::v3::len(shark::v3::sub(self.little[a].pos, cp)).partial_cmp(&shark::v3::len(shark::v3::sub(self.little[b].pos, cp))).unwrap()).unwrap();
                if let Some(c) = self.last_cam {
                    if shark::v3::len(shark::v3::sub(self.little[i].pos, cp)) > 20.0 {
                        let back = [-c.yaw.sin() * 6.0, -1.0, c.yaw.cos() * 6.0];
                        let p = shark::v3::add(cp, back);
                        if self.field.dist(p[0], p[1], p[2]) < -1.5 && self.field.clear_line(p, cp, 0.2) { self.little[i].pos = p; self.little[i].fwd = shark::v3::norm([c.yaw.sin(), 0.0, -c.yaw.cos()]); }
                    }
                }
                self.follower = Some(i); self.little[i].role = shark::Role::Follow; self.follow_until = self.time + 45.0 + 50.0 * (self.time * 3.7).fract();
            }
            if let Some(i) = self.follower { if self.time > self.follow_until { self.little[i].role = shark::Role::Roam; self.follower = None; self.next_follow = self.time + 40.0 + 60.0 * (self.time * 1.3).fract(); } }
            let (cp, cf) = match self.last_cam { Some(c) => (c.pos, [c.yaw.sin() * c.pitch.cos(), c.pitch.sin(), -c.yaw.cos() * c.pitch.cos()]), None => (self.shark.pos, self.shark.fwd) };
            let all: Vec<math::V3> = std::iter::once(self.shark.pos).chain(self.little.iter().map(|l| l.pos)).collect();
            for (k, l) in self.little.iter_mut().enumerate() {
                l.cam = cp; l.cam_fwd = cf;
                l.others = all.iter().enumerate().filter(|(j, _)| *j != k + 1).map(|(_, p)| *p).collect();
                l.step(s, &self.path, &self.field);
            }
            let c = self.dir.step(s, &self.shark, &self.field);
            // a cut moved the camera: a follower left far behind catches up off screen, just behind the new camera
            if let (Some(i), Some(prev)) = (self.follower, self.last_cam) {
                let l = &mut self.little[i];
                if shark::v3::len(shark::v3::sub(c.pos, prev.pos)) > 3.0 && shark::v3::len(shark::v3::sub(l.pos, c.pos)) > 12.0 {
                    let f = [c.yaw.sin(), 0.0, -c.yaw.cos()];
                    for back in [5.0, 3.5] {
                        let p = [c.pos[0] - f[0] * back, c.pos[1] - 0.8, c.pos[2] - f[2] * back];
                        if self.field.dist(p[0], p[1], p[2]) < -1.2 && self.field.clear_line(p, c.pos, 0.1) { l.pos = p; l.fwd = f; break; }
                    }
                }
            }
            self.last_cam = Some(c); cam = Some(c);
        }
        cam.unwrap()
    }
    /// one frame: dt in seconds since the last one, drawn into `out` (the window's texture)
    pub fn frame(&mut self, dt: f64, out: &wgpu::TextureView) {
        let mut cam = self.advance(dt);
        cam.fade *= self.fade_in;
        let (poses, plush) = self.poses(&cam);
        self.r.render(&cam, self.time, &poses, &plush, out);
    }
    /// everything that moves, for the renderer
    pub fn poses(&self, cam: &render::Cam) -> (Vec<render::SharkPose>, Vec<creatures::PlushPose>) {
        let poses: Vec<render::SharkPose> = std::iter::once(self.shark.pose()).chain(self.little.iter().map(|l| l.pose())).collect();
        let mut plush: Vec<creatures::PlushPose> = vec![self.whale.pose()];
        plush.extend(self.octopi.iter().map(|o| o.pose()));
        // December: only the nearest tree is ever drawn, so there is never more than one on screen
        if let Some(t) = self.trees.iter().min_by(|a, b| shark::v3::len(shark::v3::sub(**a, cam.pos)).partial_cmp(&shark::v3::len(shark::v3::sub(**b, cam.pos))).unwrap()) {
            let a = t[0] * 0.37 + t[2] * 0.11;
            plush.push(creatures::PlushPose { pos: *t, fwd: [a.cos(), 0.0, a.sin()], up: [0.0, 1.0, 0.0], scale: 1.0, kind: 2, a: [a, 0.0, 0.0] });
        }
        (poses, plush)
    }
    /// the December trees' spots
    pub fn trees(&self) -> &[math::V3] { &self.trees }
}

/// 0 normally, 1 in November (autumn), 2 in December (Christmas); month is 1..12
pub fn season_for_month(month: u32) -> u32 { match month { 11 => 1, 12 => 2, _ => 0 } }
/// the current month (1..12, UTC) on native builds; HAJ_MONTH overrides it for testing
#[cfg(not(target_arch = "wasm32"))]
pub fn month_now() -> u32 {
    if let Some(m) = std::env::var("HAJ_MONTH").ok().and_then(|v| v.parse().ok()) { return m; }
    let days = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() / 86400).unwrap_or(0) as i64;
    // civil-from-days (Howard Hinnant)
    let z = days + 719468; let era = z.div_euclid(146097); let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365; let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); let mp = (5 * doy + 2) / 153;
    (if mp < 10 { mp + 3 } else { mp - 9 }) as u32
}

/// lets the browser breathe (repaint the loading text) between load stages; nothing to do natively
pub async fn yield_now() {
    #[cfg(target_arch = "wasm32")]
    {
        let p = js_sys::Promise::new(&mut |res, _| { let _ = web_sys::window().unwrap().set_timeout_with_callback_and_timeout_and_arguments_0(&res, 0); });
        let _ = wasm_bindgen_futures::JsFuture::from(p).await;
    }
}

/// picks a surface format the post shader can write to directly (it applies its own gamma)
pub fn pick_format(formats: &[wgpu::TextureFormat]) -> wgpu::TextureFormat {
    for f in [wgpu::TextureFormat::Bgra8Unorm, wgpu::TextureFormat::Rgba8Unorm] { if formats.contains(&f) { return f; } }
    formats.first().copied().unwrap_or(wgpu::TextureFormat::Bgra8Unorm).remove_srgb_suffix()
}

/// device limits the renderer needs (the world's buffers are big)
pub fn limits(adapter: &wgpu::Adapter) -> wgpu::Limits {
    let a = adapter.limits();
    wgpu::Limits { max_buffer_size: a.max_buffer_size, max_storage_buffer_binding_size: a.max_storage_buffer_binding_size, max_texture_dimension_2d: a.max_texture_dimension_2d, max_texture_dimension_3d: a.max_texture_dimension_3d, ..wgpu::Limits::default() }
}

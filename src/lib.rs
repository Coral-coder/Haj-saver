//! Hajsaver: Blahaj swims through the Abyssal Garden.
//! One renderer (wgpu, WGSL) behind two front ends: a native Windows screensaver (src/main.rs) and a
//! WebAssembly page for GitHub Pages (src/web.rs).
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

pub struct App { pub r: Renderer, field: Field, path: FlyPath, pub shark: Shark, pub dir: Director, pub time: f64, /// 0..1, multiplies the picture (start-up fade)
    pub fade_in: f64 }

impl App {
    /// lv: detail level, 0 High, 1 Medium, 2 Low
    pub async fn new(device: wgpu::Device, queue: wgpu::Queue, out_fmt: wgpu::TextureFormat, lv: u32, seed: f64, progress: render::Progress<'_>) -> App {
        let mesh = shark::load();
        let (r, field) = Renderer::new(device, queue, out_fmt, lv, &mesh, progress).await;
        let path = FlyPath::new();
        let shark = Shark::new(&path, &field, 0.02 + 0.9 * math::fract(seed));
        let dir = Director::new(&shark, &field);
        App { r, field, path, shark, dir, time: 20.0 + seed * 100.0, fade_in: 1.0 }
    }
    pub fn resize(&mut self, w: u32, h: u32) { self.r.resize(w, h); }
    pub fn field(&self) -> &Field { &self.field }
    /// advances the world by dt seconds (simulated in small steps) without drawing
    pub fn advance(&mut self, dt: f64) -> render::Cam {
        let mut left = dt; let mut cam = None;
        while left > 1e-9 || cam.is_none() {
            let s = left.min(1.0 / 30.0); left -= s;
            self.time += s;
            self.shark.step(s, &self.path, &self.field);
            cam = Some(self.dir.step(s, &self.shark, &self.field));
        }
        cam.unwrap()
    }
    /// one frame: dt in seconds since the last one, drawn into `out` (the window's texture)
    pub fn frame(&mut self, dt: f64, out: &wgpu::TextureView) {
        let mut cam = self.advance(dt);
        cam.fade *= self.fade_in;
        let pose = self.shark.pose();
        self.r.render(&cam, self.time, &pose, out);
    }
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

// The GitHub Pages front end: the same renderer compiled to WebAssembly, drawing with the browser's WebGPU.
use std::cell::RefCell;
use std::rc::Rc;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;

// what the page asks for (mouse, touch, keys, buttons), queued until the next frame
enum Cmd { Look(f64, f64), Zoom(f64), Explore(bool), Feed }
thread_local! {
    static CMDS: RefCell<Vec<Cmd>> = const { RefCell::new(Vec::new()) };
    static MOVE: std::cell::Cell<[f64; 4]> = const { std::cell::Cell::new([0.0; 4]) };
}
/// drag: dx, dy in CSS pixels (orbits the shot, or turns your head while exploring)
#[wasm_bindgen] pub fn look(dx: f64, dy: f64) { CMDS.with(|c| c.borrow_mut().push(Cmd::Look(dx, dy))); }
/// wheel / pinch: factor > 1 moves further out
#[wasm_bindgen] pub fn zoom(f: f64) { CMDS.with(|c| c.borrow_mut().push(Cmd::Zoom(f))); }
/// free swimming on or off
#[wasm_bindgen] pub fn explore(on: bool) { CMDS.with(|c| c.borrow_mut().push(Cmd::Explore(on))); }
/// drop food in front of the camera
#[wasm_bindgen] pub fn feed() { CMDS.with(|c| c.borrow_mut().push(Cmd::Feed)); }
/// while exploring: x right, y up, z forward (-1..1), fast 0/1
#[wasm_bindgen] pub fn set_move(x: f64, y: f64, z: f64, fast: f64) { MOVE.with(|m| m.set([x, y, z, fast])); }

fn status(msg: &str) {
    if let Some(e) = web_sys::window().and_then(|w| w.document()).and_then(|d| d.get_element_by_id("status")) { e.set_text_content(Some(msg)); }
}

/// Starts the screensaver on the canvas with id `canvas_id`. level: 0 High, 1 Medium, 2 Low.
#[wasm_bindgen]
/// month: 1..12 (the page passes the local month, or ?month= to try another)
#[allow(clippy::too_many_arguments)]
pub async fn run(canvas_id: String, level: u32, month: u32) -> Result<(), JsValue> {
    console_error_panic_hook::set_once();
    let win = web_sys::window().ok_or("no window")?;
    let canvas: web_sys::HtmlCanvasElement = win.document().ok_or("no document")?.get_element_by_id(&canvas_id).ok_or("no canvas")?.dyn_into()?;
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor { backends: wgpu::Backends::BROWSER_WEBGPU, ..wgpu::InstanceDescriptor::new_without_display_handle() });
    let surface = instance.create_surface(wgpu::SurfaceTarget::Canvas(canvas.clone())).map_err(|e| JsValue::from_str(&format!("{e}")))?;
    let adapter = instance.request_adapter(&wgpu::RequestAdapterOptions { power_preference: wgpu::PowerPreference::HighPerformance, compatible_surface: Some(&surface), ..Default::default() }).await
        .map_err(|_| JsValue::from_str("No WebGPU adapter found. Try a current Chrome or Edge."))?;
    let (device, queue) = adapter.request_device(&wgpu::DeviceDescriptor { label: None, required_features: wgpu::Features::empty(), required_limits: crate::limits(&adapter), ..Default::default() }).await
        .map_err(|e| JsValue::from_str(&format!("{e}")))?;
    let caps = surface.get_capabilities(&adapter);
    let fmt = crate::pick_format(&caps.formats);
    let seed = js_sys::Math::random();
    let app = crate::App::new(device.clone(), queue.clone(), fmt, level, seed, crate::season_for_month(month), &mut |m, f| status(&format!("{m}… {:.0}%", f * 100.0))).await;
    status("");
    let size = |c: &web_sys::HtmlCanvasElement| {
        // the scene is drawn at its own (tuned) resolution and scaled up: past 1.25 the canvas only costs bandwidth
        let dpr = web_sys::window().unwrap().device_pixel_ratio().min(1.25);
        (((c.client_width() as f64 * dpr).round() as u32).max(2), ((c.client_height() as f64 * dpr).round() as u32).max(2))
    };
    let mut cfg = wgpu::SurfaceConfiguration { usage: wgpu::TextureUsages::RENDER_ATTACHMENT, format: fmt, width: 2, height: 2, present_mode: wgpu::PresentMode::Fifo, alpha_mode: wgpu::CompositeAlphaMode::Opaque, view_formats: vec![], desired_maximum_frame_latency: 2, color_space: Default::default() };
    let perf = win.performance().ok_or("no performance")?;
    let t0 = perf.now();
    struct St { app: crate::App, cfg: wgpu::SurfaceConfiguration, last: f64, slow: f64 }
    let st = Rc::new(RefCell::new(St { app, cfg: { cfg.width = 2; cfg }, last: t0, slow: 0.0 }));
    let f: Rc<RefCell<Option<Closure<dyn FnMut(f64)>>>> = Rc::new(RefCell::new(None));
    let g = f.clone();
    *g.borrow_mut() = Some(Closure::new(move |now: f64| {
        {
            let mut s = st.borrow_mut(); let s = &mut *s;
            let (w, h) = size(&canvas);
            if w != s.cfg.width || h != s.cfg.height {
                s.cfg.width = w; s.cfg.height = h; canvas.set_width(w); canvas.set_height(h);
                surface.configure(&device, &s.cfg); s.app.resize(w, h);
            }
            for c in CMDS.with(|c| std::mem::take(&mut *c.borrow_mut())) {
                match c { Cmd::Look(x, y) => s.app.look(x, y), Cmd::Zoom(f) => s.app.zoom(f), Cmd::Explore(on) => s.app.set_explore(on), Cmd::Feed => s.app.feed() }
            }
            let m = MOVE.with(|m| m.get()); s.app.user.mv = [m[0], m[1], m[2]]; s.app.user.fast = m[3] > 0.5;
            let dt = ((now - s.last) / 1000.0).clamp(0.0, 0.1); let dtr = now - s.last; s.last = now;
            if let wgpu::CurrentSurfaceTexture::Success(frame) | wgpu::CurrentSurfaceTexture::Suboptimal(frame) = surface.get_current_texture() {
                let view = frame.texture.create_view(&Default::default());
                s.app.fade_in = ((now - t0) / 2000.0).min(1.0);
                s.app.frame(dt, &view);
                queue.present(frame);
                s.app.r.tune(dtr);
                // still too slow at the lowest resolution: remember a lower detail level and reload with it
                if s.app.r.struggling() && dtr > 28.0 { s.slow += dtr; } else { s.slow = (s.slow - dtr * 0.5).max(0.0); }
                if s.slow > 4000.0 && s.app.r.level() < 2 && now - t0 > 8000.0 {
                    let lv = s.app.r.level() + 1;
                    if let Some(w) = web_sys::window() {
                        if let Ok(Some(ls)) = w.local_storage() { let _ = ls.set_item("hajsaverQ", &lv.to_string()); }
                        let _ = w.location().set_search(&format!("?q={lv}"));
                    }
                    s.slow = -1e12;
                }
            }
        }
        web_sys::window().unwrap().request_animation_frame(f.borrow().as_ref().unwrap().as_ref().unchecked_ref()).unwrap();
    }));
    win.request_animation_frame(g.borrow().as_ref().unwrap().as_ref().unchecked_ref())?;
    Ok(())
}

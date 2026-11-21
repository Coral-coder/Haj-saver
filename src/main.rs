// Native front end. On Windows this is the screensaver itself (rename hajsaver.exe to Hajsaver.scr):
//   /s run full screen, /p <hwnd> preview in the Screen Saver Settings box, /c settings.
// Elsewhere it renders frames off screen into PNG files (used to check the look without a Windows machine):
//   hajsaver --out dir --size 1280x720 --start 5 --frames 6 --every 2.5 --level 0
#![cfg_attr(windows, windows_subsystem = "windows")]
#[cfg(windows)]
mod win;

#[cfg(windows)]
fn main() { win::main(); }

#[cfg(not(windows))]
fn main() {
    let args: Vec<String> = std::env::args().collect();
    let get = |k: &str, d: &str| -> String { args.iter().position(|a| a == k).and_then(|i| args.get(i + 1)).cloned().unwrap_or_else(|| d.to_string()) };
    let out = get("--out", "shots"); let size = get("--size", "1280x720");
    let (w, h) = { let mut it = size.split('x').map(|v| v.parse::<u32>().unwrap()); (it.next().unwrap(), it.next().unwrap()) };
    let start: f64 = get("--start", "5").parse().unwrap(); let frames: usize = get("--frames", "4").parse().unwrap(); let every: f64 = get("--every", "3").parse().unwrap();
    let lv: u32 = get("--level", "1").parse().unwrap(); let seed: f64 = get("--seed", "0.25").parse().unwrap();
    std::fs::create_dir_all(&out).unwrap();
    pollster::block_on(headless(&out, w, h, start, frames, every, lv, seed));
}

#[cfg(not(windows))]
#[allow(clippy::too_many_arguments)]
async fn headless(out: &str, w: u32, h: u32, start: f64, frames: usize, every: f64, lv: u32, seed: f64) {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor { backends: wgpu::Backends::VULKAN, ..wgpu::InstanceDescriptor::new_without_display_handle() });
    let adapter = instance.request_adapter(&wgpu::RequestAdapterOptions { power_preference: wgpu::PowerPreference::HighPerformance, ..Default::default() }).await.expect("no adapter");
    eprintln!("adapter: {:?}", adapter.get_info().name);
    let (device, queue) = adapter.request_device(&wgpu::DeviceDescriptor { label: None, required_features: wgpu::Features::empty(), required_limits: hajsaver::limits(&adapter), ..Default::default() }).await.expect("device");
    device.on_uncaptured_error(std::sync::Arc::new(|e| panic!("GPU error: {e}")));
    let fmt = wgpu::TextureFormat::Rgba8Unorm;
    let t0 = std::time::Instant::now();
    let mut app = hajsaver::App::new(device, queue, fmt, lv, seed, &mut |m, f| eprintln!("[{:5.1}s] {m} ({:.0}%)", t0.elapsed().as_secs_f64(), f * 100.0)).await;
    eprintln!("world ready in {:.1}s", t0.elapsed().as_secs_f64());
    if let Some(i) = std::env::args().position(|a| a == "--sim") {
        // run the shark and the cameras for a long time and report how they did
        let secs: f64 = std::env::args().nth(i + 1).unwrap().parse().unwrap();
        let (mut t, mut worst, mut in_rock, mut cam_rock, mut hidden, mut far) = (0.0, -9.0f64, 0.0, 0.0, 0.0, 0.0);
        let mut shots: std::collections::BTreeMap<String, (u32, f64)> = Default::default(); let mut last = String::new();
        let mut minp = [1e9f64; 3]; let mut maxp = [-1e9f64; 3]; let mut slow = 0.0;
        while t < secs {
            let cam = app.advance(1.0 / 30.0); t += 1.0 / 30.0;
            let s = &app.shark; let f = app.field();
            let d = f.dist(s.pos[0], s.pos[1], s.pos[2]); worst = worst.max(d); if d > -0.5 { in_rock += 1.0 / 30.0; }
            if f.dist(cam.pos[0], cam.pos[1], cam.pos[2]) > -0.2 { cam_rock += 1.0 / 30.0; }
            if !f.clear_line(cam.pos, s.pos, 0.0) { hidden += 1.0 / 30.0; }
            let dd = hajsaver::shark::v3::len(hajsaver::shark::v3::sub(s.pos, cam.pos)); if dd > 25.0 { far += 1.0 / 30.0; }
            if s.speed < 0.9 { slow += 1.0 / 30.0; }
            for k in 0..3 { minp[k] = minp[k].min(s.pos[k]); maxp[k] = maxp[k].max(s.pos[k]); }
            let name = format!("{:?}", app.dir.shot);
            if name != last { let e = shots.entry(name.clone()).or_default(); e.0 += 1; last = name.clone(); }
            shots.get_mut(&name).unwrap().1 += 1.0 / 30.0;
            if (t * 30.0).round() as i64 % (30 * 60) == 0 { eprintln!("t={:.0} shark=({:.1},{:.1},{:.1}) v={:.2}", t, s.pos[0], s.pos[1], s.pos[2], s.speed); }
        }
        eprintln!("worst shark rock distance {worst:.2} m; near/in rock {in_rock:.1} s; slow {slow:.1} s; camera in rock {cam_rock:.1} s; shark hidden {hidden:.1} s; far {far:.1} s");
        eprintln!("shark range x {:.0}..{:.0} y {:.0}..{:.0} z {:.0}..{:.0}", minp[0], maxp[0], minp[1], maxp[1], minp[2], maxp[2]);
        for (k, v) in shots { eprintln!("  {k:8} {:4} shots, {:6.0} s, avg {:.1} s", v.0, v.1, v.1 / v.0 as f64); }
        return;
    }
    app.r.pix = (w * h) as f64;
    app.resize(w, h);
    let target = app.r.device.create_texture(&wgpu::TextureDescriptor { label: None, size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 }, mip_level_count: 1, sample_count: 1, dimension: wgpu::TextureDimension::D2, format: fmt, usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC, view_formats: &[] });
    let view = target.create_view(&Default::default());
    let bpr = (w * 4).div_ceil(256) * 256;
    let rb = app.r.device.create_buffer(&wgpu::BufferDescriptor { label: None, size: (bpr * h) as u64, usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
    app.advance(start);
    for i in 0..frames {
        if i > 0 { app.advance(every); }
        if args_has("--mouth") {
            // debug close-up of the head, slightly from below, to check the teeth
            let sk = app.shark.pose(); let f = sk.fwd; let u = sk.up;
            let head = [sk.pos[0] + f[0] * 0.95, sk.pos[1] + f[1] * 0.95 - 0.12, sk.pos[2] + f[2] * 0.95];
            let p = [head[0] + f[0] * 1.3 - u[0] * 0.35, head[1] + f[1] * 1.3 - u[1] * 0.35, head[2] + f[2] * 1.3 - u[2] * 0.35];
            let d = [head[0] - p[0], head[1] - p[1], head[2] - p[2]];
            let cam = hajsaver::render::Cam { pos: p, yaw: d[0].atan2(-d[2]), pitch: d[1].atan2((d[0] * d[0] + d[2] * d[2]).sqrt()), tanh: 0.35, fade: 1.0 };
            app.r.render(&cam, app.time, &sk, &view);
        } else {
            app.frame(1.0 / 60.0, &view);
        }
        let mut e = app.r.device.create_command_encoder(&Default::default());
        e.copy_texture_to_buffer(wgpu::TexelCopyTextureInfo { texture: &target, mip_level: 0, origin: Default::default(), aspect: Default::default() },
            wgpu::TexelCopyBufferInfo { buffer: &rb, layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(bpr), rows_per_image: None } }, wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 });
        app.r.queue.submit([e.finish()]);
        rb.slice(..).map_async(wgpu::MapMode::Read, |r| r.unwrap());
        app.r.device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
        let data = rb.slice(..).get_mapped_range().unwrap();
        let mut img = image::RgbaImage::new(w, h);
        for y in 0..h { for x in 0..w { let o = (y * bpr + x * 4) as usize; img.put_pixel(x, y, image::Rgba([data[o], data[o + 1], data[o + 2], 255])); } }
        drop(data); rb.unmap();
        let p = format!("{out}/shot{i:02}.png"); img.save(&p).unwrap();
        let s = &app.shark;
        eprintln!("{p}: t={:.1} shot={:?} shark=({:.1},{:.1},{:.1}) speed {:.2}", app.time, app.dir.shot, s.pos[0], s.pos[1], s.pos[2], s.speed);
    }
}

#[cfg(not(windows))]
fn args_has(k: &str) -> bool { std::env::args().any(|a| a == k) }

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
    let mut app = hajsaver::App::new(device, queue, fmt, lv, seed, hajsaver::season_for_month(hajsaver::month_now()), &mut |m, f| eprintln!("[{:5.1}s] {m} ({:.0}%)", t0.elapsed().as_secs_f64(), f * 100.0)).await;
    eprintln!("world ready in {:.1}s", t0.elapsed().as_secs_f64());
    if let Some(i) = std::env::args().position(|a| a == "--sim") {
        // run the shark and the cameras for a long time and report how they did
        let secs: f64 = std::env::args().nth(i + 1).unwrap().parse().unwrap();
        let (mut t, mut worst, mut in_rock, mut cam_rock, mut hidden, mut far) = (0.0, -9.0f64, 0.0, 0.0, 0.0, 0.0);
        let mut shots: std::collections::BTreeMap<String, (u32, f64)> = Default::default(); let mut last = String::new();
        let (mut lworst, mut ftime, mut fdist) = (-9.0f64, 0.0f64, 0.0f64); let mut minp = [1e9f64; 3]; let mut maxp = [-1e9f64; 3]; let mut slow = 0.0;
        while t < secs {
            let cam = app.advance(1.0 / 30.0); t += 1.0 / 30.0;
            let s = &app.shark; let f = app.field();
            let d = s.body_spheres().iter().map(|(k, r)| f.dist(k[0], k[1], k[2]) + r).fold(-9.0f64, f64::max); worst = worst.max(d); if d > 0.05 { in_rock += 1.0 / 30.0; }
            if f.dist(cam.pos[0], cam.pos[1], cam.pos[2]) > -0.2 { cam_rock += 1.0 / 30.0; }
            if !f.clear_line(cam.pos, s.pos, 0.0) { hidden += 1.0 / 30.0; }
            let dd = hajsaver::shark::v3::len(hajsaver::shark::v3::sub(s.pos, cam.pos)); if dd > 25.0 { far += 1.0 / 30.0; }
            if s.speed < 0.9 { slow += 1.0 / 30.0; }
            for l in &app.little {
                let d = l.body_spheres().iter().map(|(k, r)| f.dist(k[0], k[1], k[2]) + r).fold(-9.0f64, f64::max); lworst = lworst.max(d);
                if l.role == hajsaver::shark::Role::Follow { ftime += 1.0 / 30.0; fdist += hajsaver::shark::v3::len(hajsaver::shark::v3::sub(l.pos, cam.pos)) / 30.0; }
            }
            for k in 0..3 { minp[k] = minp[k].min(s.pos[k]); maxp[k] = maxp[k].max(s.pos[k]); }
            let name = format!("{:?}", app.dir.shot);
            if name != last { let e = shots.entry(name.clone()).or_default(); e.0 += 1; last = name.clone(); }
            shots.get_mut(&name).unwrap().1 += 1.0 / 30.0;
            if (t * 30.0).round() as i64 % (30 * 60) == 0 { eprintln!("t={:.0} shark=({:.1},{:.1},{:.1}) v={:.2}", t, s.pos[0], s.pos[1], s.pos[2], s.speed); }
        }
        eprintln!("worst body penetration {worst:.2} m; touching (>5 cm) for {in_rock:.1} s; slow {slow:.1} s; camera in rock {cam_rock:.1} s; shark hidden {hidden:.1} s; far {far:.1} s");
        eprintln!("little sharks: worst penetration {lworst:.2} m; following the camera {ftime:.0} s at {:.1} m on average", fdist / ftime.max(1e-9));
        for l in &app.little { eprintln!("  little {:.2}: at ({:.0},{:.0},{:.0})", l.size, l.pos[0], l.pos[1], l.pos[2]); }
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
        if let Ok(what) = std::env::var("HAJ_LOOK") {
            // debug: look at one of the animals (whale, octopus, tree, little) from a few metres away
            let (t, d): (hajsaver::math::V3, f64) = match what.as_str() {
                "whale" => (app.whale.pos, 30.0), "reef" => ([-57.0, -38.0, 0.5], 1.6), "school" => ([13.0, -36.0, -10.0], 1.6), "octopus" => (app.octopi[0].pos, 3.2), "little" => (app.little[0].pos, 2.6),
                sp @ ("fish0" | "fish1" | "fish2") => {
                    // the shader's swimming maths for that fish (vsFish in shaders/anim.wgsl)
                    let f = app.r.fish_probe[(sp.as_bytes()[4] - b'0') as usize].map(|v| v as f64);
                    let hs = |x: f64| ((x * 127.1).sin() * 43758.5453).fract().abs();
                    let (s, r, t) = (f[4], f[3], app.time); let h = [hs(s), hs(s + 0.37), hs(s + 0.71)];
                    let ta = t * f[5] + f[7] * 6.3 + (h[0] - 0.5) * (26.0 / r); let ph = f[7] * 17.0;
                    let p = [f[0] + r * ta.cos(), f[1] + 2.2 * (3.0 * ta + ph).sin(), f[2] + r * 0.6 * ta.sin()];
                    let tv = [-r * ta.sin() * f[5].signum(), 0.0, r * 0.6 * ta.cos() * f[5].signum()]; let tl = (tv[0] * tv[0] + tv[2] * tv[2]).sqrt();
                    let side = [-tv[2] / tl, 0.0, tv[0] / tl];
                    let off = (h[1] - 0.5) * 5.0 + (t * 0.6 + s * 40.0).sin() * 0.5; let offu = (h[2] - 0.5) * 3.2 + (t * 0.5 + s * 31.0).cos() * 0.4;
                    ([p[0] + side[0] * off, p[1] + offu, p[2] + side[2] * off], 0.9 * f[6] / 0.3)
                }
                _ => { let t = app.trees().first().copied().unwrap_or(app.shark.pos); ([t[0], t[1] + 2.4, t[2]], 9.0) }
            };
            let a = 0.7 + i as f64 * 0.9;
            let mut p = [t[0] + a.cos() * d, t[1] + d * 0.15, t[2] + a.sin() * d];
            if what == "whale" { let f = app.whale.fwd; let k = if i == 0 { -1.0 } else { 0.35 }; p = [t[0] + f[0] * d * k - f[2] * d * 0.5, t[1] + 4.0 - 6.0 * i as f64, t[2] + f[2] * d * k + f[0] * d * 0.5]; eprintln!("cam dist to rock {:.1}, line clear {}", app.field().dist_static(p[0], p[1], p[2]), app.field().clear_line(p, t, 0.0)); }
            let dd = [t[0] - p[0], t[1] - p[1], t[2] - p[2]];
            let cam = hajsaver::render::Cam { pos: p, yaw: dd[0].atan2(-dd[2]), pitch: dd[1].atan2((dd[0] * dd[0] + dd[2] * dd[2]).sqrt()), tanh: 0.5, fade: 1.0 };
            let (sp, pp) = app.poses(&cam);
            eprintln!("look {what}: target {:?} trees {} whale {:?} whale dist {:.1}", t, app.trees().len(), app.whale.pos, app.field().dist(app.whale.pos[0], app.whale.pos[1], app.whale.pos[2]));
            app.r.render(&cam, app.time, &sp, &pp, &view);
        } else if args_has("--mouth") {
            // debug close-up of the head, slightly from below, to check the teeth
            let sk = app.shark.pose(); let f = sk.fwd; let u = sk.up;
            let head = [sk.pos[0] + f[0] * 0.95, sk.pos[1] + f[1] * 0.95 - 0.12, sk.pos[2] + f[2] * 0.95];
            // HAJ_MOUTH=front|side|level picks the angle
            let l = [u[1] * f[2] - u[2] * f[1], u[2] * f[0] - u[0] * f[2], u[0] * f[1] - u[1] * f[0]];
            let (a, b, c) = match std::env::var("HAJ_MOUTH").as_deref() { Ok("side") => (0.5, -0.15, 1.2), Ok("level") => (1.4, 0.0, 0.0), _ => (1.3, -0.35, 0.0) };
            let p = [head[0] + f[0] * a + u[0] * b + l[0] * c, head[1] + f[1] * a + u[1] * b + l[1] * c, head[2] + f[2] * a + u[2] * b + l[2] * c];
            let d = [head[0] - p[0], head[1] - p[1], head[2] - p[2]];
            let cam = hajsaver::render::Cam { pos: p, yaw: d[0].atan2(-d[2]), pitch: d[1].atan2((d[0] * d[0] + d[2] * d[2]).sqrt()), tanh: 0.35, fade: 1.0 };
            app.r.render(&cam, app.time, &[sk], &[], &view);
        } else {
            app.frame(1.0 / 60.0, &view);
        }
        let mut e = app.r.device.create_command_encoder(&Default::default());
        e.copy_texture_to_buffer(wgpu::TexelCopyTextureInfo { texture: &target, mip_level: 0, origin: Default::default(), aspect: Default::default() },
            wgpu::TexelCopyBufferInfo { buffer: &rb, layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(bpr), rows_per_image: None } }, wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 });
        let tf = std::time::Instant::now();
        app.r.queue.submit([e.finish()]);
        rb.slice(..).map_async(wgpu::MapMode::Read, |r| r.unwrap());
        app.r.device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
        let data = rb.slice(..).get_mapped_range().unwrap();
        let mut img = image::RgbaImage::new(w, h);
        for y in 0..h { for x in 0..w { let o = (y * bpr + x * 4) as usize; img.put_pixel(x, y, image::Rgba([data[o], data[o + 1], data[o + 2], 255])); } }
        drop(data); rb.unmap();
        let p = format!("{out}/shot{i:02}.png"); img.save(&p).unwrap();
        let s = &app.shark;
        eprintln!("{p} ({:.0} ms): t={:.1} shot={:?} shark=({:.1},{:.1},{:.1}) speed {:.2}", tf.elapsed().as_secs_f64() * 1000.0, app.time, app.dir.shot, s.pos[0], s.pos[1], s.pos[2], s.speed);
    }
}

#[cfg(not(windows))]
fn args_has(k: &str) -> bool { std::env::args().any(|a| a == k) }

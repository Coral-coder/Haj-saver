// The Windows screensaver: plain Win32 + wgpu (Direct3D 12 or Vulkan), no browser anywhere.
//   Hajsaver.scr /s          full screen on every monitor until the mouse moves or a key is pressed
//   Hajsaver.scr /p <hwnd>   the little preview in Screen Saver Settings
//   Hajsaver.scr /c          settings (detail level), also what a double-click on the file opens
use std::ffi::c_void;
use std::num::NonZeroIsize;
use std::sync::mpsc;
use std::time::Instant;
use windows_sys::Win32::Foundation::*;
use windows_sys::Win32::Graphics::Gdi::*;
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::Registry::*;
use windows_sys::Win32::UI::HiDpi::*;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::SetFocus;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

fn w(s: &str) -> Vec<u16> { s.encode_utf16().chain(std::iter::once(0)).collect() }

#[derive(PartialEq, Clone, Copy)]
enum Mode { Full, Preview(HWND), Config }

fn parse() -> Mode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let a = args.first().map(|s| s.to_ascii_lowercase()).unwrap_or_default();
    let num = |s: &str| s.trim().parse::<usize>().ok();
    if a.starts_with("/s") || a.starts_with("-s") { return Mode::Full; }
    if a.starts_with("/p") || a.starts_with("-p") {
        let h = if a.len() > 3 { num(&a[3..]) } else { args.get(1).and_then(|s| num(s)) };
        if let Some(h) = h { return Mode::Preview(h as HWND); }
    }
    Mode::Config
}

// ---------------------------------------------------------------- settings (HKCU\Software\Hajsaver\Detail)
const KEY: &str = "Software\\Hajsaver";
fn load_level() -> u32 {
    unsafe {
        let mut k: HKEY = std::ptr::null_mut();
        if RegOpenKeyExW(HKEY_CURRENT_USER, w(KEY).as_ptr(), 0, KEY_READ, &mut k) != 0 { return 0; }
        let (mut v, mut n, mut t) = (0u32, 4u32, 0u32);
        let r = RegQueryValueExW(k, w("Detail").as_ptr(), std::ptr::null(), &mut t, &mut v as *mut u32 as *mut u8, &mut n);
        RegCloseKey(k);
        if r == 0 && v <= 2 { v } else { 0 }
    }
}
fn save_level(v: u32) {
    unsafe {
        let mut k: HKEY = std::ptr::null_mut();
        if RegCreateKeyExW(HKEY_CURRENT_USER, w(KEY).as_ptr(), 0, std::ptr::null(), 0, KEY_WRITE, std::ptr::null(), &mut k, std::ptr::null_mut()) == 0 {
            RegSetValueExW(k, w("Detail").as_ptr(), 0, REG_DWORD, &v as *const u32 as *const u8, 4);
            RegCloseKey(k);
        }
    }
}
fn config() {
    let cur = ["High", "Medium", "Low"][load_level() as usize];
    let text = format!("Hajsaver: Blåhaj swims through the Abyssal Garden.\n\nDetail level (now: {cur}):\n\n  Yes  →  High (the full scene; a recent graphics card)\n  No  →  Medium (half the plants, smaller shadows)\n  Cancel  →  Low (laptops and older graphics)\n\nThe picture also scales its resolution by itself to stay smooth.\n\nModel: \"Blahaj\" by Kaine_G (CC BY 4.0). World: Abyssal Garden by Maksymilian Dendura (MIT).");
    let r = unsafe { MessageBoxW(std::ptr::null_mut(), w(&text).as_ptr(), w("Hajsaver settings").as_ptr(), MB_YESNOCANCEL | MB_ICONINFORMATION | MB_DEFBUTTON1) };
    save_level(match r { IDYES => 0, IDNO => 1, _ => 2 });
}

// ---------------------------------------------------------------- window
static mut MODE_FULL: bool = false;
static mut START_MOUSE: (i32, i32, bool) = (0, 0, false);
static mut QUIT: bool = false;

unsafe extern "system" fn wndproc(h: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    let full = MODE_FULL;
    match msg {
        WM_DESTROY => { QUIT = true; PostQuitMessage(0); 0 }
        WM_SETCURSOR if full => { SetCursor(std::ptr::null_mut()); 1 }
        WM_ERASEBKGND => 1,
        WM_ACTIVATEAPP if full && wp == 0 => { DestroyWindow(h); 0 }
        WM_KEYDOWN | WM_SYSKEYDOWN | WM_LBUTTONDOWN | WM_RBUTTONDOWN | WM_MBUTTONDOWN | WM_XBUTTONDOWN | WM_MOUSEWHEEL if full => { DestroyWindow(h); 0 }
        WM_MOUSEMOVE if full => {
            let (x, y) = ((lp & 0xffff) as i16 as i32, ((lp >> 16) & 0xffff) as i16 as i32);
            let (sx, sy, set) = START_MOUSE;
            if !set { START_MOUSE = (x, y, true); } else if (x - sx).abs() > 6 || (y - sy).abs() > 6 { DestroyWindow(h); }
            0
        }
        WM_SYSCOMMAND if full && (wp as u32 == SC_SCREENSAVE || wp as u32 == SC_CLOSE) => 0,
        _ => DefWindowProcW(h, msg, wp, lp),
    }
}

pub fn main() {
    let mode = parse();
    if mode == Mode::Config { config(); return; }
    unsafe {
        SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        MODE_FULL = mode == Mode::Full;
        // a second copy (Windows sometimes starts one while another runs) just leaves
        let inst = GetModuleHandleW(std::ptr::null());
        let class = w("HajsaverWindow");
        let wc = WNDCLASSW { style: CS_HREDRAW | CS_VREDRAW, lpfnWndProc: Some(wndproc), cbClsExtra: 0, cbWndExtra: 0, hInstance: inst, hIcon: std::ptr::null_mut(), hCursor: LoadCursorW(std::ptr::null_mut(), IDC_ARROW), hbrBackground: GetStockObject(BLACK_BRUSH) as HBRUSH, lpszMenuName: std::ptr::null(), lpszClassName: class.as_ptr() };
        RegisterClassW(&wc);
        let (hwnd, wdt, hgt) = match mode {
            Mode::Preview(parent) => {
                let mut r: RECT = std::mem::zeroed(); GetClientRect(parent, &mut r);
                let h = CreateWindowExW(0, class.as_ptr(), w("Hajsaver").as_ptr(), WS_CHILD | WS_VISIBLE, 0, 0, r.right - r.left, r.bottom - r.top, parent, std::ptr::null_mut(), inst, std::ptr::null());
                (h, (r.right - r.left).max(1) as u32, (r.bottom - r.top).max(1) as u32)
            }
            _ => {
                let (x, y) = (GetSystemMetrics(SM_XVIRTUALSCREEN), GetSystemMetrics(SM_YVIRTUALSCREEN));
                let (cw, ch) = (GetSystemMetrics(SM_CXVIRTUALSCREEN), GetSystemMetrics(SM_CYVIRTUALSCREEN));
                let h = CreateWindowExW(WS_EX_TOPMOST | WS_EX_TOOLWINDOW, class.as_ptr(), w("Hajsaver").as_ptr(), WS_POPUP | WS_VISIBLE, x, y, cw, ch, std::ptr::null_mut(), std::ptr::null_mut(), inst, std::ptr::null());
                ShowCursor(0); SetForegroundWindow(h); SetFocus(h);
                (h, cw.max(1) as u32, ch.max(1) as u32)
            }
        };
        if hwnd.is_null() { return; }
        run(hwnd, wdt, hgt, mode);
    }
}

fn pump() -> bool {
    unsafe {
        let mut m: MSG = std::mem::zeroed();
        while PeekMessageW(&mut m, std::ptr::null_mut(), 0, 0, PM_REMOVE) != 0 {
            if m.message == WM_QUIT { return false; }
            TranslateMessage(&m); DispatchMessageW(&m);
        }
        !QUIT
    }
}

fn run(hwnd: HWND, mut wdt: u32, mut hgt: u32, mode: Mode) {
    let preview = matches!(mode, Mode::Preview(_));
    // Vulkan first (it compiles the shaders quickly), Direct3D 12 when there is no Vulkan driver
    let mut found = None;
    for backends in [wgpu::Backends::VULKAN, wgpu::Backends::DX12] {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor { backends, ..wgpu::InstanceDescriptor::new_without_display_handle() });
        let target = wgpu::SurfaceTargetUnsafe::RawHandle {
            raw_display_handle: Some(wgpu::rwh::RawDisplayHandle::Windows(wgpu::rwh::WindowsDisplayHandle::new())),
            raw_window_handle: wgpu::rwh::RawWindowHandle::Win32(wgpu::rwh::Win32WindowHandle::new(NonZeroIsize::new(hwnd as isize).unwrap())),
        };
        let Ok(surface) = (unsafe { instance.create_surface_unsafe(target) }) else { continue };
        if let Ok(a) = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions { power_preference: wgpu::PowerPreference::HighPerformance, compatible_surface: Some(&surface), ..Default::default() })) {
            found = Some((instance, surface, a)); break;
        }
    }
    let Some((_instance, surface, adapter)) = found else { fail(hwnd, preview, "Hajsaver needs a graphics card with Vulkan or Direct3D 12."); return; };
    let (device, queue) = match pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor { label: None, required_features: wgpu::Features::empty(), required_limits: hajsaver::limits(&adapter), ..Default::default() })) {
        Ok(d) => d,
        Err(e) => { fail(hwnd, preview, &format!("Hajsaver could not start the graphics device: {e}")); return; }
    };
    let caps = surface.get_capabilities(&adapter);
    let fmt = hajsaver::pick_format(&caps.formats);
    let mut cfg = wgpu::SurfaceConfiguration { usage: wgpu::TextureUsages::RENDER_ATTACHMENT, format: fmt, width: wdt, height: hgt, present_mode: wgpu::PresentMode::Fifo, alpha_mode: caps.alpha_modes[0], view_formats: vec![], desired_maximum_frame_latency: 2, color_space: Default::default() };
    surface.configure(&device, &cfg);
    let clear = |surface: &wgpu::Surface, device: &wgpu::Device, queue: &wgpu::Queue| {
        if let wgpu::CurrentSurfaceTexture::Success(f) | wgpu::CurrentSurfaceTexture::Suboptimal(f) = surface.get_current_texture() {
            let v = f.texture.create_view(&Default::default());
            let mut e = device.create_command_encoder(&Default::default());
            e.begin_render_pass(&wgpu::RenderPassDescriptor { label: None, color_attachments: &[Some(wgpu::RenderPassColorAttachment { view: &v, depth_slice: None, resolve_target: None, ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::BLACK), store: wgpu::StoreOp::Store } })], depth_stencil_attachment: None, timestamp_writes: None, occlusion_query_set: None, multiview_mask: None });
            queue.submit([e.finish()]); queue.present(f);
        }
    };
    clear(&surface, &device, &queue);

    // build the world on a worker thread while this one keeps the window responsive
    let lv = if preview { 2 } else { load_level() };
    let seed = (std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0) % 100_000) as f64 / 100_000.0;
    let (tx, rx) = mpsc::channel();
    let dq = (device.clone(), queue.clone());
    std::thread::spawn(move || {
        let app = pollster::block_on(hajsaver::App::new(dq.0, dq.1, fmt, lv, seed, hajsaver::season_for_month(hajsaver::month_now()), &mut |_, _| {}));
        let _ = tx.send(app);
    });
    let mut app = loop {
        if !pump() { return; }
        match rx.try_recv() {
            Ok(a) => break a,
            Err(mpsc::TryRecvError::Empty) => { clear(&surface, &device, &queue); std::thread::sleep(std::time::Duration::from_millis(30)); }
            Err(_) => return,
        }
    };
    if preview { app.r.pix = (wdt * hgt) as f64; }
    app.resize(wdt, hgt);
    let start = Instant::now(); let mut last = start;
    while pump() {
        unsafe {
            let mut r: RECT = std::mem::zeroed(); GetClientRect(hwnd, &mut r);
            let (nw, nh) = ((r.right - r.left).max(1) as u32, (r.bottom - r.top).max(1) as u32);
            if nw != wdt || nh != hgt { wdt = nw; hgt = nh; cfg.width = nw; cfg.height = nh; surface.configure(&device, &cfg); app.resize(nw, nh); }
        }
        let frame = match surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(f) => f,
            wgpu::CurrentSurfaceTexture::Suboptimal(f) => { drop(f); surface.configure(&device, &cfg); continue; }
            wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => { surface.configure(&device, &cfg); continue; }
            _ => { std::thread::sleep(std::time::Duration::from_millis(16)); continue; }
        };
        let now = Instant::now(); let dt = (now - last).as_secs_f64(); last = now;
        let view = frame.texture.create_view(&Default::default());
        // fade in over the first two seconds
        app.fade_in = ((now - start).as_secs_f64() / 2.0).min(1.0);
        app.frame(dt.min(0.1), &view);
        app.r.queue.present(frame);
        if !preview { app.r.tune(dt * 1000.0); }
    }
}

fn fail(hwnd: HWND, preview: bool, msg: &str) {
    if !preview { unsafe { ShowCursor(1); MessageBoxW(hwnd, w(msg).as_ptr(), w("Hajsaver").as_ptr(), MB_OK | MB_ICONERROR); } }
    let _ = hwnd as *mut c_void;
}

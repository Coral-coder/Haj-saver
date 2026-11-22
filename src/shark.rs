// Blahaj: the model, and how he swims.
// Model: "Blahaj" by Kaine_G (https://sketchfab.com/Kaine_G), CC BY 4.0, via the Coral-coder/Blahaj repo
// (https://sketchfab.com/3d-models/blahaj-ce981de49111488c81ea646067abe1ec). The mesh has no skeleton: the fins are
// subdivided here so the vertex shader (shaders/shark.wgsl) can bend them smoothly.
use crate::math::*;
use crate::render::{FlyPath, SharkPose};
use crate::world::Field;

const GLB: &[u8] = include_bytes!("../assets/blahaj.glb");
const COLOR_JPG: &[u8] = include_bytes!("../assets/blahaj_color.jpg");
const ROUGH_JPG: &[u8] = include_bytes!("../assets/blahaj_rough.jpg");

/// interleaved vertices (pos3, normal3, uv2, part) + indices + RGBA8 textures
pub struct SharkMesh { pub verts: Vec<f32>, pub idx: Vec<u32>, pub color: (u32, u32, Vec<u8>), pub rough: (u32, u32, Vec<u8>) }

fn accessor(json: &serde_json::Value, bin: &[u8], i: usize) -> (Vec<f32>, usize) {
    let a = &json["accessors"][i]; let bv = &json["bufferViews"][a["bufferView"].as_u64().unwrap() as usize];
    let comps = match a["type"].as_str().unwrap() { "SCALAR" => 1, "VEC2" => 2, "VEC3" => 3, "VEC4" => 4, t => panic!("accessor type {t}") };
    let ct = a["componentType"].as_u64().unwrap(); let csize = match ct { 5126 | 5125 => 4, 5123 => 2, 5121 => 1, _ => panic!("component type {ct}") };
    let count = a["count"].as_u64().unwrap() as usize;
    let base = bv["byteOffset"].as_u64().unwrap_or(0) as usize + a["byteOffset"].as_u64().unwrap_or(0) as usize;
    let stride = bv["byteStride"].as_u64().map(|s| s as usize).unwrap_or(comps * csize);
    let mut out = Vec::with_capacity(count * comps);
    for k in 0..count { for c in 0..comps {
        let o = base + k * stride + c * csize; let b = &bin[o..o + csize];
        out.push(match ct { 5126 => f32::from_le_bytes([b[0], b[1], b[2], b[3]]), 5125 => u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as f32, 5123 => u16::from_le_bytes([b[0], b[1]]) as f32, _ => b[0] as f32 });
    } }
    (out, comps)
}
fn decode(jpg: &[u8]) -> (u32, u32, Vec<u8>) {
    let img = image::load_from_memory_with_format(jpg, image::ImageFormat::Jpeg).expect("texture").to_rgba8();
    (img.width(), img.height(), img.into_raw())
}
/// the fin regions of the mesh (mesh space: snout -x, dorsal +y, pectoral fins +-z)
fn is_fin(x: f32, y: f32, z: f32) -> bool { (x > -1.6 && x < 0.7 && y > 0.85) || (x > 2.1 && y > 0.4) || (z.abs() > 0.75 && y < 0.8 && x > -1.7 && x < 1.0) }

/// The teeth are flaps hanging straight down from the mouth. Keep each one's wide end (high u) where it meets the
/// body and swing its tip toward the other row, so the two rows point at each other like the plush's.
fn swing_teeth(p: &mut [f32], uv: &[f32], ix: &[f32]) {
    // group the vertices into teeth (each tooth is two triangles sharing an edge)
    let n = p.len() / 3; let mut grp: Vec<usize> = (0..n).collect();
    fn root(g: &mut Vec<usize>, mut i: usize) -> usize { while g[i] != i { g[i] = g[g[i]]; i = g[i]; } i }
    for t in ix.chunks(3) { let a = root(&mut grp, t[0] as usize); for &k in &t[1..] { let b = root(&mut grp, k as usize); grp[b] = a; } }
    let mut teeth: std::collections::BTreeMap<usize, Vec<usize>> = Default::default();
    for i in 0..n { let r = root(&mut grp, i); teeth.entry(r).or_default().push(i); }
    let lower = |i: usize| uv[i * 2] > 0.0555;
    let v = |p: &[f32], i: usize| [p[i * 3] as f64, p[i * 3 + 1] as f64, p[i * 3 + 2] as f64];
    let ttl: Vec<Vec<usize>> = teeth.into_values().collect();
    // base edge: the two vertices with the highest u; tip: the rest
    let split = |t: &Vec<usize>| { let mut s = t.clone(); s.sort_by(|&a, &b| uv[b * 2].partial_cmp(&uv[a * 2]).unwrap()); (s[..2].to_vec(), s[2..].to_vec()) };
    let mid = |p: &[f32], s: &[usize]| { let mut m = [0.0; 3]; for &i in s { let q = v(p, i); for k in 0..3 { m[k] += q[k] / s.len() as f64; } } m };
    let orig = p.to_vec();
    for t in &ttl {
        let (base, tip) = split(t); let lw = lower(t[0]);
        let a = v(&orig, base[0]); let b = v(&orig, base[1]); let bm = mid(&orig, &base);
        let axis = norm(sub(b, a));
        let rot = |q: V3, ang: f64| { // rotate q about the base edge
            let r = sub(q, a); let (c, s) = (ang.cos(), ang.sin()); let d = dot(axis, r);
            add(a, add(add(mul(r, c), mul(cross(axis, r), s)), mul(axis, d * (1.0 - c))))
        };
        // the rows are two arcs around the mouth: the top row on the outside, the bottom row inside it. Top-row
        // tips lean in toward the mouth's middle, bottom-row tips lean out toward the top row.
        let centre = [-2.45, bm[1], 0.0];
        let inward = norm([centre[0] - bm[0], 0.0, centre[2] - bm[2]]);
        let want = if lw { mul(inward, -1.0) } else { inward };
        let tm = mid(&orig, &tip);
        let gain = |ang: f64| dot(sub(rot(tm, ang), tm), want);
        let sw = if lw { 0.5 } else { 0.4 }; let ang = if gain(sw) > gain(-sw) { sw } else { -sw };
        for &i in &tip { let q = rot(v(&orig, i), ang); for k in 0..3 { p[i * 3 + k] = q[k] as f32; } }
    }
}

pub fn load() -> SharkMesh {
    let jl = u32::from_le_bytes(GLB[12..16].try_into().unwrap()) as usize;
    let json: serde_json::Value = serde_json::from_slice(&GLB[20..20 + jl]).expect("glb json");
    let bin = &GLB[20 + jl + 8..];
    let mut verts: Vec<f32> = vec![];
    for (mesh, part) in [(0usize, 0.0f32), (1, 1.0)] {
        let pr = &json["meshes"][mesh]["primitives"][0];
        let ga = |n: &str| pr["attributes"][n].as_u64().unwrap() as usize;
        let (p, _) = accessor(&json, bin, ga("POSITION")); let (n, _) = accessor(&json, bin, ga("NORMAL")); let (uv, _) = accessor(&json, bin, ga("TEXCOORD_0"));
        let (ix, _) = accessor(&json, bin, pr["indices"].as_u64().unwrap() as usize);
        // triangle soup: 9 floats per corner
        let mut p = p;
        if part == 1.0 { swing_teeth(&mut p, &uv, &ix); }
        let mut tri: Vec<[f32; 9]> = ix.iter().map(|&i| { let i = i as usize; [p[i * 3], p[i * 3 + 1], p[i * 3 + 2], n[i * 3], n[i * 3 + 1], n[i * 3 + 2], uv[i * 2], uv[i * 2 + 1], part] }).collect();
        if part == 0.0 {
            for _ in 0..2 {
                let mut out = Vec::with_capacity(tri.len() * 2);
                for t in tri.chunks(3) {
                    let c = [(t[0][0] + t[1][0] + t[2][0]) / 3.0, (t[0][1] + t[1][1] + t[2][1]) / 3.0, (t[0][2] + t[1][2] + t[2][2]) / 3.0];
                    if !is_fin(c[0], c[1], c[2]) { out.extend_from_slice(t); continue; }
                    let mid = |a: &[f32; 9], b: &[f32; 9]| { let mut m = [0f32; 9]; for k in 0..9 { m[k] = (a[k] + b[k]) * 0.5; } m };
                    let (a, b, cc) = (t[0], t[1], t[2]); let (ab, bc, ca) = (mid(&a, &b), mid(&b, &cc), mid(&cc, &a));
                    out.extend_from_slice(&[a, ab, ca, ab, b, bc, ca, bc, cc, ab, bc, ca]);
                }
                tri = out;
            }
        }
        for v in tri { verts.extend_from_slice(&v); }
    }
    let idx = (0..(verts.len() / 9) as u32).collect();
    SharkMesh { verts, idx, color: decode(COLOR_JPG), rough: decode(ROUGH_JPG) }
}

// ---------------------------------------------------------------- swimming
/// mesh units -> metres: 7.37 mesh units long -> about 2.8 m
pub const SCALE: f64 = 0.38;
pub const LENGTH: f64 = 7.37 * SCALE;

pub fn add(a: V3, b: V3) -> V3 { [a[0] + b[0], a[1] + b[1], a[2] + b[2]] }
pub fn sub(a: V3, b: V3) -> V3 { [a[0] - b[0], a[1] - b[1], a[2] - b[2]] }
pub fn mul(a: V3, s: f64) -> V3 { [a[0] * s, a[1] * s, a[2] * s] }
pub fn dot(a: V3, b: V3) -> f64 { a[0] * b[0] + a[1] * b[1] + a[2] * b[2] }
pub fn cross(a: V3, b: V3) -> V3 { [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]] }
pub fn len(a: V3) -> f64 { dot(a, a).sqrt() }
pub fn norm(a: V3) -> V3 { let l = len(a); if l < 1e-9 { [0.0, 0.0, 1.0] } else { mul(a, 1.0 / l) } }
pub mod v3 { pub use super::{add, cross, dot, len, mul, norm, sub}; }

/// a slow smooth noise in [-1, 1]
fn wob(t: f64, s: f64) -> f64 { (t * 0.13 + s).sin() * 0.6 + (t * 0.071 + s * 2.3).sin() * 0.4 }

pub struct Shark {
    pub pos: V3, pub fwd: V3, pub up: V3, pub speed: f64,
    /// where he is heading along the route (fraction of the path length) and which way
    u: f64, dir: f64,
    target_speed: f64, burst: f64, next_burst: f64, t: f64,
    yaw_rate: f64, pitch_rate: f64, bank: f64, phase: f64,
    flap_l: f64, flap_r: f64, fl_v: [f64; 2], dorsal: f64, dorsal_v: f64,
    /// a wanted detour target (pos) that overrides the route for a while
    detour: Option<(V3, f64)>, ring_a: f64, escape: f64, flipped: f64,
    /// 1 for the big shark; the little ones are 0.33..0.5
    pub size: f64,
    pub role: Role,
    /// the little ones' wander target and when to pick a new one
    goal: V3, goal_until: f64, rng: u32,
    /// set by the App: where the camera is (for the follower) and where the others are (to keep apart)
    pub cam: V3, pub cam_fwd: V3, pub others: Vec<V3>,
    /// the nearest food pellet, set by the App while there is food in the water: everyone goes for it
    pub food: Option<V3>,
    /// where he was over the last few seconds (every half second), to tell when he is really stuck
    hist: std::collections::VecDeque<V3>, hist_t: f64,
    /// the way out he picked when he got stuck, and a lure he gave up on (food he could not reach) until a time
    escape_dir: V3, pub food_block: Option<(V3, f64)>, chase_at: V3, chase_t: f64,
    /// how many times he got stuck (for the --sim statistics)
    pub stuck_events: u32,
}
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Role { Route, Roam, Follow }

/// places worth a closer look: the ship, the whale fluke, the grotto, the kelp basin, the sponge canyon
pub const SIGHTS: [[f64; 3]; 6] = [[55.0, -27.0, 6.0], [-5.0, -34.0, -4.0], [6.0, -34.0, 82.0], [4.0, -30.0, -86.0], [-62.0, -36.0, 2.0], [-40.0, -40.0, 44.0]];

impl Shark {
    pub fn new(path: &FlyPath, field: &Field, start: f64) -> Shark {
        let p = path.at(start); let q = path.at(start + 0.01);
        let mut pos = [p[0], p[1] + 1.5, p[2]];
        // make sure he starts in open water
        for _ in 0..40 { if field.dist(pos[0], pos[1], pos[2]) < -1.2 { break; } pos[1] += 0.5; }
        let fwd = norm([q[0] - p[0], 0.0, q[2] - p[2]]);
        Shark { pos, fwd, up: [0.0, 1.0, 0.0], speed: 1.6, u: start, dir: 1.0, target_speed: 1.6, burst: 0.0, next_burst: 25.0, t: 0.0,
            yaw_rate: 0.0, pitch_rate: 0.0, bank: 0.0, phase: 0.0, flap_l: 0.0, flap_r: 0.0, fl_v: [0.0; 2], dorsal: 0.0, dorsal_v: 0.0, detour: None, ring_a: 0.0, escape: 0.0, flipped: -99.0,
            size: 1.0, role: Role::Route, goal: pos, goal_until: 0.0, rng: 1, cam: pos, cam_fwd: [0.0, 0.0, 1.0], others: vec![], food: None, hist: Default::default(), hist_t: 0.0, escape_dir: [0.0, 0.0, 1.0], food_block: None, chase_at: [0.0; 3], chase_t: 0.0, stuck_events: 0 }
    }
    /// a little free-roaming shark (size 0.33..0.5) starting near p
    pub fn little(path: &FlyPath, field: &Field, start: f64, size: f64, seed: u32) -> Shark {
        let mut s = Shark::new(path, field, start);
        s.size = size; s.role = Role::Roam; s.rng = seed.wrapping_mul(2654435761) | 1; s.speed = 1.2;
        s.phase = seed as f64; s.pos[1] += 3.0 + (seed % 3) as f64;
        for _ in 0..40 { if field.dist(s.pos[0], s.pos[1], s.pos[2]) < -1.0 { break; } s.pos[1] += 0.5; }
        s.next_burst = 15.0 + (seed % 7) as f64 * 6.0;
        s
    }
    fn rand(&mut self) -> f64 { let mut x = self.rng; x ^= x << 13; x ^= x >> 17; x ^= x << 5; self.rng = x; x as f64 / 4294967296.0 }
    pub fn t(&self) -> f64 { self.t }
    pub fn length(&self) -> f64 { LENGTH * self.size }
    /// the little ones: somewhere new in open water every so often, often near one of the sights
    fn roam_target(&mut self, field: &Field) -> V3 {
        if self.t > self.goal_until || len(sub(self.goal, self.pos)) < 4.0 {
            for _ in 0..30 {
                let g = if self.rand() < 0.4 { let s = SIGHTS[(self.rand() * SIGHTS.len() as f64) as usize]; [s[0] + (self.rand() - 0.5) * 24.0, s[1] + self.rand() * 8.0, s[2] + (self.rand() - 0.5) * 24.0] }
                    else { let a = self.rand() * TAU; let r = 20.0 + self.rand() * 30.0; [self.pos[0] + a.cos() * r, -36.0 + self.rand() * 22.0, self.pos[2] + a.sin() * r] };
                if hypot2(g[0], g[2]) < 125.0 && field.dist(g[0], g[1], g[2]) < -3.0 && field.clear_line(self.pos, g, 0.5) { self.goal = g; break; }
            }
            self.goal_until = self.t + 20.0 + self.rand() * 30.0;
        }
        self.goal
    }
    /// the follower: keeps company a few metres in front of the camera, drifting from side to side
    fn follow_target(&self) -> V3 {
        let f = self.cam_fwd; let side = norm(cross([0.0, 1.0, 0.0], f));
        add(add(self.cam, mul(f, 4.5)), add(mul(side, 2.2 * (self.t * 0.23).sin()), [0.0, -0.4 + 0.8 * (self.t * 0.17).sin(), 0.0]))
    }
    /// where the route wants him to be: a point ahead on the path, wandering a little around it
    fn route_target(&mut self, path: &FlyPath) -> V3 {
        let look = 10.0 / path.len;
        // advance the route marker while he keeps up with it
        let p = path.at(self.u); let dpos = sub([p[0], p[1], p[2]], self.pos);
        if len([dpos[0], 0.0, dpos[2]]) < 9.0 { self.u += self.dir * 3.0 / path.len; }
        if self.u > 0.995 { self.u = 0.995; self.dir = -1.0; }
        if self.u < 0.005 { self.u = 0.005; self.dir = 1.0; }
        let a = path.at(self.u + self.dir * look);
        let side = norm(cross([0.0, 1.0, 0.0], [a[3] - a[0], 0.0, a[5] - a[2]]));
        let off = wob(self.t, 1.7) * 3.0;
        [a[0] + side[0] * off, a[1] + 2.0 + wob(self.t, 4.1) * 2.5, a[2] + side[2] * off]
    }
    pub fn step(&mut self, dt: f64, path: &FlyPath, field: &Field) {
        let sz = self.size;
        let dt = dt.min(0.05);
        self.t += dt;
        // pace: an easy cruise that drifts, and now and then a burst of speed
        if self.t > self.next_burst { self.burst = 2.6; self.next_burst = self.t + 30.0 + 40.0 * (0.5 + 0.5 * wob(self.t * 3.0, 9.0)); }
        self.burst = (self.burst - dt).max(0.0);
        self.target_speed = (1.5 + 0.5 * wob(self.t, 0.3) + if self.burst > 0.0 { 2.4 } else { 0.0 }) * (0.55 + 0.45 * sz);
        if let Some(f) = self.food {
            // food! a quick dash, slowing to snap it up
            let d = len(sub(f, self.pos));
            self.target_speed = clamp(0.8 + d * 0.5, 0.8, 5.5) * (0.6 + 0.4 * sz);
        } else if self.role == Role::Follow {
            // keep up with the camera: faster when far behind, easy when alongside
            let d = len(sub(self.follow_target(), self.pos));
            self.target_speed = clamp(0.6 + d * 0.6, 0.6, 6.0);
        }
        self.speed += (self.target_speed - self.speed) * (1.0 - (-dt * if self.burst > 0.0 { 1.6 } else { 0.5 }).exp());

        // detours to the sights near the route
        if self.role == Role::Route && self.detour.is_none() && self.escape <= 0.0 && wob(self.t * 2.0, 5.5) > 0.82 {
            for s in SIGHTS { let d = len(sub(s, self.pos)); if d < 30.0 && d > 8.0 { self.detour = Some((s, self.t + 16.0)); self.ring_a = (self.pos[2] - s[2]).atan2(self.pos[0] - s[0]); break; } }
        }
        let mut target = match self.role { Role::Route => self.route_target(path), Role::Roam => self.roam_target(field), Role::Follow => self.follow_target() };
        if let Some(f) = self.food { target = f; self.detour = None; }
        if let Some((s, until)) = self.detour {
            // circle the sight a few metres out, on a ring that stays in open water
            self.ring_a += dt * 0.3;
            let mut found = None;
            'ring: for r in [8.0, 11.0, 14.0] { for dy in [1.0, 4.0, 7.0] {
                let p = [s[0] + self.ring_a.cos() * r, s[1] + dy, s[2] + self.ring_a.sin() * r];
                if field.dist(p[0], p[1], p[2]) < -2.5 { found = Some(p); break 'ring; }
            } }
            match found { Some(p) => target = p, None => self.detour = None }
            if self.t > until { self.detour = None; }
        }
        // Stuck: hardly any progress over the last few seconds, whatever he is trying to do. Stop, look for the
        // most open way out, turn around into it, and give up on whatever drew him in.
        self.hist_t += dt;
        if self.hist_t >= 0.5 { self.hist_t = 0.0; self.hist.push_back(self.pos); if self.hist.len() > 8 { self.hist.pop_front(); } }
        if self.escape <= 0.0 && self.hist.len() == 8 && len(sub(self.pos, self.hist[0])) < 1.3 * sz.max(0.5) {
            self.stuck_events += 1; self.hist.clear();
            self.escape_dir = self.way_out(field);
            if std::env::var("HAJ_TRACE").is_ok() { eprintln!("STUCK size {:.2} t={:.0} pos=({:.1},{:.1},{:.1}) role {:?} food {} out=({:.2},{:.2},{:.2}) dist {:.2}", self.size, self.t, self.pos[0], self.pos[1], self.pos[2], self.role, self.food.is_some(), self.escape_dir[0], self.escape_dir[1], self.escape_dir[2], field.dist(self.pos[0], self.pos[1], self.pos[2])); }
            self.escape = 5.0; self.detour = None;
            if let Some(f) = self.food { self.food_block = Some((f, self.t + 25.0)); }
            if self.role == Role::Route && self.t > self.flipped + 15.0 { self.dir = -self.dir; self.flipped = self.t; }
            self.goal = add(self.pos, mul(self.escape_dir, 18.0)); self.goal_until = self.t + 12.0;
        }
        if self.escape > 0.0 {
            self.escape -= dt;
            target = add(self.pos, mul(self.escape_dir, 10.0));
            // turn on the spot first (slowly, so the turn is tight), then swim out
            self.target_speed = if self.escape > 3.5 { 0.6 } else { 1.6 } * (0.6 + 0.4 * sz);
        }
        let mut want = norm(sub(target, self.pos));
        // obstacle avoidance: feel ahead with the smooth distance field and turn away from the rock; kelp stems
        // are soft obstacles he weaves between
        let mut push = [0.0; 3]; let mut danger: f64 = 0.0;
        let l = norm(cross(self.up, self.fwd));
        for (k, w) in [(1.5, 1.0), (3.5, 0.8), (6.0, 0.6), (9.0, 0.35)] {
            for side in [-1.0, 0.0, 1.0] {
                let pr = add(add(self.pos, mul(self.fwd, k * (0.4 + 0.6 * sz) * (1.0 + self.speed * 0.25))), mul(l, side * 0.9 * sz));
                let d = field.dist(pr[0], pr[1], pr[2]);
                let m = 2.4 * (0.5 + 0.5 * sz);
                if d > -m {
                    let s = (d + m) / m; danger = danger.max(s * w);
                    let g = field.grad(pr[0], pr[1], pr[2]);
                    push = add(push, mul(g, -s * w * 2.2));
                }
            }
            if k < 7.0 {
                let pr = add(self.pos, mul(self.fwd, k));
                field.kelp_near(pr[0], pr[2], 1.6, |c| {
                    if pr[1] > c[2] as f64 && pr[1] < c[3] as f64 + 0.5 {
                        let away = norm([pr[0] - c[0] as f64, 0.0, pr[2] - c[1] as f64]);
                        let d = hypot2(pr[0] - c[0] as f64, pr[2] - c[1] as f64);
                        push = add(push, mul(away, (1.6 - d) / 1.6 * w * 0.9));
                    }
                });
            }
        }
        // keep a little apart from the other sharks
        for o in &self.others {
            let d = sub(self.pos, *o); let l = len(d);
            if l < 3.5 && l > 1e-3 { push = add(push, mul(d, (3.5 - l) / (3.5 * l) * 1.5)); }
        }
        // stay off the surface and inside the world
        if self.pos[1] > -5.0 { push[1] -= (self.pos[1] + 5.0) * 0.4; }
        let r = hypot2(self.pos[0], self.pos[2]); if r > 130.0 { push = add(push, mul(norm([-self.pos[0], 0.0, -self.pos[2]]), (r - 130.0) * 0.1)); }
        want = norm(add(want, push));
        if danger > 0.7 { self.speed *= 1.0 - dt * 0.6; }
        // turning round to get out: go for the way out he picked, and keep moving
        if self.escape > 0.0 { want = self.escape_dir; danger = 0.0; self.speed = self.speed.max(0.7 * (0.6 + 0.4 * sz)); }
        want[1] = clamp(want[1], -0.5, 0.5); let want = norm(want);

        // turn toward it with a limited, smoothed turn rate (yaw about world up, pitch about the side axis)
        let fwd = self.fwd;
        let yaw_err = { let a = fwd[2].atan2(fwd[0]); let b = want[2].atan2(want[0]); let mut d = b - a; while d > PI { d -= TAU; } while d < -PI { d += TAU; } d };
        let pitch_err = want[1].asin() - fwd[1].asin();
        let max_yaw = if self.escape > 0.0 { 1.6 } else { 0.55 + 0.5 * danger };
        // a target behind him: commit to one turning direction instead of dithering between the two
        let yaw_err = if yaw_err.abs() > 2.4 && self.yaw_rate.abs() > 0.05 { yaw_err.abs() * self.yaw_rate.signum() } else { yaw_err };
        let yr = clamp(yaw_err * 1.6, -max_yaw, max_yaw); let pr = clamp(pitch_err * 1.4, -0.35, 0.35);
        let k = 1.0 - (-dt * 2.5).exp();
        self.yaw_rate += (yr - self.yaw_rate) * k; self.pitch_rate += (pr - self.pitch_rate) * k;
        let a = fwd[2].atan2(fwd[0]) + self.yaw_rate * dt; let el = clamp(fwd[1].asin() + self.pitch_rate * dt, -0.5, 0.5);
        self.fwd = [a.cos() * el.cos(), el.sin(), a.sin() * el.cos()];
        // bank into turns
        self.bank += (clamp(self.yaw_rate * 0.55, -0.5, 0.5) - self.bank) * (1.0 - (-dt * 2.0).exp());
        let flat_up = norm(sub([0.0, 1.0, 0.0], mul(self.fwd, self.fwd[1])));
        let side = cross(self.fwd, flat_up);
        self.up = norm(add(mul(flat_up, self.bank.cos()), mul(side, self.bank.sin())));

        // move, then make sure he never ends up inside the rock
        let m = mul(self.fwd, self.speed * dt);
        let mut v = mul(self.fwd, self.speed);
        field.collide(&mut self.pos, m, &mut v, 0.55 * sz);
        // the whole body: a chain of spheres from nose to tail (and out to the pectoral fins), each pushed out of
        // whatever it touches; the push moves the shark, and he loses the speed that went into the obstacle
        for _ in 0..3 {
            let mut push = [0.0; 3]; let mut hit = false;
            for (k, w) in self.body_spheres() {
                let d = field.dist(k[0], k[1], k[2]) + w;
                if d > 0.0 { let g = field.grad(k[0], k[1], k[2]); push = add(push, mul(g, -d)); hit = true; }
            }
            if !hit { break; }
            self.pos = add(self.pos, mul(push, 0.6));
            let into = -dot(norm(push), self.fwd);
            if into > 0.0 { self.speed *= 1.0 - 0.5 * into; }
        }

        // still wedged in (between kelp stems, under a ledge) while turning round: slide out along the way out,
        // a little at a time, wherever the whole body fits
        if self.escape > 0.0 && self.escape < 3.5 && self.hist.len() >= 3 && len(sub(self.pos, self.hist[self.hist.len() - 3])) < 0.4 * sz.max(0.5) {
            let step = mul(self.escape_dir, 0.8 * dt);
            let save = self.pos; self.pos = add(self.pos, step);
            if self.body_spheres().iter().any(|(k, r)| field.dist(k[0], k[1], k[2]) + r > 0.02) { self.pos = save; }
        }
        // food he cannot reach: give up on that pellet after a while
        if let Some(f) = self.food {
            if len(sub(f, self.chase_at)) > 1.0 { self.chase_at = f; self.chase_t = 0.0; }
            self.chase_t += dt;
            if self.chase_t > 12.0 { self.food_block = Some((f, self.t + 25.0)); self.chase_t = 0.0; }
        } else { self.chase_t = 0.0; }

        // animation: tail beat follows the speed, fins steer and balance
        let freq = (0.45 + 0.38 * self.speed) / sz.sqrt();
        self.phase += dt * TAU * freq;
        let fl_t = [0.11 * (self.phase * 0.5).sin() + self.yaw_rate * 0.35 + self.pitch_rate * 0.6, 0.11 * (self.phase * 0.5 + 0.4).sin() - self.yaw_rate * 0.35 + self.pitch_rate * 0.6];
        for (i, f) in [&mut self.flap_l, &mut self.flap_r].into_iter().enumerate() {
            // loose under-damped springs
            self.fl_v[i] += ((fl_t[i] - *f) * 40.0 - self.fl_v[i] * 5.0) * dt; *f += self.fl_v[i] * dt;
        }
        let dt_ = -self.yaw_rate * 1.2;
        self.dorsal_v += ((dt_ - self.dorsal) * 30.0 - self.dorsal_v * 4.0) * dt; self.dorsal += self.dorsal_v * dt;
    }
    pub fn pose(&self) -> SharkPose {
        let amp = 0.34 + 0.08 * self.speed.min(4.0);
        let tail = (1.15 * 3.6 - self.phase).sin() * amp; // lateral tail position
        SharkPose { pos: self.pos, fwd: self.fwd, up: self.up, scale: SCALE * self.size,
            a: [self.phase, amp, clamp(-self.yaw_rate * 1.1, -0.9, 0.9), clamp(self.pitch_rate * 0.8, -0.5, 0.5)],
            b: [self.flap_l, self.flap_r, self.dorsal, -tail * 1.6] }
    }
    /// collision spheres (centre, radius) along his body and at the pectoral fins
    pub fn body_spheres(&self) -> [(V3, f64); 7] {
        let z = self.size; let f = self.fwd; let l = norm(cross(self.up, f)); let p = self.pos; let at = |a: f64, s: f64| add(add(p, mul(f, LENGTH * z * a)), mul(l, s * z));
        [(at(0.40, 0.0), 0.28 * z), (at(0.22, 0.0), 0.42 * z), (at(0.0, 0.0), 0.48 * z), (at(-0.2, 0.0), 0.38 * z), (at(-0.4, 0.0), 0.25 * z), (at(0.08, 0.62), 0.16 * z), (at(0.08, -0.62), 0.16 * z)]
    }
    /// the most open direction from here: march out along 24 headings (level and a little up or down) and pick the
    /// one with the longest clear run, preferring to turn round rather than push on the way he was going
    fn way_out(&self, field: &Field) -> V3 {
        let clear = 0.6 * self.size + 0.35; let mut best = (-1.0, mul(self.fwd, -1.0));
        for k in 0..16 { for el in [0.0f64, 0.35, -0.25] {
            let a = k as f64 / 16.0 * TAU; let d = norm([a.cos() * el.cos(), el.sin(), a.sin() * el.cos()]);
            let mut run = 0.0;
            while run < 16.0 { let p = add(self.pos, mul(d, run + 0.75)); if field.dist(p[0], p[1], p[2]) > -clear || p[1] > -2.0 { break; } run += 0.75; }
            let score = run - 2.0 * dot(d, self.fwd).max(0.0);
            if score > best.0 { best = (score, d); }
        } }
        best.1
    }
    /// where the mouth is (for eating)
    pub fn mouth(&self) -> V3 { add(self.pos, mul(self.fwd, self.length() * 0.42)) }
    pub fn vel(&self) -> V3 { mul(self.fwd, self.speed) }
}

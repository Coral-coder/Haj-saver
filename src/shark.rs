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
    detour: Option<(V3, f64)>, ring_a: f64, stuck: f64, escape: f64, flipped: f64,
}

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
            yaw_rate: 0.0, pitch_rate: 0.0, bank: 0.0, phase: 0.0, flap_l: 0.0, flap_r: 0.0, fl_v: [0.0; 2], dorsal: 0.0, dorsal_v: 0.0, detour: None, ring_a: 0.0, stuck: 0.0, escape: 0.0, flipped: -99.0 }
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
        let dt = dt.min(0.05);
        self.t += dt;
        // pace: an easy cruise that drifts, and now and then a burst of speed
        if self.t > self.next_burst { self.burst = 2.6; self.next_burst = self.t + 30.0 + 40.0 * (0.5 + 0.5 * wob(self.t * 3.0, 9.0)); }
        self.burst = (self.burst - dt).max(0.0);
        self.target_speed = 1.5 + 0.5 * wob(self.t, 0.3) + if self.burst > 0.0 { 2.4 } else { 0.0 };
        self.speed += (self.target_speed - self.speed) * (1.0 - (-dt * if self.burst > 0.0 { 1.6 } else { 0.5 }).exp());

        // detours to the sights near the route
        if self.detour.is_none() && self.escape <= 0.0 && wob(self.t * 2.0, 5.5) > 0.82 {
            for s in SIGHTS { let d = len(sub(s, self.pos)); if d < 30.0 && d > 8.0 { self.detour = Some((s, self.t + 16.0)); self.ring_a = (self.pos[2] - s[2]).atan2(self.pos[0] - s[0]); break; } }
        }
        let mut target = self.route_target(path);
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
        // stuck against the rock: give up the detour, turn back along the route and rise for a few seconds
        if self.speed < 0.9 && self.burst <= 0.0 && self.escape <= 0.0 { self.stuck += dt; } else { self.stuck = (self.stuck - dt * 0.5).max(0.0); }
        if self.stuck > 2.5 {
            self.stuck = 0.0; self.detour = None; self.escape = 4.0;
            if self.t > self.flipped + 20.0 { self.dir = -self.dir; self.flipped = self.t; }
        }
        if self.escape > 0.0 {
            self.escape -= dt;
            let g = field.grad(self.pos[0], self.pos[1], self.pos[2]);
            target = add(self.pos, add(mul(g, -6.0), [0.0, 3.0, 0.0]));
        }
        let mut want = norm(sub(target, self.pos));
        // obstacle avoidance: feel ahead with the smooth distance field and turn away from the rock; kelp stems
        // are soft obstacles he weaves between
        let mut push = [0.0; 3]; let mut danger: f64 = 0.0;
        let l = norm(cross(self.up, self.fwd));
        for (k, w) in [(1.5, 1.0), (3.5, 0.8), (6.0, 0.6), (9.0, 0.35)] {
            for side in [-1.0, 0.0, 1.0] {
                let pr = add(add(self.pos, mul(self.fwd, k * (1.0 + self.speed * 0.25))), mul(l, side * 0.9));
                let d = field.dist(pr[0], pr[1], pr[2]);
                let m = 2.4;
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
        // stay off the surface and inside the world
        if self.pos[1] > -5.0 { push[1] -= (self.pos[1] + 5.0) * 0.4; }
        let r = hypot2(self.pos[0], self.pos[2]); if r > 130.0 { push = add(push, mul(norm([-self.pos[0], 0.0, -self.pos[2]]), (r - 130.0) * 0.1)); }
        want = norm(add(want, push));
        if danger > 0.7 { self.speed *= 1.0 - dt * 0.6; }
        want[1] = clamp(want[1], -0.5, 0.5); let want = norm(want);

        // turn toward it with a limited, smoothed turn rate (yaw about world up, pitch about the side axis)
        let fwd = self.fwd;
        let yaw_err = { let a = fwd[2].atan2(fwd[0]); let b = want[2].atan2(want[0]); let mut d = b - a; while d > PI { d -= TAU; } while d < -PI { d += TAU; } d };
        let pitch_err = want[1].asin() - fwd[1].asin();
        let max_yaw = 0.55 + 0.5 * danger;
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
        field.collide(&mut self.pos, m, &mut v, 0.55);
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

        // animation: tail beat follows the speed, fins steer and balance
        let freq = 0.45 + 0.38 * self.speed;
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
        SharkPose { pos: self.pos, fwd: self.fwd, up: self.up, scale: SCALE,
            a: [self.phase, amp, clamp(-self.yaw_rate * 1.1, -0.9, 0.9), clamp(self.pitch_rate * 0.8, -0.5, 0.5)],
            b: [self.flap_l, self.flap_r, self.dorsal, -tail * 1.6] }
    }
    /// collision spheres (centre, radius) along his body and at the pectoral fins
    pub fn body_spheres(&self) -> [(V3, f64); 7] {
        let f = self.fwd; let l = norm(cross(self.up, f)); let p = self.pos; let at = |a: f64, s: f64| add(add(p, mul(f, LENGTH * a)), mul(l, s));
        [(at(0.40, 0.0), 0.28), (at(0.22, 0.0), 0.42), (at(0.0, 0.0), 0.48), (at(-0.2, 0.0), 0.38), (at(-0.4, 0.0), 0.25), (at(0.08, 0.62), 0.16), (at(0.08, -0.62), 0.16)]
    }
    pub fn vel(&self) -> V3 { mul(self.fwd, self.speed) }
}

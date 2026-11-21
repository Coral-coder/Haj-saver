// The other plush animals, built entirely in code: a huge BLÅVINGAD-style blue whale, yellow octopuses, and (in
// December) Christmas trees. No openly licensed 3D files of the BLÅVINGAD toys exist, so these are modelled
// procedurally in the same soft, stitched-plush style and animated in shaders/plush.wgsl.
//
// Mesh space: forward +z, up +y, left +x. Vertex: position f32x3, normal f32x3, colour unorm8x4 (alpha < 1 means
// it glows: 1 - alpha is the glow), anim f32x2 (what the vertex shader bends: see plush.wgsl).
use crate::math::*;
use crate::shark::v3::*;
use crate::world::Field;

pub const STRIDE: usize = 36;

#[derive(Default)]
pub struct PMesh { pub v: Vec<u8>, pub i: Vec<u32> }
impl PMesh {
    fn n(&self) -> u32 { (self.v.len() / STRIDE) as u32 }
    fn vert(&mut self, p: V3, n: V3, c: [u8; 4], a: [f32; 2]) -> u32 {
        let k = self.n();
        for x in p { self.v.extend_from_slice(&(x as f32).to_le_bytes()); }
        let n = norm(n); for x in n { self.v.extend_from_slice(&(x as f32).to_le_bytes()); }
        self.v.extend_from_slice(&c); self.v.extend_from_slice(&a[0].to_le_bytes()); self.v.extend_from_slice(&a[1].to_le_bytes());
        k
    }
    /// a closed surface of revolution-like grid: f(u, v) -> (position, normal, colour, anim), u along, v around
    fn grid(&mut self, nu: usize, nv: usize, f: &dyn Fn(f64, f64) -> (V3, V3, [u8; 4], [f32; 2])) {
        let b = self.n();
        for i in 0..=nu { for j in 0..=nv { let (p, n, c, a) = f(i as f64 / nu as f64, j as f64 / nv as f64); self.vert(p, n, c, a); } }
        let w = (nv + 1) as u32;
        for i in 0..nu as u32 { for j in 0..nv as u32 {
            let (a, bb, c, d) = (b + i * w + j, b + i * w + j + 1, b + (i + 1) * w + j + 1, b + (i + 1) * w + j);
            self.i.extend_from_slice(&[a, d, c, a, c, bb]);
        } }
    }
    fn sphere(&mut self, c: V3, r: V3, col: [u8; 4], a: [f32; 2], seg: usize) {
        self.grid(seg, seg * 2, &|u, v| {
            let th = u * PI; let ph = v * TAU; let n = [th.sin() * ph.cos(), th.cos(), th.sin() * ph.sin()];
            ([c[0] + n[0] * r[0], c[1] + n[1] * r[1], c[2] + n[2] * r[2]], [n[0] / r[0], n[1] / r[1], n[2] / r[2]], col, a)
        });
    }
    /// a flat-ish plush panel (fin, fluke, tree tier) from an outline in the xz plane, puffed to thickness t
    fn puffed(&mut self, outline: &[[f64; 2]], y: f64, t: f64, col: [u8; 4], col_under: [u8; 4], a: &dyn Fn(f64, f64) -> [f32; 2]) {
        let c = outline.iter().fold([0.0, 0.0], |s, p| [s[0] + p[0] / outline.len() as f64, s[1] + p[1] / outline.len() as f64]);
        for (side, col) in [(1.0, col), (-1.0, col_under)] {
            let b = self.n();
            self.vert([c[0], y + side * t, c[1]], [0.0, side, 0.0], col, a(c[0], c[1]));
            for p in outline { self.vert([p[0], y, p[1]], [(p[0] - c[0]) * 0.3, side, (p[1] - c[1]) * 0.3], col, a(p[0], p[1])); }
            let n = outline.len() as u32;
            for k in 0..n { let (i0, i1) = (b + 1 + k, b + 1 + (k + 1) % n); if side > 0.0 { self.i.extend_from_slice(&[b, i1, i0]); } else { self.i.extend_from_slice(&[b, i0, i1]); } }
        }
    }
}
fn rgb(h: u32) -> [u8; 4] { [(h >> 16) as u8, (h >> 8) as u8, h as u8, 255] }
fn mixc(a: [u8; 4], b: [u8; 4], t: f64) -> [u8; 4] { let t = clamp(t, 0.0, 1.0); [0, 1, 2, 3].map(|k| (a[k] as f64 + (b[k] as f64 - a[k] as f64) * t).round() as u8) }

/// BLÅVINGAD-style blue whale: chunky plush body (dark blue back, pale grooved belly), flippers, a two-lobed fluke,
/// stitched eyes. 1 unit = 1 m at scale 1; about 2 m long, scaled up in the world. anim.x: 0 at the head to 1 at the
/// fluke tip (the up-and-down swimming wave grows along it); anim.y: 1 on the flippers.
pub fn whale() -> PMesh {
    let mut m = PMesh::default();
    let (back, side, belly, groove) = (rgb(0x284f8c), rgb(0x3f6fb5), rgb(0xdde6f0), rgb(0xc3d2e4));
    let len = 2.0;
    let prof = |t: f64| { // t: 0 tail .. 1 nose
        let r = if t < 0.62 { 0.06 + 0.34 * (t / 0.62).powf(0.8) } else { 0.40 * (1.0 - ((t - 0.62) / 0.38).powf(2.4)).max(0.0).powf(0.45) };
        r.max(0.0)
    };
    m.grid(40, 32, &|u, v| {
        let t = u.min(0.999); let r = prof(t); let a = v * TAU;
        let (ca, sa) = (a.cos(), a.sin());
        let p = [ca * r * 1.08, sa * r * 0.92 + 0.04 * (t - 0.5), (t - 0.5) * len];
        let below = -sa; // 1 at the bottom
        let mut c = mixc(back, side, (0.3 - sa) * 1.1);
        if below > 0.25 && t > 0.25 {
            c = mixc(c, belly, (below - 0.25) * 6.0);
            // ventral grooves: pale stripes along the belly
            if below > 0.55 && t > 0.55 && (ca * 11.0).sin().abs() < 0.18 { c = mixc(c, groove, 0.6); }
        }
        (p, [ca / 1.08, sa / 0.92, 0.0], c, [(1.0 - t) as f32, 0.0])
    });
    // fluke: two lobes behind the tail, horizontal
    let tz = -len * 0.5;
    let lobe: Vec<[f64; 2]> = (0..=16).map(|i| { let s = i as f64 / 16.0 * TAU; let x = 0.42 * s.cos(); let z = -0.17 + 0.16 * s.sin() - 0.12 * (x.abs() / 0.42).powi(2); [x, tz + z] }).collect();
    m.puffed(&lobe, 0.0, 0.035, back, side, &|_, z| [(1.0 + ((tz - z) / 0.35).max(0.0) * 0.4) as f32, 0.0]);
    // flippers
    for sx in [-1.0, 1.0] {
        let fl: Vec<[f64; 2]> = (0..=14).map(|i| { let s = i as f64 / 14.0 * TAU; let x = sx * (0.36 + 0.22 * (0.5 + 0.5 * s.cos())); let z = 0.42 + 0.09 * s.sin() - 0.12 * (0.5 + 0.5 * s.cos()); [x, z] }).collect();
        m.puffed(&fl, -0.2, 0.03, side, belly, &|x, _| [0.3, ((x.abs() - 0.3) / 0.25).clamp(0.0, 1.0) as f32]);
    }
    // embroidered eyes
    for sx in [-1.0, 1.0] { m.sphere([sx * 0.33, 0.02, 0.62], [0.035, 0.035, 0.035], rgb(0x0d1018), [0.2, 0.0], 6); }
    m
}

/// a food pellet: a little crumpled flake
pub fn pellet() -> PMesh { let mut m = PMesh::default(); m.sphere([0.0; 3], [0.12, 0.06, 0.1], [214, 140, 70, 200], [0.0, 0.0], 4); m }

/// BLÅVINGAD-style octopus: a round yellow head and eight short curling tentacles. About 1 m across at scale 1.
/// anim.x: 0 on the head, 0..1 along a tentacle; anim.y: the tentacle's angle / TAU.
pub fn octopus() -> PMesh {
    let mut m = PMesh::default();
    let (yel, yel2, under) = (rgb(0xf3c22e), rgb(0xe3a51c), rgb(0xf7dc7a));
    // head: an egg, a little taller than wide
    m.grid(20, 28, &|u, v| {
        let th = u * PI; let ph = v * TAU; let n = [th.sin() * ph.cos(), th.cos(), th.sin() * ph.sin()];
        let r = [0.34, 0.38, 0.34];
        ([n[0] * r[0], 0.32 + n[1] * r[1] * if n[1] > 0.0 { 1.0 } else { 0.7 }, n[2] * r[2]], n, mixc(yel, yel2, 0.5 - n[1] * 0.8), [0.0, 0.0])
    });
    // tentacles: tapered tubes curving out and down, tips curling up
    for k in 0..8 {
        let ang = k as f64 / 8.0 * TAU + 0.2;
        let (ca, sa) = (ang.cos(), ang.sin());
        m.grid(14, 10, &|u, v| {
            let s = u; let r = 0.085 * (1.0 - 0.75 * s);
            let out = 0.12 + 0.5 * s; let y = 0.12 - 0.2 * s + 0.16 * (s * s * s);
            let c = [ca * out, y, sa * out];
            let tng = norm([ca * 0.5, -0.2 + 0.48 * s * s, sa * 0.5]); let side = [-sa, 0.0, ca]; let nn = cross(side, tng);
            let a = v * TAU; let d = add(mul(side, a.cos()), mul(nn, a.sin()));
            let col = if a.sin() < -0.3 { under } else { yel };
            (add(c, mul(d, r)), d, col, [s as f32, (k as f64 / 8.0) as f32])
        });
    }
    for sx in [-1.0, 1.0] { m.sphere([sx * 0.13, 0.38, 0.3], [0.04, 0.05, 0.03], rgb(0x101010), [0.0, 0.0], 6); }
    m
}

/// A Christmas tree for the seabed: five ragged tiers, a trunk, a star, baubles and fairy lights (glowing vertices).
/// About 4.5 m tall at scale 1. anim.x = height / 4.5 (it sways a little in the current).
pub fn tree(seed: u32) -> PMesh {
    let mut m = PMesh::default();
    let mut s = seed | 1; let mut rnd = || { s ^= s << 13; s ^= s >> 17; s ^= s << 5; s as f64 / 4294967296.0 };
    let (g1, g2) = (rgb(0x0f4a2a), rgb(0x1f6e3a));
    // trunk
    m.grid(2, 10, &|u, v| { let a = v * TAU; ([a.cos() * 0.16, u * 0.8, a.sin() * 0.16], [a.cos(), 0.0, a.sin()], rgb(0x4a2e1a), [0.0, 0.0]) });
    for t in 0..5 {
        let y0 = 0.5 + t as f64 * 0.72; let h = 1.35 - t as f64 * 0.12; let r0 = 1.45 - t as f64 * 0.27;
        // a cone with a zigzag hem
        m.grid(6, 36, &|u, v| {
            let a = v * TAU; let hem = 1.0 + 0.12 * (a * 9.0).sin().abs();
            let r = r0 * (1.0 - u) * if u < 0.01 { hem } else { 1.0 }; let y = y0 + h * u - 0.18 * (1.0 - u) * (a * 9.0).sin().abs();
            ([a.cos() * r, y, a.sin() * r], [a.cos(), r0 / h, a.sin()], mixc(g1, g2, u + 0.15 * (a * 5.0).sin()), [(y / 4.5) as f32, 0.0])
        });
        // baubles and lights around the tier
        for _ in 0..7 {
            let a = rnd() * TAU; let u = 0.1 + rnd() * 0.5; let r = r0 * (1.0 - u) + 0.05; let y = y0 + h * u - 0.05;
            let col = [rgb(0xd21f2b), rgb(0xe8b322), rgb(0x2e6ad6), rgb(0xc93fa8)][(rnd() * 4.0) as usize];
            let glow = [col[0], col[1], col[2], 150];
            m.sphere([a.cos() * r, y, a.sin() * r], [0.09, 0.09, 0.09], glow, [(y / 4.5) as f32, 0.0], 5);
        }
        for i in 0..14 {
            let a = i as f64 / 14.0 * TAU + t as f64; let u = 0.15 + 0.05 * (i % 3) as f64; let r = r0 * (1.0 - u) + 0.04; let y = y0 + h * u;
            let warm = [255, 214, 140, 0];
            m.sphere([a.cos() * r, y, a.sin() * r], [0.035, 0.035, 0.035], warm, [(y / 4.5) as f32, 1.0 + (i % 7) as f32], 3);
        }
    }
    // the star
    let top = 0.5 + 4.0 * 0.72 + 1.35 - 4.0 * 0.12;
    let star: Vec<[f64; 2]> = (0..10).map(|i| { let a = i as f64 / 10.0 * TAU; let r = if i % 2 == 0 { 0.32 } else { 0.13 }; [a.sin() * r, a.cos() * r] }).collect();
    // in the xy plane: build in xz then swap
    let b = m.n(); m.puffed(&star, 0.0, 0.06, [255, 220, 90, 0], [255, 220, 90, 0], &|_, _| [1.0, 0.0]);
    for k in b as usize..m.n() as usize { let o = k * STRIDE; let y = f32::from_le_bytes(m.v[o + 4..o + 8].try_into().unwrap()); let z = f32::from_le_bytes(m.v[o + 8..o + 12].try_into().unwrap());
        m.v[o + 4..o + 8].copy_from_slice(&(z + top as f32 + 0.2).to_le_bytes()); m.v[o + 8..o + 12].copy_from_slice(&y.to_le_bytes()); }
    m
}

// ---------------------------------------------------------------- behaviour

/// what the renderer needs for one plush animal: where, which way, how big, and its animation inputs
#[derive(Clone, Copy, Default)]
pub struct PlushPose { pub pos: V3, pub fwd: V3, pub up: V3, pub scale: f64, pub kind: u32, pub a: [f64; 3] }

/// The whale: cruises a slow loop high over the reef. The loop is a closed curve through open-water waypoints,
/// one per compass sector, each checked to have room for his bulk and a clear way to the next.
pub struct Whale { pub pos: V3, pub fwd: V3, t: f64, phase: f64, way: Vec<V3>, u: f64 }
pub const WHALE_SCALE: f64 = 7.0; // about 14 m long
fn loop_at(way: &[V3], u: f64) -> V3 {
    let n = way.len(); let i = u.floor() as isize; let f = u - i as f64;
    let g = |k: isize| way[k.rem_euclid(n as isize) as usize];
    let (a, b, c, d) = (g(i - 1), g(i), g(i + 1), g(i + 2));
    [0, 1, 2].map(|k| 0.5 * (2.0 * b[k] + (c[k] - a[k]) * f + (2.0 * a[k] - 5.0 * b[k] + 4.0 * c[k] - d[k]) * f * f + (3.0 * b[k] - a[k] - 3.0 * c[k] + d[k]) * f * f * f))
}
impl Whale {
    pub fn new(seed: f64, field: &Field) -> Whale {
        let n = 10; let mut way: Vec<V3> = vec![];
        for k in 0..n {
            let a = k as f64 / n as f64 * TAU + 0.3;
            let mut pick = None;
            'p: for r in [85.0, 95.0, 75.0, 105.0, 65.0, 55.0, 115.0, 45.0] { for y in [-14.0, -18.0, -11.0, -22.0, -26.0] {
                let p = [a.cos() * r, y, a.sin() * r];
                if field.dist_static(p[0], p[1], p[2]) < -7.0 && way.last().is_none_or(|q| field.clear_line(*q, p, 4.0)) { pick = Some(p); break 'p; }
            } }
            if let Some(p) = pick { way.push(p); }
        }
        if way.len() < 3 { way = vec![[60.0, -14.0, 0.0], [0.0, -14.0, 60.0], [-60.0, -14.0, 0.0], [0.0, -14.0, -60.0]]; }
        if std::env::var("HAJ_DEBUG").is_ok() { eprintln!("whale loop: {} waypoints", way.len()); }
        let u = (seed * way.len() as f64).fract() * way.len() as f64;
        let pos = loop_at(&way, u); let fwd = norm(sub(loop_at(&way, u + 0.05), pos));
        Whale { pos, fwd, t: seed * 100.0, phase: 0.0, way, u }
    }
    pub fn step(&mut self, dt: f64, _field: &Field) {
        self.t += dt;
        let speed = 1.6;
        let seg = len(sub(loop_at(&self.way, self.u + 0.1), loop_at(&self.way, self.u))) * 10.0;
        self.u = (self.u + speed * dt / seg.max(1.0)) % self.way.len() as f64;
        let target = loop_at(&self.way, self.u + 0.25);
        let want = norm(sub(target, self.pos));
        let k = 1.0 - (-dt * 0.6).exp();
        self.fwd = norm(add(mul(self.fwd, 1.0 - k), mul(want, k)));
        self.fwd[1] = clamp(self.fwd[1], -0.15, 0.15); self.fwd = norm(self.fwd);
        // stay on the loop (it is the part that was checked for room)
        let on = loop_at(&self.way, self.u);
        self.pos = add(add(self.pos, mul(self.fwd, speed * dt)), mul(sub(on, self.pos), dt * 0.3));
        self.phase += dt * TAU * 0.12;
    }
    pub fn pose(&self) -> PlushPose { PlushPose { pos: self.pos, fwd: self.fwd, up: [0.0, 1.0, 0.0], scale: WHALE_SCALE, kind: 0, a: [self.phase, 0.0, 0.0] } }
    /// collision spheres along the body for the field (so the sharks and cameras keep clear of it)
    pub fn spheres(&self) -> Vec<[f64; 4]> {
        (0..7).map(|i| { let z = (i as f64 / 6.0 - 0.5) * 2.0 * WHALE_SCALE * 0.9; let r = WHALE_SCALE * if i == 0 { 0.12 } else { 0.38 } * (1.0 - (i as f64 / 6.0 - 0.62).abs() * 0.8); let p = add(self.pos, mul(self.fwd, z)); [p[0], p[1], p[2], r] }).collect()
    }
}

/// An octopus: potters about just above the seabed, and now and then pushes off with a jet of its tentacles.
pub struct Octopus { pub pos: V3, pub head: f64, goal: V3, t: f64, jet: f64, phase: f64, rng: u32, vy: f64 }
impl Octopus {
    pub fn new(home: V3, seed: u32) -> Octopus { Octopus { pos: home, head: seed as f64, goal: home, t: seed as f64 * 3.0, jet: 0.0, phase: 0.0, rng: seed | 1, vy: 0.0 } }
    fn rand(&mut self) -> f64 { let mut x = self.rng; x ^= x << 13; x ^= x >> 17; x ^= x << 5; self.rng = x; x as f64 / 4294967296.0 }
    fn floor(field: &Field, x: f64, y: f64, z: f64) -> f64 {
        let mut yy = y; for _ in 0..120 { if field.dist_static(x, yy, z) > -0.05 { return yy; } yy -= 0.25; } yy
    }
    pub fn step(&mut self, dt: f64, field: &Field) {
        self.t += dt;
        if len(sub(self.goal, self.pos)) < 1.0 || (self.t * 0.05).fract() < dt * 0.05 {
            for _ in 0..10 { let a = self.rand() * TAU; let g = [self.pos[0] + a.cos() * 8.0, self.pos[1] + 2.0, self.pos[2] + a.sin() * 8.0]; if field.dist_static(g[0], g[1], g[2]) < -1.0 { self.goal = g; break; } }
            if self.rand() < 0.35 { self.jet = 1.0; }
        }
        let d = sub(self.goal, self.pos); let dh = norm([d[0], 0.0, d[2]]);
        let sp = 0.35 + self.jet * 1.6;
        let want_head = dh[2].atan2(dh[0]); let mut e = want_head - self.head; while e > PI { e -= TAU; } while e < -PI { e += TAU; }
        self.head += e * (1.0 - (-dt * 0.8).exp());
        let mut p = add(self.pos, mul([self.head.cos(), 0.0, self.head.sin()], sp * dt));
        // hover 0.6 m over the floor; a jet lifts it, then it sinks back
        let fl = Octopus::floor(field, p[0], p[1] + 2.0, p[2]);
        let rest = fl + 0.6 + self.jet * 2.5;
        self.vy += ((rest - p[1]) * 1.5 - self.vy * 1.8) * dt; p[1] += self.vy * dt;
        if field.dist_static(p[0], p[1], p[2]) < -0.4 { self.pos = p; } else { self.goal = self.pos; self.vy = self.vy.max(0.3); }
        self.jet = (self.jet - dt * 0.35).max(0.0);
        self.phase += dt * (1.2 + self.jet * 4.0);
    }
    pub fn pose(&self) -> PlushPose {
        PlushPose { pos: self.pos, fwd: [self.head.cos(), 0.0, self.head.sin()], up: [0.0, 1.0, 0.0], scale: 1.1, kind: 1, a: [self.phase, self.jet, 0.0] }
    }
    pub fn sphere(&self) -> [f64; 4] { [self.pos[0], self.pos[1] + 0.35, self.pos[2], 0.7] }
}

/// where the December trees stand: on the seabed beside the scenic route, far enough apart that the renderer can
/// always show just the nearest one
pub fn tree_spots(field: &Field, path: &crate::render::FlyPath) -> Vec<V3> {
    let mut out: Vec<V3> = vec![];
    for (i, f) in [0.06, 0.27, 0.48, 0.69, 0.9].iter().enumerate() {
        let p = path.at(*f); let side = norm([-(p[5] - p[2]), 0.0, p[3] - p[0]]);
        'try_: for off in [9.0, -9.0, 13.0, -13.0, 6.0, -6.0] {
            let x = p[0] + side[0] * off; let z = p[2] + side[2] * off;
            // the first rock below open water, coming down from well above
            let mut y = -8.0;
            while y > -55.0 && field.dist(x, y, z) < -0.05 { y -= 0.3; }
            if std::env::var("HAJ_DEBUG").is_ok() { eprintln!("tree cand ({x:.0},{z:.0}) y={y:.1} above={:?}", (2..7).map(|k| (field.dist(x, y + k as f64, z) * 10.0).round() / 10.0).collect::<Vec<_>>()); }
            if y <= -55.0 || y > -20.0 { continue; }
            // room for the tree: open water above the spot, and away from the camera route itself
            if (2..7).all(|k| field.dist(x, y + k as f64, z) < -0.5 * k as f64) && out.iter().all(|o| len(sub(*o, [x, y, z])) > 60.0) { out.push([x, y - 0.3, z]); break 'try_; }
        }
        let _ = i;
    }
    out
}

/// The fish texture, made in code: 256 x 128 RGBA masks in fish space (u from the nose back to the tail tip, v from
/// the back down to the belly). r: scales, g: bars (for the striped kinds), b: the eye, a: fin rays + lateral line.
/// fsFish (shaders/anim.wgsl) colours each species from these.
pub fn fish_texture() -> Vec<u8> {
    let (w, h) = (256usize, 128usize);
    let mut t = vec![0u8; w * h * 4];
    for y in 0..h { for x in 0..w {
        let u = (x as f64 + 0.5) / w as f64; let v = (y as f64 + 0.5) / h as f64;
        // scales: offset rows of arcs
        let su = u * 46.0; let sv = v * 22.0; let row = sv.floor(); let fu = (su + 0.5 * (row as i64 % 2) as f64).fract(); let fv = sv.fract();
        let arc = ((fu - 0.5).powi(2) * 4.0 + (fv - 0.15).powi(2) * 2.2).sqrt();
        let scales = 0.78 + 0.22 * sstep(0.55, 0.95, arc) * sstep(0.05, 0.3, u) * sstep(0.95, 0.75, u);
        // vertical bars, softened at the ends
        let bars = sstep(0.25, 0.05, ((u * 7.0 + 0.3).fract() - 0.5).abs()) * sstep(0.1, 0.18, u) * sstep(0.78, 0.68, u);
        // the eye: a dark pupil in a pale ring, a little above the middle near the nose
        let (eu, ev) = ((u - 0.12) * 2.0, v - 0.40); let er = (eu * eu + ev * ev).sqrt();
        let eye = if er < 0.05 { 1.0 } else if er < 0.08 { 0.5 } else { 0.0 };
        // fin rays toward the tail, and the lateral line along the flank
        let rays = if u > 0.72 { 0.5 + 0.5 * ((v * 60.0 + u * 8.0).sin()).abs().powf(4.0) } else { 0.0 };
        let lateral = sstep(0.012, 0.0, (v - 0.47 - 0.05 * (u * 3.0).sin()).abs()) * sstep(0.15, 0.25, u) * sstep(0.8, 0.7, u);
        let o = (x + y * w) * 4;
        t[o] = (scales * 255.0) as u8; t[o + 1] = (bars * 255.0) as u8; t[o + 2] = (eye * 255.0) as u8; t[o + 3] = ((rays * 0.6 + lateral).min(1.0) * 255.0) as u8;
    } }
    t
}

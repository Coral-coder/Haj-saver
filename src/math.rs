// Math + procedural helpers, ported from abyssal-garden's source/src/math.js (load-time only unless noted).
#![allow(clippy::too_many_arguments)]
use std::cell::Cell;

pub const PI: f64 = std::f64::consts::PI;
pub const TAU: f64 = PI * 2.0;

thread_local! { static SEED: Cell<u32> = const { Cell::new(20261110) }; }

/// mulberry32, bit-exact with the JS version
pub fn rnd() -> f64 {
    SEED.with(|c| {
        let s = c.get().wrapping_add(0x6D2B79F5);
        c.set(s);
        let mut t = (s ^ (s >> 15)).wrapping_mul(1 | s);
        t = (t.wrapping_add((t ^ (t >> 7)).wrapping_mul(61 | t))) ^ t;
        ((t ^ (t >> 14)) as f64) / 4294967296.0
    })
}
pub fn rr(a: f64, b: f64) -> f64 { a + (b - a) * rnd() }
/// each generator system starts its own random stream, so the systems stay independent
pub fn rseed(s: i32) { SEED.with(|c| c.set(20261110i32.wrapping_add(s.wrapping_mul(7919)) as u32)); }
pub fn clamp(x: f64, a: f64, b: f64) -> f64 { if x < a { a } else if x > b { b } else { x } }
pub fn mix(a: f64, b: f64, t: f64) -> f64 { a + (b - a) * t }
pub fn sstep(a: f64, b: f64, x: f64) -> f64 { let x = clamp((x - a) / (b - a), 0.0, 1.0); x * x * (3.0 - 2.0 * x) }
pub fn hypot2(x: f64, y: f64) -> f64 { (x * x + y * y).sqrt() }
pub fn hypot3(x: f64, y: f64, z: f64) -> f64 { (x * x + y * y + z * z).sqrt() }
pub fn fract(x: f64) -> f64 { x - x.floor() }

fn ihash(x: i32, y: i32, z: i32) -> f64 {
    let h = x.wrapping_mul(374761393).wrapping_add(y.wrapping_mul(668265263)).wrapping_add(z.wrapping_mul(1274126177)) as u32;
    let h = (h ^ (h >> 13)).wrapping_mul(1274126177);
    ((h ^ (h >> 16)) as f64) / 4294967296.0
}
pub fn vnoise(x: f64, y: f64, z: f64) -> f64 {
    let (fx0, fy0, fz0) = (x.floor(), y.floor(), z.floor());
    let (ix, iy, iz) = (fx0 as i32, fy0 as i32, fz0 as i32);
    let (mut fx, mut fy, mut fz) = (x - fx0, y - fy0, z - fz0);
    fx = fx * fx * (3.0 - 2.0 * fx); fy = fy * fy * (3.0 - 2.0 * fy); fz = fz * fz * (3.0 - 2.0 * fz);
    let a = mix(mix(ihash(ix, iy, iz), ihash(ix + 1, iy, iz), fx), mix(ihash(ix, iy + 1, iz), ihash(ix + 1, iy + 1, iz), fx), fy);
    let b = mix(mix(ihash(ix, iy, iz + 1), ihash(ix + 1, iy, iz + 1), fx), mix(ihash(ix, iy + 1, iz + 1), ihash(ix + 1, iy + 1, iz + 1), fx), fy);
    mix(a, b, fz)
}
pub fn fbm(mut x: f64, mut y: f64, mut z: f64, o: i32) -> f64 {
    let (mut a, mut s) = (0.5, 0.0);
    for _ in 0..o { s += a * vnoise(x, y, z); x = x * 2.03 + 17.1; y = y * 2.03 + 31.7; z = z * 2.03 + 5.3; a *= 0.5; }
    s / (1.0 - 2.0 * a)
}

pub type Q = [f64; 4];
pub fn q_axis(x: f64, y: f64, z: f64, a: f64) -> Q { let l = hypot3(x, y, z); let l = if l == 0.0 { 1.0 } else { l }; let s = (a / 2.0).sin() / l; [x * s, y * s, z * s, (a / 2.0).cos()] }
pub fn q_mul(a: Q, b: Q) -> Q {
    [a[3] * b[0] + a[0] * b[3] + a[1] * b[2] - a[2] * b[1],
     a[3] * b[1] - a[0] * b[2] + a[1] * b[3] + a[2] * b[0],
     a[3] * b[2] + a[0] * b[1] - a[1] * b[0] + a[2] * b[3],
     a[3] * b[3] - a[0] * b[0] - a[1] * b[1] - a[2] * b[2]]
}
pub fn q_rand() -> Q {
    let q = [rr(-1.0, 1.0), rr(-1.0, 1.0), rr(-1.0, 1.0), rr(-1.0, 1.0)];
    let l = (q[0] * q[0] + q[1] * q[1] + q[2] * q[2] + q[3] * q[3]).sqrt(); let l = if l == 0.0 { 1.0 } else { l };
    [q[0] / l, q[1] / l, q[2] / l, q[3] / l]
}
/// shortest rotation taking +Y to n
pub fn q_from_y(nx: f64, ny: f64, nz: f64) -> Q {
    if ny < -0.9999 { return [1.0, 0.0, 0.0, 0.0]; }
    let q = [nz, 0.0, -nx, 1.0 + ny]; let l = (q[0] * q[0] + q[2] * q[2] + q[3] * q[3]).sqrt();
    [q[0] / l, 0.0, q[2] / l, q[3] / l]
}

// world grid
pub const NX: usize = 512;
pub const NY: usize = 80;
pub const NZ: usize = 512;
pub const VS: f64 = 0.75;
pub const OX: f64 = -192.0;
pub const OY: f64 = -56.0;
pub const OZ: f64 = -192.0;
pub const CELLS: usize = 16;
pub const CELLW: f64 = 24.0; // culling grid over XZ
pub const NCHUNK: usize = 7; // 3 small + 3 large rock chunk meshes + pebbles

pub struct Nets { pub pos: Vec<f32>, pub idx: Vec<u32> }

/// naive surface nets. D: scalar field (>0 inside). Returns grid-space positions + triangle indices.
pub fn surface_nets(d: &[f32], nx: usize, ny: usize, nz: usize) -> Nets {
    let mut pos: Vec<f32> = Vec::with_capacity(1 << 20);
    let mut idx: Vec<u32> = Vec::with_capacity(1 << 21);
    let sl = nx * ny;
    let mut cur = vec![0u32; sl];
    let mut prev = vec![0u32; sl];
    for z in 0..nz - 1 {
        for y in 0..ny - 1 {
            let mut i0 = nx * (y + ny * z);
            for x in 0..nx - 1 {
                let (d0, d1, d2, d3) = (d[i0], d[i0 + 1], d[i0 + nx], d[i0 + nx + 1]);
                let (d4, d5, d6, d7) = (d[i0 + sl], d[i0 + sl + 1], d[i0 + sl + nx], d[i0 + sl + nx + 1]);
                let (b0, b1, b2, b3, b4, b5, b6, b7) = (d0 > 0.0, d1 > 0.0, d2 > 0.0, d3 > 0.0, d4 > 0.0, d5 > 0.0, d6 > 0.0, d7 > 0.0);
                let m = b0 as u32 | (b1 as u32) << 1 | (b2 as u32) << 2 | (b3 as u32) << 3 | (b4 as u32) << 4 | (b5 as u32) << 5 | (b6 as u32) << 6 | (b7 as u32) << 7;
                if m == 0 || m == 255 { i0 += 1; continue; }
                let (mut sx, mut sy, mut sz, mut n) = (0f32, 0f32, 0f32, 0f32);
                if b0 != b1 { sx += d0 / (d0 - d1); n += 1.0; }
                if b2 != b3 { sx += d2 / (d2 - d3); sy += 1.0; n += 1.0; }
                if b4 != b5 { sx += d4 / (d4 - d5); sz += 1.0; n += 1.0; }
                if b6 != b7 { sx += d6 / (d6 - d7); sy += 1.0; sz += 1.0; n += 1.0; }
                if b0 != b2 { sy += d0 / (d0 - d2); n += 1.0; }
                if b1 != b3 { sy += d1 / (d1 - d3); sx += 1.0; n += 1.0; }
                if b4 != b6 { sy += d4 / (d4 - d6); sz += 1.0; n += 1.0; }
                if b5 != b7 { sy += d5 / (d5 - d7); sx += 1.0; sz += 1.0; n += 1.0; }
                if b0 != b4 { sz += d0 / (d0 - d4); n += 1.0; }
                if b1 != b5 { sz += d1 / (d1 - d5); sx += 1.0; n += 1.0; }
                if b2 != b6 { sz += d2 / (d2 - d6); sy += 1.0; n += 1.0; }
                if b3 != b7 { sz += d3 / (d3 - d7); sx += 1.0; sy += 1.0; n += 1.0; }
                let vi = (pos.len() / 3) as u32;
                pos.push(x as f32 + sx / n); pos.push(y as f32 + sy / n); pos.push(z as f32 + sz / n);
                let ci = x + nx * y;
                cur[ci] = vi;
                let s0 = b0;
                let mut emit = |a: u32, b: u32, c: u32, dd: u32| {
                    if s0 { idx.extend_from_slice(&[a, b, c, a, c, dd]); } else { idx.extend_from_slice(&[a, c, b, a, dd, c]); }
                };
                if b1 != s0 && y > 0 && z > 0 { emit(vi, cur[ci - nx], prev[ci - nx], prev[ci]); }
                if b2 != s0 && x > 0 && z > 0 { emit(vi, prev[ci], prev[ci - 1], cur[ci - 1]); }
                if b4 != s0 && x > 0 && y > 0 { emit(vi, cur[ci - 1], cur[ci - 1 - nx], cur[ci - nx]); }
                i0 += 1;
            }
        }
        std::mem::swap(&mut cur, &mut prev);
    }
    Nets { pos, idx }
}

/// trilinear sampler over a scalar grid (grid coords)
pub struct Sampler3<'a> { pub d: &'a [f32], pub nx: usize, pub ny: usize, pub nz: usize }
impl Sampler3<'_> {
    pub fn at(&self, x: f64, y: f64, z: f64) -> f64 {
        let x = clamp(x, 0.0, self.nx as f64 - 1.001); let y = clamp(y, 0.0, self.ny as f64 - 1.001); let z = clamp(z, 0.0, self.nz as f64 - 1.001);
        let (ix, iy, iz) = (x as usize, y as usize, z as usize);
        let (fx, fy, fz) = (x - ix as f64, y - iy as f64, z - iz as f64);
        let (nx, d) = (self.nx, self.d);
        let i = ix + nx * (iy + self.ny * iz); let sl = nx * self.ny;
        let g = |k: usize| d[k] as f64;
        let a = mix(mix(g(i), g(i + 1), fx), mix(g(i + nx), g(i + nx + 1), fx), fy);
        let b = mix(mix(g(i + sl), g(i + sl + 1), fx), mix(g(i + sl + nx), g(i + sl + nx + 1), fx), fy);
        mix(a, b, fz)
    }
}

pub type V3 = [f64; 3];

/// mesh builder (solid vertex format: f32x3 pos, snorm8x4 normal+aux)
#[derive(Default)]
pub struct Mesh { pub p: Vec<f64>, pub i: Vec<u32>, pub aux: Vec<f64> }
pub struct Built { pub vb: Vec<u8>, pub ib: Vec<u32>, pub nv: usize }

impl Mesh {
    pub fn new() -> Self { Self::default() }
    pub fn nverts(&self) -> usize { self.p.len() / 3 }
    pub fn v(&mut self, x: f64, y: f64, z: f64, aux: f64) -> u32 { self.p.extend_from_slice(&[x, y, z]); self.aux.push(aux); (self.p.len() / 3 - 1) as u32 }
    pub fn v1(&mut self, p: V3) -> u32 { self.v(p[0], p[1], p[2], 1.0) }
    pub fn tri(&mut self, a: u32, b: u32, c: u32) { self.i.extend_from_slice(&[a, b, c]); }
    pub fn quad(&mut self, a: u32, b: u32, c: u32, d: u32) { self.i.extend_from_slice(&[a, b, c, a, c, d]); }
    /// box-section sweep along a polyline; faces get their own vertices (hard edges)
    pub fn sweep(&mut self, pts: &[V3], w: &dyn Fn(usize) -> f64, h: &dyn Fn(usize) -> f64, up_fn: &dyn Fn(V3, usize) -> V3, caps: bool) {
        let mut rings: Vec<[u32; 8]> = vec![];
        let n = pts.len();
        for k in 0..n {
            let p = pts[k]; let a = pts[k.saturating_sub(1)]; let b = pts[(k + 1).min(n - 1)];
            let (mut tx, mut ty, mut tz) = (b[0] - a[0], b[1] - a[1], b[2] - a[2]);
            let tl = nz1(hypot3(tx, ty, tz)); tx /= tl; ty /= tl; tz /= tl;
            let u0 = up_fn(p, k);
            let (mut sx, mut sy, mut sz) = (u0[1] * tz - u0[2] * ty, u0[2] * tx - u0[0] * tz, u0[0] * ty - u0[1] * tx);
            let sl = nz1(hypot3(sx, sy, sz)); sx /= sl; sy /= sl; sz /= sl;
            let (ux, uy, uz) = (ty * sz - tz * sy, tz * sx - tx * sz, tx * sy - ty * sx);
            let (ww, hh) = (w(k), h(k));
            let c: Vec<V3> = [[-1.0, -1.0], [1.0, -1.0], [1.0, 1.0], [-1.0, 1.0]].iter().map(|&[e, f]: &[f64; 2]| {
                [p[0] + sx * e * ww / 2.0 + ux * f * hh / 2.0, p[1] + sy * e * ww / 2.0 + uy * f * hh / 2.0, p[2] + sz * e * ww / 2.0 + uz * f * hh / 2.0]
            }).collect();
            let mut r = [0u32; 8];
            for s in 0..4 { r[s * 2] = self.v1(c[s]); r[s * 2 + 1] = self.v1(c[(s + 1) % 4]); }
            rings.push(r);
            if caps && (k == 0 || k == n - 1) {
                let q: Vec<u32> = c.iter().map(|&cc| self.v1(cc)).collect();
                if k == 0 { self.quad(q[0], q[1], q[2], q[3]); } else { self.quad(q[3], q[2], q[1], q[0]); }
            }
        }
        for k in 0..rings.len().saturating_sub(1) {
            for s in 0..4 {
                let (a, b, c, d) = (rings[k][s * 2], rings[k][s * 2 + 1], rings[k + 1][s * 2 + 1], rings[k + 1][s * 2]);
                self.quad(a, d, c, b);
            }
        }
    }
    /// round tube along polyline with radius function
    pub fn tube(&mut self, pts: &[V3], rad: &dyn Fn(usize) -> f64, sides: usize, cap_end: bool, aux_fn: Option<&dyn Fn(usize) -> f64>) {
        let mut rings: Vec<Vec<u32>> = vec![];
        let (mut ux, mut uy, mut uz) = (0.31, 0.9, 0.3);
        let n = pts.len();
        for k in 0..n {
            let p = pts[k]; let a = pts[k.saturating_sub(1)]; let b = pts[(k + 1).min(n - 1)];
            let (mut tx, mut ty, mut tz) = (b[0] - a[0], b[1] - a[1], b[2] - a[2]);
            let tl = nz1(hypot3(tx, ty, tz)); tx /= tl; ty /= tl; tz /= tl;
            let (mut sx, mut sy, mut sz) = (uy * tz - uz * ty, uz * tx - ux * tz, ux * ty - uy * tx);
            let sl = nz1(hypot3(sx, sy, sz)); sx /= sl; sy /= sl; sz /= sl;
            ux = ty * sz - tz * sy; uy = tz * sx - tx * sz; uz = tx * sy - ty * sx;
            let r = rad(k);
            let mut ring = vec![];
            for s in 0..sides {
                let an = s as f64 / sides as f64 * TAU;
                let aux = match aux_fn { Some(f) => f(k), None => (0.35 + 0.65 * k as f64 / (n as f64 - 1.0)).min(1.0) };
                ring.push(self.v(p[0] + (sx * an.cos() + ux * an.sin()) * r, p[1] + (sy * an.cos() + uy * an.sin()) * r, p[2] + (sz * an.cos() + uz * an.sin()) * r, aux));
            }
            rings.push(ring);
        }
        for k in 0..rings.len() - 1 {
            for s in 0..sides { let s2 = (s + 1) % sides; self.quad(rings[k][s], rings[k][s2], rings[k + 1][s2], rings[k + 1][s]); }
        }
        if cap_end {
            let e = pts[n - 1];
            let aux = match aux_fn { Some(f) => f(n - 1), None => 1.0 };
            let c = self.v(e[0], e[1], e[2], aux);
            let r = rings[rings.len() - 1].clone();
            for s in 0..sides { self.tri(r[s], r[(s + 1) % sides], c); }
        }
    }
    /// normals area-weighted; aux -> normal.w
    pub fn build(&self) -> Built {
        let n = self.p.len() / 3; let pp = &self.p; let ii = &self.i;
        let mut nn = vec![0f64; n * 3];
        for t in (0..ii.len()).step_by(3) {
            let (a, b, c) = (ii[t] as usize * 3, ii[t + 1] as usize * 3, ii[t + 2] as usize * 3);
            let (ux, uy, uz) = (pp[b] - pp[a], pp[b + 1] - pp[a + 1], pp[b + 2] - pp[a + 2]);
            let (vx, vy, vz) = (pp[c] - pp[a], pp[c + 1] - pp[a + 1], pp[c + 2] - pp[a + 2]);
            let (nx, ny, nz) = (uy * vz - uz * vy, uz * vx - ux * vz, ux * vy - uy * vx);
            for k in [a, b, c] { nn[k] += nx; nn[k + 1] += ny; nn[k + 2] += nz; }
        }
        let mut vb = vec![0u8; n * 16];
        for k in 0..n {
            let (nx, ny, nz) = (nn[k * 3], nn[k * 3 + 1], nn[k * 3 + 2]);
            let l = nz1(hypot3(nx, ny, nz));
            write_vert(&mut vb, k, [pp[k * 3] as f32, pp[k * 3 + 1] as f32, pp[k * 3 + 2] as f32], [nx / l, ny / l, nz / l, clamp(self.aux[k], -1.0, 1.0)]);
        }
        Built { vb, ib: ii.clone(), nv: n }
    }
}
pub fn nz1(l: f64) -> f64 { if l == 0.0 || l.is_nan() { 1.0 } else { l } }
pub fn snorm(v: f64) -> u8 { ((v * 127.0).round() as i32).clamp(-127, 127) as i8 as u8 }
pub fn write_vert(vb: &mut [u8], k: usize, p: [f32; 3], n: [f64; 4]) {
    let o = k * 16;
    vb[o..o + 4].copy_from_slice(&p[0].to_le_bytes()); vb[o + 4..o + 8].copy_from_slice(&p[1].to_le_bytes()); vb[o + 8..o + 12].copy_from_slice(&p[2].to_le_bytes());
    for j in 0..4 { vb[o + 12 + j] = snorm(n[j]); }
}

/// icosphere with shared vertices
pub fn icosphere(sub: usize, f: &dyn Fn(f64, f64, f64) -> V3) -> Mesh {
    let mut m = Mesh::new(); let t = (1.0 + 5f64.sqrt()) / 2.0;
    let mut v: Vec<V3> = [[-1.0, t, 0.0], [1.0, t, 0.0], [-1.0, -t, 0.0], [1.0, -t, 0.0], [0.0, -1.0, t], [0.0, 1.0, t], [0.0, -1.0, -t], [0.0, 1.0, -t], [t, 0.0, -1.0], [t, 0.0, 1.0], [-t, 0.0, -1.0], [-t, 0.0, 1.0]]
        .iter().map(|v| { let l = hypot3(v[0], v[1], v[2]); [v[0] / l, v[1] / l, v[2] / l] }).collect();
    let mut faces: Vec<[usize; 3]> = vec![[0, 11, 5], [0, 5, 1], [0, 1, 7], [0, 7, 10], [0, 10, 11], [1, 5, 9], [5, 11, 4], [11, 10, 2], [10, 7, 6], [7, 1, 8], [3, 9, 4], [3, 4, 2], [3, 2, 6], [3, 6, 8], [3, 8, 9], [4, 9, 5], [2, 4, 11], [6, 2, 10], [8, 6, 7], [9, 8, 1]];
    for _ in 0..sub {
        let mut cache = std::collections::HashMap::new(); let mut f2 = vec![];
        let mut mid = |a: usize, b: usize, v: &mut Vec<V3>| -> usize {
            let key = if a < b { a * 65536 + b } else { b * 65536 + a };
            *cache.entry(key).or_insert_with(|| {
                let p = [v[a][0] + v[b][0], v[a][1] + v[b][1], v[a][2] + v[b][2]]; let l = hypot3(p[0], p[1], p[2]);
                v.push([p[0] / l, p[1] / l, p[2] / l]); v.len() - 1
            })
        };
        for &[a, b, c] in &faces { let ab = mid(a, b, &mut v); let bc = mid(b, c, &mut v); let ca = mid(c, a, &mut v); f2.extend_from_slice(&[[a, ab, ca], [b, bc, ab], [c, ca, bc], [ab, bc, ca]]); }
        faces = f2;
    }
    for q in &v { let p = f(q[0], q[1], q[2]); m.v1(p); }
    for fc in &faces { m.tri(fc[0] as u32, fc[1] as u32, fc[2] as u32); }
    m
}

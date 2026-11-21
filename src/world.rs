// World layout + procedural content generation (load-time only).
// Ported from abyssal-garden's source/src/world.js. Changes for Hajsaver:
//  - the floating boulder clusters ("floating blobs") are gone: every rock now sits on the seabed
//  - extra fish species (see FISH below) and a collision field the shark and the cameras use
#![allow(clippy::too_many_arguments)]
use crate::math::*;

/// designed rock masses: ax, az, bx, bz, radius, top, overhang, waist
pub const MESAS: [[f32; 8]; 22] = [
    [-37., -42., -33., -22., 9., -7., 3.5, 2.], [-29., -15., -33., 5., 12., -14., 3., 2.], [-36., 11., -30., 27., 9., -5., 4., 2.],
    [30., -45., 35., -27., 9., -9., 4., 2.], [37., -20., 31., -3., 11., -16., 3., 2.], [32., 5., 35., 9., 7., -8., 4., 2.],
    [19., -15., 19., -15., 4.4, -6., 2., 1.5],
    [6., -88., 6., -88., 10., -34., 0., 0.],
    [0., -106., 0., -106., 8., 3., 2., 2.],
    [44., 6., 47., -8., 7., -17., 3., 1.],
    [33., 29., 37., 12., 9., -19., -6.5, 0.],
    [74., 14., 74., 14., 8., -15., 3., 1.],
    [-84., -34., -80., -12., 10., -7., 6., 2.], [-78., -6., -83., 12., 12., -13., 5., 2.], [-82., 18., -79., 32., 9., -8., 6., 2.],
    [-17., 62., -19., 78., 10., -6., 9., 3.], [-21., 84., -18., 100., 11., -9., 8., 3.],
    [22., 67., 24., 80., 7., -11., 6., 2.],
    [-16., 122., 34., 122., 9., -27., 3., 2.],
    // padding to keep the array literal simple: zero-radius entries are skipped by MESA_N
    [0.; 8], [0.; 8], [0.; 8],
];
pub const MESA_N: usize = 19;
/// vertical ring arches: cx, cy, cz, yaw, major radius, minor radius, y squash
pub const ARCHES: [[f32; 7]; 3] = [[-23., -9.5, -16., 0.3, 4.2, 1.4, 1.2], [0., -45., -34., 1.57, 14., 2.7, 1.2], [-40., -45., 44., 1.57, 10., 2.2, 1.3]];
/// carved tunnels: ax, ay, az, radius, bx, by, bz
pub const HOLES: [[f32; 7]; 2] = [[19., -21., -23., 2.5, 19., -21., -7.], [9., -37., 108., 4.2, 9., -34., 136.]];
/// regions kept free of random mesas: ax, az, bx, bz, radius, softness
pub const CLEARS: [[f32; 6]; 8] = [[0., -45., 0., 35., 22., 10.], [0., -86., 0., -86., 46., 12.], [42., 24., 74., 6., 22., 10.], [72., -26., 72., -26., 26., 10.],
    [-60., -36., -60., 36., 13., 8.], [-60., 42., -18., 46., 12., 8.], [4., 58., 8., 100., 24., 10.], [-34., -74., -56., -46., 11., 8.]];
pub const PURPLE: [f64; 2] = [6.0, 84.0];
fn zone_p(x: f64, z: f64) -> f64 { sstep(44.0, 26.0, hypot2(x - PURPLE[0], z - PURPLE[1])) }
const MAGENTA: [[f64; 3]; 15] = [[14., 6., 13.], [-4., 20., 9.], [6., 84., 30.], [-58., 8., 10.], [50., 30., 12.], [8., -70., 10.], [-12., -98., 9.], [-36., 44., 9.], [-46., -60., 9.], [24., 42., 8.], [-56., 26., 9.], [-44., 38., 8.], [-10., 44., 8.], [12., 24., 8.], [-50., -45., 8.]];
fn mag_at(x: f64, z: f64) -> f64 {
    let mut m = sstep(0.54, 0.66, fbm(x * 0.03, 7.0, z * 0.03, 3)) * 0.9;
    for p in MAGENTA { m = m.max(sstep(p[2], p[2] * 0.45, hypot2(x - p[0], z - p[1]))); }
    m * sstep(0.3, 0.55, fbm(x * 0.17, 1.0, z * 0.17, 2) + m * 0.25)
}

pub fn prim_buffer() -> Vec<f32> {
    let mut a = vec![];
    for m in &MESAS[..MESA_N] { a.extend_from_slice(m); }
    for m in ARCHES { a.extend_from_slice(&m); a.push(0.); }
    for m in HOLES { a.extend_from_slice(&m); a.push(0.); }
    for m in CLEARS { a.extend_from_slice(&m); a.extend_from_slice(&[0., 0.]); }
    a
}

/// rock connected to the seabed: each column's unbroken run up from the bottom is, and the rock above a gap
/// (overhangs, specks) is when it touches rock that is. What never connects is a speck of noise floating in
/// the water (surface nets would make it a small box), so it becomes water.
pub fn drop_floating_rock(d: &mut [f32]) {
    let s = NX * NY;
    let mut r = vec![0u8; d.len()];
    let mut b = vec![1u8; NX];
    let mut l: Vec<usize> = vec![];
    let mut i = 0;
    while i < d.len() {
        b.fill(1);
        for _y in 0..NY { for x in 0..NX { if d[i] <= 0.0 { b[x] = 0; } else if b[x] != 0 { r[i] = 1; } else { l.push(i); } i += 1; } }
    }
    let at = |r: &[u8], i: usize, o: isize| -> bool { let j = i as isize + o; j >= 0 && (j as usize) < r.len() && r[j as usize] != 0 };
    let offs = [1isize, -1, NX as isize, -(NX as isize), s as isize, -(s as isize)];
    loop {
        let mut k = false;
        for &i in &l { if r[i] == 0 && offs.iter().any(|&o| at(&r, i, o)) { r[i] = 1; k = true; } }
        l.reverse();
        if !k { break; }
    }
    for &i in &l { if r[i] == 0 { d[i] = -d[i]; } }
}

// ---------------------------------------------------------------- collision field
/// Signed distance to the rock (metres, > 0 in rock), for the shark and the cameras.
/// fine: the original's 4x4x4-voxel bricks near the surface, clamped to ±1 m (exact, for collision response).
/// coarse: a half-resolution field clamped to ±8 m (for steering ahead of time).
pub struct Field {
    ci: Vec<u16>, cg: Vec<i8>, coarse: Vec<f32>,
    /// solid set pieces that are meshes, not terrain (the ship, the whale fluke): capsules a, b, radius
    caps: Vec<(V3, V3, f64)>,
    /// kelp columns (x, z, base y, top y), bucketed on a 4 m grid: soft obstacles that block the view
    kelp: Vec<[f32; 4]>, kgrid: Vec<Vec<u32>>,
}
const KG: usize = 100;
const KW: f64 = 4.0;
const CO: usize = 52;
const CN: usize = 102;
const HX: usize = NX / 2;
const HY: usize = NY / 2;
const HZ: usize = NZ / 2;
impl Field {
    pub fn new(d: &[f32]) -> Field {
        let mut ci = vec![0u16; CN * CN * NY / 4];
        let mut cg: Vec<i8> = Vec::with_capacity(64 * 40000);
        let mut n = 0usize;
        for bz in 0..CN { for by in 1..NY / 4 - 1 { for bx in 0..CN {
            if hypot2(bx as f64 - CN as f64 / 2.0 + 0.5, bz as f64 - CN as f64 / 2.0 + 0.5) > 52.0 { continue; }
            let (mut lo, mut hi) = (127i32, -127i32);
            let mut brick = [0i8; 64];
            for z in 0..4 { for y in 0..4 { for x in 0..4 {
                let i = CO + bx * 4 + x + NX * (by * 4 + y + NY * (CO + bz * 4 + z));
                let gz = d[i + NX * NY] - d[i - NX * NY]; let gx = d[i + 1] - d[i - 1]; let g = d[i + NX] - d[i - NX];
                let l = ((gx * gx + g * g + gz * gz) as f64).sqrt() / 1.5;
                let q = (clamp(d[i] as f64 / l.max(0.05), -1.0, 1.0) * 127.0).round() as i32;
                brick[x + 4 * y + 16 * z] = q as i8; lo = lo.min(q); hi = hi.max(q);
            } } }
            let idx = bx + CN * (by + NY / 4 * bz);
            if lo == hi && lo * lo == 16129 { ci[idx] = (lo > 0) as u16; } else { cg.extend_from_slice(&brick); n += 1; ci[idx] = (n + 1) as u16; }
        } } }
        // coarse field: half-res density divided by its gradient, in metres
        let mut dh = vec![0f32; HX * HY * HZ];
        for z in 0..HZ { for y in 0..HY { for x in 0..HX {
            let g0 = (x * 2) + NX * ((y * 2) + NY * (z * 2));
            let s = |o: usize| if o < d.len() { d[o] } else { -1.0 };
            dh[x + HX * (y + HY * z)] = (s(g0) + s(g0 + 1) + s(g0 + NX) + s(g0 + NX + 1) + s(g0 + NX * NY) + s(g0 + NX * NY + 1) + s(g0 + NX * NY + NX) + s(g0 + NX * NY + NX + 1)) * 0.125;
        } } }
        let mut coarse = vec![0f32; HX * HY * HZ];
        for z in 0..HZ { for y in 0..HY { for x in 0..HX {
            let at = |x: usize, y: usize, z: usize| dh[x.min(HX - 1) + HX * (y.min(HY - 1) + HY * z.min(HZ - 1))];
            let gx = at(x + 1, y, z) - at(x.saturating_sub(1), y, z); let gy = at(x, y + 1, z) - at(x, y.saturating_sub(1), z); let gz = at(x, y, z + 1) - at(x, y, z.saturating_sub(1));
            let l = ((gx * gx + gy * gy + gz * gz).sqrt() / (2.0 * 2.0 * VS as f32)).max(0.25);
            coarse[x + HX * (y + HY * z)] = (at(x, y, z) / l).clamp(-8.0, 8.0);
        } } }
        Field { ci, cg, coarse, caps: vec![], kelp: vec![], kgrid: vec![vec![]; KG * KG] }
    }
    fn cv(&self, x: i32, y: i32, z: i32) -> f64 {
        if x < 0 || y < 0 || z < 0 || x >= (CN * 4) as i32 || y >= NY as i32 || z >= (CN * 4) as i32 { return -127.0; }
        let (x, y, z) = (x as usize, y as usize, z as usize);
        let b = self.ci[(x >> 2) + CN * ((y >> 2) + NY / 4 * (z >> 2))] as usize;
        if b > 1 { self.cg[b * 64 - 128 + (x & 3) + 4 * (y & 3) + 16 * (z & 3)] as f64 } else { b as f64 * 254.0 - 127.0 }
    }
    /// distance to the rock in metres (< 0 in water), clamped to ±1 m
    pub fn fine(&self, px: f64, py: f64, pz: f64) -> f64 {
        let x = (px - OX) / VS - CO as f64; let y = (py - OY) / VS; let z = (pz - OZ) / VS - CO as f64;
        let (fx, fy, fz) = (x.floor(), y.floor(), z.floor());
        let (ix, iy, iz) = (fx as i32, fy as i32, fz as i32); let (u, v, w) = (x - fx, y - fy, z - fz);
        let a = self.cv(ix, iy, iz); let b = self.cv(ix, iy + 1, iz); let c = self.cv(ix, iy, iz + 1); let d = self.cv(ix, iy + 1, iz + 1);
        let p = a + (self.cv(ix + 1, iy, iz) - a) * u; let q = b + (self.cv(ix + 1, iy + 1, iz) - b) * u;
        let r = c + (self.cv(ix + 1, iy, iz + 1) - c) * u; let s = d + (self.cv(ix + 1, iy + 1, iz + 1) - d) * u;
        let e = p + (q - p) * v;
        (e + (r + (s - r) * v - e) * w) / 127.0
    }
    /// smooth distance estimate in metres (< 0 in water), clamped to ±8 m. Outside the world: water up to the
    /// surface, rock below the floor.
    pub fn coarse(&self, px: f64, py: f64, pz: f64) -> f64 {
        let x = clamp((px - OX) / (VS * 2.0) - 0.25, 0.0, HX as f64 - 1.001);
        let y = clamp((py - OY) / (VS * 2.0) - 0.25, 0.0, HY as f64 - 1.001);
        let z = clamp((pz - OZ) / (VS * 2.0) - 0.25, 0.0, HZ as f64 - 1.001);
        Sampler3 { d: &self.coarse, nx: HX, ny: HY, nz: HZ }.at(x, y, z).max(py + 1.5) // the water surface counts as a wall
    }
    pub fn coarse_grad(&self, x: f64, y: f64, z: f64) -> V3 {
        let e = 0.9;
        let g = [self.coarse(x + e, y, z) - self.coarse(x - e, y, z), self.coarse(x, y + e, z) - self.coarse(x, y - e, z), self.coarse(x, y, z + e) - self.coarse(x, y, z - e)];
        let l = nz1(hypot3(g[0], g[1], g[2]));
        [g[0] / l, g[1] / l, g[2] / l]
    }
    /// best distance estimate: the exact fine field near the rock, the coarse one further out
    pub fn set_obstacles(&mut self, caps: Vec<(V3, V3, f64)>, kelp: Vec<[f32; 4]>) {
        for (i, k) in kelp.iter().enumerate() {
            let cx = (((k[0] as f64 + 200.0) / KW) as usize).min(KG - 1); let cz = (((k[1] as f64 + 200.0) / KW) as usize).min(KG - 1);
            self.kgrid[cx + KG * cz].push(i as u32);
        }
        self.caps = caps; self.kelp = kelp;
    }
    fn caps_dist(&self, x: f64, y: f64, z: f64) -> f64 {
        let mut d = -1e9f64;
        for (a, b, r) in &self.caps {
            let pa = [x - a[0], y - a[1], z - a[2]]; let ba = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
            let h = clamp((pa[0] * ba[0] + pa[1] * ba[1] + pa[2] * ba[2]) / (ba[0] * ba[0] + ba[1] * ba[1] + ba[2] * ba[2]), 0.0, 1.0);
            d = d.max(r - hypot3(pa[0] - ba[0] * h, pa[1] - ba[1] * h, pa[2] - ba[2] * h));
        }
        d
    }
    /// calls f for each kelp column near (x, z) within r
    pub fn kelp_near(&self, x: f64, z: f64, r: f64, mut f: impl FnMut(&[f32; 4])) {
        let c0 = (((x - r + 200.0) / KW).floor().max(0.0) as usize).min(KG - 1); let c1 = (((x + r + 200.0) / KW).floor().max(0.0) as usize).min(KG - 1);
        let r0 = (((z - r + 200.0) / KW).floor().max(0.0) as usize).min(KG - 1); let r1 = (((z + r + 200.0) / KW).floor().max(0.0) as usize).min(KG - 1);
        for cz in r0..=r1 { for cx in c0..=c1 { for &i in &self.kgrid[cx + KG * cz] { let k = &self.kelp[i as usize]; if hypot2(k[0] as f64 - x, k[1] as f64 - z) < r { f(k); } } } }
    }
    /// distance from p to the nearest kelp stem (metres, horizontal), up to r
    pub fn kelp_dist(&self, p: V3, r: f64) -> f64 {
        let mut d = r;
        self.kelp_near(p[0], p[2], r, |k| if p[1] > k[2] as f64 - 0.5 && p[1] < k[3] as f64 + 0.5 { d = d.min(hypot2(k[0] as f64 - p[0], k[1] as f64 - p[2])); });
        d
    }
    /// is the line a-b clear of kelp stems (with their leaves, ~r around each)?
    pub fn clear_of_kelp(&self, a: V3, b: V3, r: f64) -> bool {
        let d = [b[0] - a[0], b[1] - a[1], b[2] - a[2]]; let l = hypot3(d[0], d[1], d[2]);
        let n = (l / 1.5).ceil().max(1.0) as usize;
        for i in 1..n { let t = i as f64 / n as f64; if self.kelp_dist([a[0] + d[0] * t, a[1] + d[1] * t, a[2] + d[2] * t], r) < r { return false; } }
        true
    }
    pub fn dist(&self, x: f64, y: f64, z: f64) -> f64 {
        let c = self.caps_dist(x, y, z);
        if c > -1.0 { return c.max(self.fine(x, y, z)); }
        let f = self.fine(x, y, z).max(y + 1.5).max(c);
        if f <= -0.99 { self.coarse(x, y, z).min(-0.99) } else if f >= 0.99 { self.coarse(x, y, z).max(0.99) } else { f }
    }
    pub fn grad(&self, x: f64, y: f64, z: f64) -> V3 {
        let e = 0.375;
        let g = [self.dist(x + e, y, z) - self.dist(x - e, y, z), self.dist(x, y + e, z) - self.dist(x, y - e, z), self.dist(x, y, z + e) - self.dist(x, y, z - e)];
        let l = hypot3(g[0], g[1], g[2]);
        if l < 1e-4 { self.coarse_grad(x, y, z) } else { [g[0] / l, g[1] / l, g[2] / l] }
    }
    /// Moves a sphere of radius cr at p by m in steps shorter than its radius, pushing it back out of the rock
    /// after each step and dropping velocity v into the rock, so it slides along (the original camera collision).
    pub fn collide(&self, p: &mut V3, m: V3, v: &mut V3, cr: f64) {
        let n = (hypot3(m[0], m[1], m[2]) / (cr * 0.8).max(0.2)).floor() as usize + 1;
        for _ in 0..n {
            for k in 0..3 { p[k] += m[k] / n as f64; }
            for _ in 0..3 {
                let s = self.dist(p[0], p[1], p[2]) + cr;
                if s <= 0.0 { break; }
                let g = self.grad(p[0], p[1], p[2]);
                let vn = v[0] * g[0] + v[1] * g[1] + v[2] * g[2];
                for k in 0..3 { p[k] -= g[k] * s; }
                if vn > 0.0 { for k in 0..3 { v[k] -= g[k] * vn; } }
            }
        }
    }
    /// is the straight line a-b open water (checked against the coarse field, with a margin)?
    pub fn clear_line(&self, a: V3, b: V3, margin: f64) -> bool {
        let d = [b[0] - a[0], b[1] - a[1], b[2] - a[2]]; let l = hypot3(d[0], d[1], d[2]);
        let n = (l / 0.7).ceil().max(1.0) as usize;
        for i in 0..=n {
            let t = i as f64 / n as f64;
            let (x, y, z) = (a[0] + d[0] * t, a[1] + d[1] * t, a[2] + d[2] * t);
            if self.coarse(x, y, z).max(self.caps_dist(x, y, z)) > -margin { return false; }
        }
        true
    }
}

// ---------------------------------------------------------------- terrain mesh from the density grid
pub struct Terrain { pub vb: Vec<u8>, pub ib: Vec<u32>, pub nv: usize, pub wp: Vec<f32>, pub wn: Vec<f32>, pub start: Vec<u32>, pub lo: Vec<f32>, pub hi: Vec<f32> }
pub fn build_terrain(d: &[f32]) -> Terrain {
    let sn = surface_nets(d, NX, NY, NZ); let nv = sn.pos.len() / 3; let gp = &sn.pos;
    let smp = Sampler3 { d, nx: NX, ny: NY, nz: NZ };
    let mut vb = vec![0u8; nv * 16]; let mut wp = vec![0f32; nv * 3]; let mut wn = vec![0f32; nv * 3];
    for k in 0..nv {
        let (x, y, z, e) = (gp[k * 3] as f64, gp[k * 3 + 1] as f64, gp[k * 3 + 2] as f64, 0.8);
        let (mut nx, mut ny, mut nz) = (smp.at(x - e, y, z) - smp.at(x + e, y, z), smp.at(x, y - e, z) - smp.at(x, y + e, z), smp.at(x, y, z - e) - smp.at(x, y, z + e));
        let l = nz1(hypot3(nx, ny, nz)); nx /= l; ny /= l; nz /= l;
        let (xx, yy, zz) = ((OX + x * VS) as f32, (OY + y * VS) as f32, (OZ + z * VS) as f32);
        wp[k * 3] = xx; wp[k * 3 + 1] = yy; wp[k * 3 + 2] = zz;
        wn[k * 3] = nx as f32; wn[k * 3 + 1] = ny as f32; wn[k * 3 + 2] = nz as f32;
        write_vert(&mut vb, k, [xx, yy, zz], [nx, ny, nz, 1.0]);
    }
    // bucket quads into the XZ culling grid
    let nq = sn.idx.len() / 6; let mut key = vec![0u16; nq]; let mut cnt = vec![0u32; CELLS * CELLS + 1];
    let mut lo = vec![1e9f32; CELLS * CELLS]; let mut hi = vec![-1e9f32; CELLS * CELLS];
    for q in 0..nq {
        let a = sn.idx[q * 6] as usize * 3;
        let c = cell_of(wp[a] as f64, wp[a + 2] as f64);
        key[q] = c as u16; cnt[c + 1] += 1;
        for j in 0..6 { let y = wp[sn.idx[q * 6 + j] as usize * 3 + 1]; lo[c] = lo[c].min(y); hi[c] = hi[c].max(y); }
    }
    for c in 0..CELLS * CELLS { cnt[c + 1] += cnt[c]; }
    let mut ib = vec![0u32; sn.idx.len()]; let mut fill = cnt.clone();
    for q in 0..nq { let o = fill[key[q] as usize] as usize * 6; fill[key[q] as usize] += 1; ib[o..o + 6].copy_from_slice(&sn.idx[q * 6..q * 6 + 6]); }
    Terrain { vb, ib, nv, wp, wn, start: cnt, lo, hi }
}
pub fn cell_of(x: f64, z: f64) -> usize {
    clamp(((x - OX) / CELLW).floor(), 0.0, CELLS as f64 - 1.0) as usize + CELLS * clamp(((z - OZ) / CELLW).floor(), 0.0, CELLS as f64 - 1.0) as usize
}

/// a cell-sorted instance buffer
#[derive(Default)]
pub struct Pack { pub buf: Vec<u8>, pub n: u32, pub start: Vec<u32>, pub lo: Vec<f32>, pub hi: Vec<f32> }

pub struct World {
    pub glints: Vec<f32>, pub chunks: Vec<Pack>, pub props: Vec<Pack>, pub big: Pack, pub big_f: Pack, pub small: Pack,
    pub solids: Vec<f32>, pub ranges: [(u32, u32); 12], pub bubbles: Vec<f32>, pub vol_d: Vec<u8>, pub gm: Vec<u8>,
    pub fish: [Vec<f32>; 3], pub mantas: Vec<f32>, pub snow: Vec<f32>, pub pose_y: Vec<f64>,
    /// kelp columns (x, z, base y, top y) and the set pieces' collision capsules, for the Field
    pub kelp: Vec<[f32; 4]>, pub caps: Vec<(V3, V3, f64)>,
}

const NC: usize = CELLS * CELLS;
struct Gen<'a> {
    d: &'a [f32], t: &'a Terrain, fly: &'a [V3], poses: &'a [[f64; 6]], lv: u32, thin: u32, nth: u32,
    big: Vec<Vec<f32>>, small: Vec<Vec<f32>>, glints: Vec<f32>, solids: Vec<Vec<f32>>, props: Vec<Vec<Vec<f32>>>, chunks: Vec<Vec<Vec<f32>>>,
    kelp_abs: Vec<f32>, occ: std::collections::HashSet<i64>, fl: Vec<f64>, kelp: Vec<[f32; 4]>,
}
#[derive(Clone, Copy)] enum L { Big, Small }
impl Gen<'_> {
    fn gi(x: usize, y: usize, z: usize) -> usize { x + NX * (y + NY * z) }
    fn dw(&self, x: f64, y: f64, z: f64) -> f64 { Sampler3 { d: self.d, nx: NX, ny: NY, nz: NZ }.at((x - OX) / VS, (y - OY) / VS, (z - OZ) / VS) }
    fn leaf(&mut self, l: L, x: f64, y: f64, z: f64, len: f64, yaw: f64, tilt: f64, wid: f64, ha: f64, ty: u32, s1: f64, s2: f64, bend: f64) {
        if ty == 1 || { let k = self.nth; self.nth += 1; k % self.thin == 0 } {
            let w = if ty == 1 { wid } else { wid * (1.0 + self.lv as f64 / 2.0) };
            let v = match l { L::Big => &mut self.big, L::Small => &mut self.small };
            v[cell_of(x, z)].extend_from_slice(&[x as f32, y as f32, z as f32, len as f32, yaw as f32, tilt as f32, w as f32, ha as f32, ty as f32, s1 as f32, s2 as f32, bend as f32]);
        }
    }
    fn glint(&mut self, a: [f64; 8]) { let k = self.nth; self.nth += 1; if k % self.thin == 0 { self.glints.extend(a.iter().map(|&v| v as f32)); } }
    fn solid(&mut self, mesh: usize, x: f64, y: f64, z: f64, sc: f64, q: Q, mat: f64, seed: f64, ts: f64, bob: f64) {
        let v = [x, y, z, sc, q[0], q[1], q[2], q[3], mat, seed, ts, bob].map(|v| v as f32);
        if (4..10).contains(&mesh) { self.props[mesh - 4][cell_of(x, z)].extend_from_slice(&v); } else { self.solids[mesh].extend_from_slice(&v); }
    }
    fn rel(&self, i: usize, f: f64, r: f64) -> [f64; 2] {
        let p = self.poses[i]; let (mut fx, mut fz) = (p[3] - p[0], p[5] - p[2]); let l = hypot2(fx, fz); fx /= l; fz /= l;
        [p[0] + fx * f - fz * r, p[2] + fz * f + fx * r]
    }
    /// (floor y, open water above it) pairs from the top down
    fn floors_at(&mut self, x: f64, z: f64) -> Vec<f64> {
        let ix = clamp(((x - OX) / VS).round(), 1.0, NX as f64 - 2.0) as usize; let iz = clamp(((z - OZ) / VS).round(), 1.0, NZ as f64 - 2.0) as usize;
        self.fl.clear(); let mut ceil = 3.0;
        for gy in (0..=NY - 2).rev() {
            let d = self.d[Self::gi(ix, gy, iz)] as f64; let da = self.d[Self::gi(ix, gy + 1, iz)] as f64;
            if da <= 0.0 && d > 0.0 { let y = OY + (gy as f64 + d / (d - da)) * VS; self.fl.push(y); self.fl.push(f64::min(ceil, 0.0) - y); }
            else if da > 0.0 && d <= 0.0 { ceil = OY + (gy as f64 + 0.5) * VS; }
        }
        self.fl.clone()
    }
    fn grad(&self, x: f64, y: f64, z: f64) -> V3 {
        let e = 0.6;
        let n = [self.dw(x - e, y, z) - self.dw(x + e, y, z), self.dw(x, y - e, z) - self.dw(x, y + e, z), self.dw(x, y, z - e) - self.dw(x, y, z + e)];
        let l = nz1(hypot3(n[0], n[1], n[2])); [n[0] / l, n[1] / l, n[2] / l]
    }
    /// march from outside along -n to the surface: position + normal
    fn snap(&self, x: f64, y: f64, z: f64, n: V3) -> Option<[f64; 6]> {
        let (mut px, mut py, mut pz) = (x + n[0] * 2.5, y + n[1] * 2.5, z + n[2] * 2.5); let mut prev = self.dw(px, py, pz);
        if prev > 0.0 { return None; }
        for _ in 0..24 {
            let (qx, qy, qz) = (px - n[0] * 0.25, py - n[1] * 0.25, pz - n[2] * 0.25); let d = self.dw(qx, qy, qz);
            if d > 0.0 { let t = prev / (prev - d); let p = [mix(px, qx, t), mix(py, qy, t), mix(pz, qz, t)]; let g = self.grad(p[0], p[1], p[2]); return Some([p[0], p[1], p[2], g[0], g[1], g[2]]); }
            px = qx; py = qy; pz = qz; prev = d;
        }
        None
    }
    fn pick_v(&self) -> usize { ((rnd() * self.t.nv as f64) as usize) * 3 }
    fn wp(&self, k: usize) -> V3 { [self.t.wp[k] as f64, self.t.wp[k + 1] as f64, self.t.wp[k + 2] as f64] }
    fn wn(&self, k: usize) -> V3 { [self.t.wn[k] as f64, self.t.wn[k + 1] as f64, self.t.wn[k + 2] as f64] }
    fn near_fly(&self, x: f64, y: f64, z: f64, r: f64) -> bool { self.fly.iter().any(|p| hypot2(x - p[0], z - p[2]) < r && y < p[1] + 3.0 && y > p[1] - 13.0) }
    fn near_pose(&self, x: f64, z: f64, r: f64) -> bool { self.poses.iter().any(|p| hypot2(x - p[0], z - p[2]) < r) }
    fn gy(&mut self, x: f64, z: f64) -> f64 { let f = self.floors_at(x, z); if f.is_empty() { -45.0 } else { f[f.len() - 2] } }

    // --- kelp columns
    fn column(&mut self, x: f64, y: f64, z: f64, h_: f64, kind: u32, dens: f64) {
        self.kelp.push([x as f32, z as f32, y as f32, (y + h_) as f32]);
        let strands = 2 + (rnd() < 0.6) as usize + (rnd() < 0.3) as usize; let ph = rnd() * TAU; let tw = rr(0.1, 0.28); let sr = rr(0.2, 0.55); let sc = rr(0.75, 1.25);
        let la = rnd() * TAU; let lm = rr(0.0, 0.11); let full = rr(0.55, 1.3) * dens;
        let stem = |s: usize, h: f64| { let a = ph + s as f64 * TAU / strands as f64 + h * tw; let r = sr * (1.0 - 0.6 * h / h_); let ln = lm * h * h / h_; [x + a.cos() * r + la.cos() * ln, z + a.sin() * r + la.sin() * ln, a] };
        for s in 0..strands { let mut h = 0.0; while h < h_ - 1.0 { let p = stem(s, h); let (a, b) = (rnd(), rnd()); self.leaf(L::Big, p[0], y + h, p[1], 2.1, p[2] + PI / 2.0, 0.1, 0.17, h, 1, a, b, 0.3); h += 1.7; } }
        let n = (h_ * 30.0 * full).floor() as usize;
        for _ in 0..n {
            let t = rnd().powf(0.9); let h = t * h_; let p = stem((rnd() * strands as f64) as usize, h);
            let ll = ((0.55 + 0.75 * rnd() * rnd()) * (1.0 - 0.5 * t * t) * sc + 0.25) * if kind == 1 { 1.25 } else { 1.15 };
            let tilt = { let a = rr(0.4, 1.0); let b = rr(0.15, 0.45); mix(a, b, t) };
            let ty = if kind == 1 { if rnd() < 0.8 { 3 } else { 4 } } else if kind == 2 { if rnd() < 0.5 { 4 } else { 0 } } else { 0 };
            let yaw = rnd() * TAU; let wid = ll * if kind == 1 { rr(0.09, 0.13) } else { rr(0.15, 0.21) } + 0.03;
            let (s1, s2, bend) = (rnd(), rnd(), rr(0.38, 0.95));
            self.leaf(L::Big, p[0], y + h, p[1], ll, yaw, tilt, wid, h, ty, s1, s2, bend);
        }
        let mut i = (h_ * if kind == 1 { 16.0 } else { 13.0 }).floor() as i32;
        while i > 0 {
            let h = rnd() * h_; let a = rnd() * TAU; let r = rr(0.15, 0.8) * (1.0 - 0.5 * h / h_); let ln = lm * h * h / h_;
            let s = rnd(); let c = if kind == 1 { rnd() } else { rnd() * 0.6 }; let sz = rr(0.018, 0.05);
            self.glint([x + a.cos() * r + la.cos() * ln, y + h, z + a.sin() * r + la.sin() * ln, h, s, c, sz, 0.0]);
            i -= 1;
        }
        // light absorption footprint for the skylight bake
        let hx = ((x - OX) / VS / 2.0).round() as i64; let hz = ((z - OZ) / VS / 2.0).round() as i64;
        let mut hy = ((y - OY) / VS / 2.0).floor() as i64;
        while hy <= ((y + h_ - OY) / VS / 2.0).floor() as i64 {
            if hy >= 0 && hy < HY as i64 && hx > 0 && hx < HX as i64 - 1 && hz > 0 && hz < HZ as i64 - 1 { self.kelp_abs[hx as usize + HX * (hy as usize + HY * hz as usize)] += 0.17; }
            hy += 1;
        }
    }
    fn try_column(&mut self, x: f64, z: f64, kind: u32, force: bool, dens: f64) {
        let key = (x / 2.3).floor() as i64 * 4096 + (z / 2.3).floor() as i64;
        if self.occ.contains(&key) { return; }
        let f = self.floors_at(x, z);
        let c: Vec<usize> = (0..f.len()).step_by(2).filter(|&i| f[i + 1] > 9.0).collect();
        if c.is_empty() { return; }
        let i = c[(rnd() * c.len() as f64) as usize]; let y = f[i];
        if self.grad(x, y, z)[1] < 0.55 { return; }
        if !force && self.fly.iter().any(|p| hypot2(x - p[0], z - p[2]) < 5.0) { return; }
        self.occ.insert(key);
        let hh = if force { rr(30.0, 42.0) } else { rr(15.0, 40.0) };
        let h = f64::min(f[i + 1] - rr(0.8, 5.0), hh);
        self.column(x, y - 0.3, z, h, kind, dens);
    }
    fn cluster(&mut self, x: f64, y: f64, z: f64, n: usize, ty: u32, lmin: f64, lmax: f64, wmin: f64, wmax: f64, tmax: f64, sway: f64) {
        for _ in 0..n {
            let a = rnd() * TAU; let r = rnd() * 0.5; let ll = rr(lmin, lmax);
            let tilt = rr(0.05, tmax); let w = rr(wmin, wmax); let (s1, s2, b) = (rnd(), rnd(), rr(0.3, 0.8));
            self.leaf(L::Big, x + a.cos() * r, y - 0.15, z + a.sin() * r, ll, a, tilt, w, sway, ty, s1, s2, b);
        }
    }
}

fn kelp_dens(x: f64, z: f64) -> f64 {
    const KELPZ: [[f64; 4]; 14] = [[-32., -5., 13., 0.85], [0., -86., 46., 1.], [72., -26., 27., 0.9], [-60., -4., 11., 0.8], [0., -36., 12., 0.55], [-17., -8., 6., 0.8], [15., -8., 6., 0.8], [-4., 120., 20., 0.4], [-45., -63., 15., 0.9], [62., -26., 12., 0.9], [-100., 70., 30., 0.8], [100., 60., 30., 0.8], [-95., -90., 32., 0.8], [105., -85., 30., 0.7]];
    const NOKELP: [[f64; 3]; 14] = [[-5., -106.5, 8.], [4.6, -109.4, 10.], [14.2, -112.3, 12.], [-26., -100., 6.], [-20.5, -101.7, 4.5], [-14.8, -104.5, 6.], [-7.5, -107.8, 7.], [4., 112., 11.], [2., 4., 15.], [42., 22., 12.], [6., 78., 19.], [-58., -16., 8.], [9., -66., 9.], [8., -76., 8.]];
    for p in NOKELP { if hypot2(x - p[0], z - p[1]) < p[2] { return 0.0; } }
    let mut d = sstep(0.52, 0.64, fbm(x * 0.022, 4.0, z * 0.022, 3)) * 0.5;
    for p in KELPZ { d = d.max(p[3] * sstep(p[2], p[2] * 0.75, hypot2(x - p[0], z - p[1]))); }
    d
}
const HOT: [[f64; 3]; 7] = [[0., 5., 30.], [42., 30., 24.], [6., 82., 34.], [-58., 10., 24.], [4., -88., 30.], [-36., 44., 18.], [-46., -60., 20.]];
fn hot(x: f64, z: f64) -> f64 { let mut h: f64 = 0.4; for p in HOT { h = h.max(sstep(p[2], p[2] * 0.4, hypot2(x - p[0], z - p[1]))); } h }

/// fish schools: cx, cy, cz, radius, count, size, direction, phase (or random), species
/// species 0: the original silver schoolers, 1: blue-and-yellow reef fish, 2: big striped jacks
pub const SCHOOLS: [[f64; 9]; 17] = [
    [-2., -36., -10., 15., 520., 0.3, 1., 0.945, 0.], [8., -30., -70., 22., 420., 0.32, -1., -1., 0.], [48., -30., 14., 18., 360., 0.3, 1., -1., 0.], [-58., -30., -8., 11., 420., 0.28, -1., 0.3, 0.], [8., -36., 84., 16., 320., 0.28, 1., -1., 0.],
    [-90., -28., -70., 25., 300., 0.3, 1., -1., 0.], [95., -30., 70., 25., 300., 0.3, -1., -1., 0.], [0., -22., 20., 30., 380., 0.34, -1., -1., 0.], [-30., -30., -100., 18., 300., 0.3, 1., -1., 0.], [16., -36., 30., 12., 300., 0.3, 1., -1., 0.], [-48., -34., -58., 12., 300., 0.3, -1., -1., 0.], [-45., -35., 38., 14., 320., 0.3, 1., -1., 0.],
    // new: reef fish hugging the sponge gardens and the grotto, jacks cruising the open canyon
    [-64., -38., 0., 7., 160., 0.22, 1., -1., 1.], [10., -38., 76., 9., 180., 0.22, -1., -1., 1.], [44., -36., 16., 7., 120., 0.22, 1., -1., 1.],
    [0., -30., -45., 20., 60., 0.75, 1., -1., 2.], [-10., -26., 40., 24., 50., 0.8, -1., -1., 2.],
];

pub fn make_world(d: &[f32], t: &Terrain, fly: &[V3], poses: &[[f64; 6]], lv: u32) -> World {
    let mut g = Gen { d, t, fly, poses, lv, thin: 1 << lv, nth: 0, big: vec![vec![]; NC], small: vec![vec![]; NC], glints: vec![], solids: vec![vec![]; 12], props: vec![vec![vec![]; NC]; 6], chunks: vec![vec![vec![]; NC]; NCHUNK], kelp_abs: vec![0f32; HX * HY * HZ], occ: Default::default(), fl: vec![], kelp: vec![] };

    rseed(1);
    // hand-placed columns framing the viewpoints: x, z, kind, density
    let hand: Vec<([f64; 2], u32, f64)> = {
        let r = |i, f, s| g.rel(i, f, s);
        vec![(r(0, 11., -8.5), 0, 1.), (r(0, 16., -10.5), 0, 1.), (r(0, 21., -9.), 0, 1.), (r(0, 9., 7.), 0, 1.), (r(0, 14., 9.5), 0, 1.), (r(0, 19., 8.), 0, 1.), (r(0, 26., 6.), 0, 1.),
            (r(5, 6.5, 5.2), 1, 1.), (r(5, 12., 7.), 1, 1.), (r(5, 15., -8.), 0, 1.), (r(5, 22., -5.), 1, 1.), (r(3, 12., -5.5), 0, 1.), (r(3, 16., 5.5), 0, 1.), (r(3, 20., -4.5), 0, 1.), (r(3, 25., 6.), 0, 1.), (r(3, 13., 8.), 0, 1.), (r(3, 30., -7.), 0, 1.), ([24., 62.], 1, 1.), ([-6., 96.], 1, 1.), ([20., 96.], 2, 1.), ([-10., 70.], 0, 1.), ([16., 52.], 1, 1.),
            (r(4, 15., -9.), 0, 1.2), (r(4, 19., -13.), 0, 1.2), (r(4, 23., -8.5), 0, 1.2), (r(4, 25., -15.), 0, 1.2),
            (r(4, 25., 0.5), 2, 1.2), (r(4, 27., -3.), 2, 1.2), (r(4, 28., 3.5), 2, 1.2), (r(4, 31., 0.), 0, 1.3), (r(4, 33., -5.), 0, 1.3), (r(4, 33., 5.), 0, 1.3), (r(4, 21., 9.), 0, 1.2), (r(4, 27., 11.), 0, 1.2),
            (r(2, 6.5, -5.5), 0, 1.5), (r(2, 9.5, -8.), 0, 1.3), (r(2, 8., 6.), 0, 1.4), (r(2, 12., 3.6), 0, 1.3), (r(2, 17., -2.5), 0, 1.2), (r(2, 21., 4.), 0, 1.2), (r(2, 12., -4.5), 2, 1.2), (r(2, 27., -9.), 0, 1.2), (r(2, 30., 8.5), 0, 1.2), (r(2, 37., -3.), 0, 1.1),
            (r(2, 13.5, 6.5), 0, 1.3), (r(2, 14.5, -7.), 0, 1.3), (r(2, 6.5, 6.5), 0, 1.3), (r(2, 9., -8.), 2, 1.3)]
    };
    for (p, k, dn) in hand { g.try_column(p[0], p[1], k, true, dn); }
    // random columns across the world
    rseed(2);
    for _ in 0..15000 {
        let a = rnd() * TAU; let r = rnd().sqrt() * 170.0; let (x, z) = (a.cos() * r, a.sin() * r);
        if rnd() < kelp_dens(x, z) { let kind = if zone_p(x, z) > 0.5 && rnd() < 0.5 { 1 } else if rnd() < 0.03 { 2 } else { 0 }; g.try_column(x, z, kind, false, 1.0); }
    }
    // --- clusters of ribbon leaves on the ground
    rseed(3);
    for _ in 0..40000 {
        let k = g.pick_v(); let [x, y, z] = g.wp(k); let ny = g.wn(k)[1];
        if hypot2(x, z) > 172.0 || y > -6.0 { continue; }
        let h = hot(x, z); let zp = zone_p(x, z); let u = rnd();
        if g.near_fly(x, y, z, 4.0) || hypot2(x + 1.0, z - 2.0) < 9.0 || g.near_pose(x, z, 8.0) { continue; }
        if ny > 0.75 && u < 0.10 * h { let n = 8 + (rnd() * 14.0) as usize; g.cluster(x, y, z, n, 2, 1.0, 3.0, 0.13, 0.26, 0.6, 2.5); }
        else if ny > 0.6 && u < 0.10 * h + 0.07 * zp.max(h * 0.4) { let n = 10 + (rnd() * 18.0) as usize; let ty = if rnd() < 0.8 { 3 } else { 4 }; g.cluster(x, y, z, n, ty, 0.9, 2.6, 0.14, 0.28, 0.9, 1.5); }
        else if ny < -0.35 && u < 0.5 * zp.max(0.2) { // hanging fronds
            let n = 12 + (rnd() * 20.0) as usize;
            for _ in 0..n { let (a, b) = (rr(-0.6, 0.6), rr(-0.6, 0.6)); let l = rr(0.8, 3.2); let yaw = rnd() * TAU; let tl = PI - rr(0.0, 0.4); let w = rr(0.08, 0.18); let ty = if zp > 0.3 || rnd() < 0.5 { 6 } else { 0 }; let (s1, s2, bd) = (rnd(), rnd(), rr(0.3, 0.6)); g.leaf(L::Big, x + a, y + 0.2, z + b, l, yaw, tl, w, 0.0, ty, s1, s2, bd); }
        } else if ny > -0.2 && ny < 0.8 && u < 0.07 { // wall tufts
            let wn = g.wn(k); let yaw = wn[2].atan2(wn[0]); let n = 5 + (rnd() * 7.0) as usize;
            for _ in 0..n { let l = rr(0.5, 1.4); let yw = yaw + rr(-0.9, 0.9); let tl = rr(0.3, 1.1); let w = rr(0.06, 0.12); let ty = if zp > 0.4 && rnd() < 0.6 { 3 } else { 0 }; let (s1, s2, bd) = (rnd(), rnd(), rr(0.2, 0.6)); g.leaf(L::Big, x, y, z, l, yw, tl, w, 0.5, ty, s1, s2, bd); }
        }
    }
    // --- ground cover: dense carpets of small tufts in the magenta patches
    rseed(4);
    for p in MAGENTA {
        let mut i = p[2] * p[2] * PI * 45.0;
        while i > 0.0 {
            i -= 1.0;
            let a = rnd() * TAU; let r = rnd().sqrt() * p[2]; let (x, z) = (p[0] + a.cos() * r, p[1] + a.sin() * r);
            if rnd() > mag_at(x, z) + 0.12 || g.near_pose(x, z, 3.0) { continue; }
            let f = g.floors_at(x, z); if f.is_empty() { continue; }
            let y = f[f.len() - 2]; if g.grad(x, y, z)[1] < 0.7 { continue; }
            let nl = 2 + (rnd() * 3.0) as usize; let yw = rnd() * TAU;
            for j in 0..nl { let l = rr(0.08, 0.26); let yy = yw + j as f64 * TAU / nl as f64 + rr(-0.4, 0.4); let tl = rr(0.15, 0.85); let w = rr(0.035, 0.07); let (s1, s2, bd) = (rnd(), rnd(), rr(0.3, 0.7)); g.leaf(L::Small, x, y - 0.03, z, l, yy, tl, w, 0.0, 5, s1, s2, bd); }
            if rnd() < 0.3 { for _ in 0..2 { let (a, b) = (rr(-0.2, 0.2), rr(-0.2, 0.2)); let l = rr(0.2, 0.55); let yy = rnd() * TAU; let tl = rr(0.05, 0.6); let w = rr(0.03, 0.06); let (s1, s2, bd) = (rnd(), rnd(), rr(0.3, 0.8)); g.leaf(L::Small, x + a, y - 0.04, z + b, l, yy, tl, w, 0.0, 7, s1, s2, bd); } }
            if rnd() < 0.3 { let yy = y + rr(0.08, 0.22); let s = rnd(); let c = 0.82 + rnd() * 0.18; let sz = rr(0.01, 0.02); g.glint([x, yy, z, 0.0, s, c, sz, 0.0]); }
        }
    }
    rseed(5);
    for _ in 0..620000 {
        let k = g.pick_v(); let ny = g.wn(k)[1];
        if ny < 0.72 { continue; }
        let x = g.t.wp[k] as f64 + rr(-0.45, 0.45); let z = g.t.wp[k + 2] as f64 + rr(-0.45, 0.45); let y = g.t.wp[k + 1] as f64;
        if hypot2(x, z) > 170.0 || g.near_pose(x, z, 3.0) { continue; }
        let m = mag_at(x, z); let zp = zone_p(x, z); let u = rnd();
        if m > 0.3 && u < m * 0.9 { for _ in 0..4 { let (a, b) = (rr(-0.1, 0.1), rr(-0.1, 0.1)); let l = rr(0.12, 0.34); let yy = rnd() * TAU; let tl = rr(0.05, 0.75); let w = rr(0.04, 0.08); let (s1, s2, bd) = (rnd(), rnd(), rr(0.3, 0.7)); g.leaf(L::Small, x + a, y - 0.05, z + b, l, yy, tl, w, 0.0, 5, s1, s2, bd); } }
        else if u < m * 0.9 + 0.5 * zp.max(hot(x, z)) * sstep(0.42, 0.6, fbm(x * 0.09, 9.0, z * 0.09, 2)) { for _ in 0..3 { let (a, b) = (rr(-0.1, 0.1), rr(-0.1, 0.1)); let l = rr(0.25, 0.7); let yy = rnd() * TAU; let tl = rr(0.1, 0.8); let w = rr(0.05, 0.1); let (s1, s2, bd) = (rnd(), rnd(), rr(0.3, 0.8)); g.leaf(L::Small, x + a, y - 0.05, z + b, l, yy, tl, w, 0.0, 7, s1, s2, bd); } }
    }

    // --- sponges and coral (clustered on rock)
    let mut spots: Vec<V3> = vec![];
    const ZS: [[f64; 4]; 5] = [[-58., 8., 30., 1.], [0., 0., 30., 0.7], [6., 84., 36., 0.9], [4., -88., 22., 0.6], [44., 20., 24., 0.5]];
    let zw = |x: f64, z: f64| { let mut h: f64 = 0.45; for p in ZS { h = h.max(p[3] * sstep(p[2], p[2] * 0.4, hypot2(x - p[0], z - p[1]))); } h };
    rseed(6);
    for _ in 0..13000 {
        let k = g.pick_v(); let [x, y, z] = g.wp(k); let n0 = g.wn(k);
        if hypot2(x, z) > 170.0 || y > -8.0 || n0[1] < -0.15 || rnd() > zw(x, z) || g.near_fly(x, y, z, 3.0) { continue; }
        let (t1, t2) = tangent(n0); let coral = n0[1] > 0.85 || rnd() < 0.35; let cnt = if coral { 2 + (rnd() * 5.0) as usize } else { 3 + (rnd() * 9.0) as usize }; let sd = rnd();
        for _ in 0..cnt {
            let s_ = 0.5 + cnt as f64 * 0.13; let a = rr(-1.0, 1.0) * s_; let b = rr(-1.0, 1.0) * s_;
            let Some(s) = g.snap(x + t1[0] * a + t2[0] * b, y + t1[1] * a + t2[1] * b, z + t1[2] * a + t2[2] * b, n0) else { continue };
            let (mut ux, mut uy, mut uz) = (s[3], s[4] + if coral { 0.9 } else { 1.1 }, s[5]); let l = hypot3(ux, uy, uz); ux /= l; uy /= l; uz /= l;
            let q = q_mul(q_from_y(ux, uy, uz), q_axis(0.0, 1.0, 0.0, rnd() * TAU)); let sc = if coral { rr(0.5, 1.5) } else { rr(0.35, 0.9) * if rnd() < 0.1 { 1.6 } else { 1.0 } };
            let mesh = if coral { 7 + (rnd() * 3.0) as usize } else { 4 + (rnd() * 3.0) as usize };
            let seed = fract(sd + rr(-0.03, 0.03));
            g.solid(mesh, s[0] - ux * 0.12 * sc, s[1] - uy * 0.12 * sc, s[2] - uz * 0.12 * sc, sc, q, if coral { 5.0 } else { 4.0 }, seed, 1.0, 0.0);
        }
        if !coral && rnd() < 0.3 { spots.push([x + n0[0], y + n0[1] + 0.5, z + n0[2]]); }
    }
    // dense sponge / coral gardens: x, z, radius, count
    rseed(7);
    for gd in [[-68., -6., 9., 260.], [-66., 6., 7., 160.], [16., 72., 7., 200.], [-6., 74., 6., 120.], [-13., -2., 6., 80.], [8., -88., 8., 160.], [44., 12., 7., 120.], [-62., 22., 6., 140.], [-47., 40., 6., 140.], [-30., 47., 6., 120.], [-12., 48., 5., 100.], [14., 28., 6., 140.], [28., 45., 6., 120.], [-36., -76., 6., 130.], [-49., -58., 6., 130.], [-58., -38., 6., 120.]] {
        let mut made = 0.0;
        let mut i = 0;
        while i < 400000 && made < gd[3] {
            i += 1;
            let k = g.pick_v(); let [x, y, z] = g.wp(k); let wn = g.wn(k);
            if hypot2(x - gd[0], z - gd[1]) > gd[2] || y > -14.0 || wn[1] < -0.1 || g.near_fly(x, y, z, 2.2) { continue; }
            made += 1.0;
            let coral = wn[1] > 0.85 || rnd() < 0.3; let (nx, ny, nz) = (wn[0], wn[1] + if coral { 0.9 } else { 1.1 }, wn[2]); let l = hypot3(nx, ny, nz);
            let sc = if coral { rr(0.5, 1.4) } else { rr(0.35, 0.9) * if rnd() < 0.15 { 1.7 } else { 1.0 } };
            let mesh = if coral { 7 + (rnd() * 3.0) as usize } else { 4 + (rnd() * 3.0) as usize };
            let q = q_mul(q_from_y(nx / l, ny / l, nz / l), q_axis(0.0, 1.0, 0.0, rnd() * TAU));
            g.solid(mesh, x - nx / l * 0.1 * sc, y - ny / l * 0.1 * sc, z - nz / l * 0.1 * sc, sc, q, if coral { 5.0 } else { 4.0 }, rnd(), 1.0, 0.0);
        }
    }
    // --- rocks: scattered ground rocks (the original's floating boulder clusters are gone)
    rseed(9);
    for _ in 0..2600 {
        let k = g.pick_v(); let [x, y, z] = g.wp(k);
        if g.wn(k)[1] < 0.7 || hypot2(x, z) > 170.0 || g.near_fly(x, y, z, 6.0) { continue; }
        let sc = rr(0.25, 1.3) * if rnd() < 0.1 { 2.0 } else { 1.0 };
        let m = (rnd() * 4.0) as usize; let q = q_rand(); let s = rnd();
        g.solid(m, x, y + sc * 0.2, z, sc, q, 1.0, s, 1.0, 0.0);
    }
    rseed(10);
    for _ in 0..60000 {
        let k = g.pick_v(); let [x, y, z] = g.wp(k); let wn = g.wn(k); let ny = wn[1];
        if hypot2(x, z) > 172.0 || y > -3.0 || (ny > 0.8 && rnd() < 0.6) || g.near_fly(x, y, z, 3.5) { continue; }
        let sc = (0.35 + 2.1 * rnd().powf(2.6)) * if ny > 0.8 { 0.5 } else { 1.0 }; let q = q_rand();
        let ci = (if sc > 1.15 { 3 } else { 0 }) + (rnd() * 3.0) as usize; let s = rnd();
        g.chunks[ci][cell_of(x, z)].extend_from_slice(&[x - wn[0] * sc * 0.3, y - ny * sc * 0.3, z - wn[2] * sc * 0.3, sc, q[0], q[1], q[2], q[3], 1.0, s, if sc > 1.15 { 0.0 } else { 96.0 }, 0.0].map(|v| v as f32));
    }
    rseed(11);
    for _ in 0..170000 { // pebbles and rubble on the sea floor
        let k = g.pick_v(); let x = g.t.wp[k] as f64 + rr(-0.4, 0.4); let z = g.t.wp[k + 2] as f64 + rr(-0.4, 0.4); let y = g.t.wp[k + 1] as f64;
        if g.wn(k)[1] < 0.8 || hypot2(x, z) > 170.0 || g.near_fly(x, y, z, 1.3) { continue; }
        let sc = 0.05 + 0.3 * rnd().powi(2); let q = q_rand(); let s = rnd();
        let kk = g.nth; g.nth += 1;
        if kk % g.thin == 0 { g.chunks[6][cell_of(x, z)].extend_from_slice(&[x, y - sc * 0.25, z, sc, q[0], q[1], q[2], q[3], 1.0, s, 44.0, 0.0].map(|v| v as f32)); }
    }
    // --- set pieces
    let fy = g.gy(-5.0, -4.0);
    g.solid(10, -5.0, fy - 0.6, -4.0, 1.0, q_mul(q_axis(0.0, 1.0, 0.0, 1.2), q_mul(q_axis(0.0, 0.0, 1.0, 0.16), q_axis(1.0, 0.0, 0.0, -0.3))), 2.0, 0.3, 1.0, 0.0); // whale fluke
    g.solid(11, 59.5, -23.0, 6.0, 1.0, q_mul(q_axis(0.0, 1.0, 0.0, -PI / 2.0), q_mul(q_axis(0.0, 0.0, 1.0, -0.1), q_axis(1.0, 0.0, 0.0, 0.32))), 3.0, 0.5, 1.0, 0.0); // ship hull
    let mut caps: Vec<(V3, V3, f64)> = vec![];
    {
        let rot = |q: Q, v: V3| -> V3 { // v + 2 q.xyz x (q.xyz x v + q.w v)
            let (qx, qy, qz, qw) = (q[0], q[1], q[2], q[3]);
            let t = [qy * v[2] - qz * v[1] + qw * v[0], qz * v[0] - qx * v[2] + qw * v[1], qx * v[1] - qy * v[0] + qw * v[2]];
            [v[0] + 2.0 * (qy * t[2] - qz * t[1]), v[1] + 2.0 * (qz * t[0] - qx * t[2]), v[2] + 2.0 * (qx * t[1] - qy * t[0])]
        };
        let qs = q_mul(q_axis(0.0, 1.0, 0.0, -PI / 2.0), q_mul(q_axis(0.0, 0.0, 1.0, -0.1), q_axis(1.0, 0.0, 0.0, 0.32)));
        let o = [59.5, -23.0, 6.0]; let w = |v: V3| { let r = rot(qs, v); [o[0] + r[0], o[1] + r[1], o[2] + r[2]] };
        caps.push((w([-15.0, -2.5, 0.0]), w([15.0, -2.0, 0.0]), 4.6)); // hull
        caps.push((w([7.0, -5.0, 0.0]), w([7.6, 14.0, 0.6]), 0.9)); // fore mast
        caps.push((w([-3.0, -5.0, 0.0]), w([-5.0, 9.0, 2.4]), 1.0)); // broken main mast
        let qf = q_mul(q_axis(0.0, 1.0, 0.0, 1.2), q_mul(q_axis(0.0, 0.0, 1.0, 0.16), q_axis(1.0, 0.0, 0.0, -0.3)));
        let of = [-5.0, fy - 0.6, -4.0]; let wf = |v: V3| { let r = rot(qf, v); [of[0] + r[0], of[1] + r[1], of[2] + r[2]] };
        for c in [[0.0, -1.6, 3.7, 0.0, 6.9, 0.1, 1.2], [0.0, 6.6, 0.1, 4.0, 9.0, -0.1, 0.9], [4.0, 9.0, -0.1, 7.8, 13.5, -0.6, 0.55], [0.0, 6.6, 0.1, -3.5, 9.9, -0.05, 0.9], [-3.5, 9.9, -0.05, -6.0, 15.5, -0.55, 0.55]] {
            caps.push((wf([c[0], c[1], c[2]]), wf([c[3], c[4], c[5]]), c[6]));
        }
    }
    // barrel sponges in front of viewpoint 4, small sponges around the fluke base
    rseed(12);
    for b in [[7., 3.2, 1.3, 4.], [8.4, 4.6, 1.0, 6.], [9.6, 2.6, 1.5, 4.], [11., 5., 1.1, 5.], [8., -3.6, 1.2, 4.], [12.5, 2., 1.2, 6.], [9., 3.9, 0.8, 4.]] {
        let p = g.rel(3, b[0], b[1]); let c = poses[3]; let (ux, uz) = ((c[0] - p[0]) * 0.07, (c[2] - p[1]) * 0.07); let l = hypot3(ux, 1.0, uz);
        let y = g.gy(p[0], p[1]) - 0.1; let s = rnd();
        g.solid(b[3] as usize, p[0], y, p[1], b[2], q_from_y(ux / l, 1.0 / l, uz / l), 4.0, s, 1.0, 0.0);
    }
    for _ in 0..9 {
        let a = rnd() * TAU; let x = -5.0 + a.cos() * rr(1.2, 2.6); let z = -4.0 + a.sin() * rr(1.5, 4.0);
        let y = g.gy(x, z) + 0.05; let sc = rr(0.4, 0.9); let q = q_axis(0.0, 1.0, 0.0, rnd() * 6.0); let s = rnd();
        g.solid(6, x, y, z, sc, q, 4.0, s, 1.0, 0.0);
    }

    // --- bubbles
    let mut vents: Vec<V3> = vec![[4., -44., -3.], [4.5, -44., -3.5], [-1., -44., 2.], [2., -44., -1.], [5.5, -44., -7.], [3., -44., -9.], [6., -44., -90.], [8., -44., -84.], [4., -44., -93.], [-9., -44., 3.], [5., -35., -86.], [9., -37., -92.], [40., -44., 20.], [-56., -44., 12.], [10., -44., 80.], [-2., -44., 30.], [0., -44., -30.], [12., -44., 2.], [6., -44., -2.], [46., -44., 30.], [-20., -44., -103.], [-52., -44., -12.], [4., -44., 70.], [9., -44., -74.], [4., -44., -78.], [14., -44., 26.], [26., -44., 40.], [-45., -44., -64.], [-55., -44., -40.], [-58., -44., 22.], [-40., -44., 40.], [-14., -44., 45.], [-1., -44., -9.], [-7., -44., -6.]];
    rseed(13);
    for s in &spots { if rnd() < 0.28 { vents.push(*s); } }
    let mut bubbles = vec![];
    for v in &vents {
        let n = 90 + (rnd() * 70.0) as usize; let y0 = if v[1] == -44.0 { g.gy(v[0], v[2]) } else { v[1] };
        for _ in 0..n {
            let sz = if rnd() < 0.24 { rr(0.1, 0.3) } else { rr(0.025, 0.08) };
            let (a, b) = (rr(-0.3, 0.3), rr(-0.3, 0.3)); let (ph, sp, wb) = (rnd(), rr(1.1, 2.4) * (0.6 + sz * 3.0), rr(0.05, 0.16));
            bubbles.extend_from_slice(&[v[0] + a, y0, v[2] + b, sz, ph, sp, -y0 - 0.2, wb].map(|v| v as f32));
        }
    }

    // --- pack leaves (36 bytes: 8 floats + 4 unorm8), sorted by culling cell
    let pack = |l: &Vec<Vec<f32>>, step: usize, wm: f32| -> Pack {
        let mut n = 0u32; let mut start = vec![0u32; NC + 1];
        for c in 0..NC { start[c] = n; n += (l[c].len() / 12).div_ceil(step) as u32; }
        start[NC] = n;
        let mut buf = vec![0u8; n as usize * 36]; let mut lo = vec![1e9f32; NC]; let mut hi = vec![-1e9f32; NC];
        let mut k = 0;
        for c in 0..NC {
            let a = &l[c]; let mut i = 0;
            while i < a.len() {
                let o = k * 36;
                for j in 0..8 { let v = if j == 6 { a[i + j] * wm } else { a[i + j] }; buf[o + j * 4..o + j * 4 + 4].copy_from_slice(&v.to_le_bytes()); }
                buf[o + 32] = a[i + 8] as u8; buf[o + 33] = (a[i + 9] * 255.0) as u8; buf[o + 34] = (a[i + 10] * 255.0) as u8; buf[o + 35] = (a[i + 11] * 255.0) as u8;
                let (y, ln) = (a[i + 1], a[i + 3]); lo[c] = lo[c].min(y - ln); hi[c] = hi[c].max(y + ln);
                i += 12 * step; k += 1;
            }
        }
        Pack { buf, n, start, lo, hi }
    };

    // --- half-res volume: r = openness, g = skylight
    let mut vol_d = vec![0u8; HX * HY * HZ * 2];
    let mut lp = vec![1f32; HX * HZ]; let mut ln = vec![0f32; HX * HZ]; let mut tmp = vec![0f32; HX * HZ];
    for y in (0..HY).rev() {
        for z in 0..HZ { for x in 0..HX { let i = x + HX * z; tmp[i] = (lp[if x > 0 { i - 1 } else { i }] + 2.0 * lp[i] + lp[if x < HX - 1 { i + 1 } else { i }]) * 0.25; } }
        for z in 0..HZ { for x in 0..HX {
            let i = x + HX * z; let b = (tmp[if z > 0 { i - HX } else { i }] + 2.0 * tmp[i] + tmp[if z < HZ - 1 { i + HX } else { i }]) * 0.25;
            let g0 = Gen::gi(x * 2, y * 2, z * 2);
            let dd = (d[g0] + d[g0 + 1] + d[g0 + NX] + d[g0 + NX + 1] + d[g0 + NX * NY] + d[g0 + NX * NY + 1] + d[g0 + NX * NY + NX] + d[g0 + NX * NY + NX + 1]) * 0.125;
            let o = x + HX * (y + HY * z);
            let l = b * (0.5 - dd / 1.5).clamp(0.0, 1.0) * (1.0 - g.kelp_abs[o].min(0.5));
            ln[i] = l;
            vol_d[o * 2] = ((0.5 - dd / 12.0).clamp(0.0, 1.0) * 255.0) as u8; vol_d[o * 2 + 1] = (b.min(1.0) * 255.0) as u8;
        } }
        std::mem::swap(&mut lp, &mut ln);
    }
    // --- ground map: r magenta cover, b purple zone, a sand
    let mut gm = vec![0u8; 512 * 512 * 4];
    for j in 0..512 { for i in 0..512 {
        let x = (i as f64 + 0.5) / 512.0 * 384.0 - 192.0; let z = (j as f64 + 0.5) / 512.0 * 384.0 - 192.0; let o = (i + 512 * j) * 4;
        gm[o] = (mag_at(x, z) * 255.0) as u8; gm[o + 2] = (zone_p(x, z) * 255.0) as u8; gm[o + 3] = (sstep(0.42, 0.6, fbm(x * 0.04, 3.0, z * 0.04, 3)) * 255.0) as u8;
    } }
    // --- fish schools + mantas
    let mut fish: [Vec<f32>; 3] = [vec![], vec![], vec![]];
    rseed(14);
    for s in SCHOOLS {
        let ss = if s[7] < 0.0 { rnd() } else { s[7] };
        for _ in 0..s[4] as usize { let r = rnd(); fish[s[8] as usize].extend_from_slice(&[s[0], s[1], s[2], s[3], r, s[6] * 1.9 / s[3] * if s[8] == 2.0 { 0.55 } else { 1.0 }, s[5], ss].map(|v| v as f32)); }
    }
    let mut mantas = vec![];
    for m in [[1., -28., 3.5, 14., 3.012, 0.085, 3.6, 0.1], [49., -31., 24.5, 5., 0.029, -0.08, 3.7, 0.4], [0., -14., -80., 34., 0.3, 0.06, 5.2, 0.7], [-50., -18., 20., 26., 5., -0.075, 3.8, 0.55], [10., -16., 86., 26., 2.2, 0.07, 4.4, 0.85], [20., -11., -20., 46., 4.2, -0.045, 5.6, 0.25]] { mantas.extend(m.iter().map(|&v| v as f32)); }
    // --- drifting particles
    let snow: Vec<f32> = (0..9000 * 4).map(|_| rnd() as f32).collect();

    let mut sol = vec![]; let mut ranges = [(0u32, 0u32); 12]; let mut si = 0u32;
    for m in 0..12 { let a = &g.solids[m]; let n = (a.len() / 12) as u32; ranges[m] = (si, n); si += n; sol.extend_from_slice(a); }
    let pack_c = |l: &Vec<Vec<f32>>| -> Pack {
        let mut n = 0u32; let mut start = vec![0u32; NC + 1];
        for c in 0..NC { start[c] = n; n += (l[c].len() / 12) as u32; }
        start[NC] = n;
        let mut data: Vec<f32> = Vec::with_capacity(n as usize * 12); let mut lo = vec![1e9f32; NC]; let mut hi = vec![-1e9f32; NC];
        for c in 0..NC { let a = &l[c]; for i in (0..a.len()).step_by(12) { data.extend_from_slice(&a[i..i + 12]); lo[c] = lo[c].min(a[i + 1] - 3.0); hi[c] = hi[c].max(a[i + 1] + 3.0); } }
        Pack { buf: bytemuck::cast_slice(&data).to_vec(), n, start, lo, hi }
    };
    let pose_y: Vec<f64> = poses.iter().map(|p| g.gy(p[0], p[2])).collect();
    World {
        glints: std::mem::take(&mut g.glints), chunks: g.chunks.iter().map(pack_c).collect(), props: g.props.iter().map(pack_c).collect(),
        big: pack(&g.big, 1, 1.0), big_f: pack(&g.big, 2, 1.5), small: pack(&g.small, 1, 1.0),
        solids: sol, ranges, bubbles, vol_d, gm, fish, mantas, snow, pose_y, kelp: std::mem::take(&mut g.kelp), caps,
    }
}
fn tangent(n: V3) -> (V3, V3) {
    let t = if n[1].abs() < 0.9 { [n[2], 0.0, -n[0]] } else { [1.0, 0.0, 0.0] }; let l = hypot3(t[0], t[1], t[2]); let t = [t[0] / l, t[1] / l, t[2] / l];
    (t, [n[1] * t[2] - n[2] * t[1], n[2] * t[0] - n[0] * t[2], n[0] * t[1] - n[1] * t[0]])
}

// ---------------------------------------------------------------- procedural meshes
pub fn mesh_chunk(sd: i32, sub: usize, det: bool, lv: u32) -> Mesh {
    let sub = if lv > 1 && sub > 1 { sub - 1 } else { sub }; // Low: a quarter of the triangles
    rseed(100 + sd);
    let mut pl = vec![];
    for _ in 0..4 { let q = q_rand(); let w = rr(0.8, 0.98); let l = nz1(hypot3(q[0], q[1], q[2])); pl.push([q[0] / l, q[1] / l, q[2] / l, w]); }
    let sdf = sd as f64;
    icosphere(sub, &|x, y, z| {
        let r = 1.0 + (fbm(x * 1.6 + sdf, y * 1.6, z * 1.6, 2) - 0.5) * 0.9 + if det { (fbm(x * 4.0 + sdf, y * 4.0, z * 4.0, 2) - 0.5) * 0.25 } else { 0.0 };
        let (mut px, mut py, mut pz) = (x * r, y * r * 0.8, z * r);
        for p in &pl { let d = px * p[0] + py * p[1] + pz * p[2] - p[3]; if d > 0.0 { px -= p[0] * d; py -= p[1] * d; pz -= p[2] * d; } }
        [px, py, pz]
    })
}
pub fn mesh_lathe(prof: &[[f64; 3]], seg: usize, lump: f64, ribs: f64) -> Mesh {
    let mut m = Mesh::new(); let mut rings = vec![];
    for &[r, h, ao] in prof {
        let mut ring = vec![];
        for s in 0..seg { let a = s as f64 / seg as f64 * TAU; let rl = r * (1.0 + lump * (fbm(a.cos() * 1.5 + h * 2.0, h * 3.0, a.sin() * 1.5, 2) - 0.5) + 0.07 * (ribs * a + h * 1.5).cos()); ring.push(m.v(a.cos() * rl, h, a.sin() * rl, ao)); }
        rings.push(ring);
    }
    for k in 0..rings.len() - 1 { for s in 0..seg { let s2 = (s + 1) % seg; m.quad(rings[k][s], rings[k + 1][s], rings[k + 1][s2], rings[k][s2]); } }
    m
}
fn nrm(v: V3) -> V3 { let l = nz1(hypot3(v[0], v[1], v[2])); [v[0] / l, v[1] / l, v[2] / l] }
pub fn mesh_coral(fan: bool, lo: bool) -> Mesh {
    rseed(200 + fan as i32);
    let mut m = Mesh::new();
    fn br(m: &mut Mesh, p: V3, mut d: V3, r: f64, len: f64, depth: i32, fan: bool, lo: bool) {
        let mut pts = vec![p]; let mut q = p;
        for _ in 0..3 {
            let a = rr(-0.3, 0.3); let b = rr(-0.1, 0.3); let c = if fan { rr(-0.05, 0.05) } else { rr(-0.3, 0.3) };
            d = nrm([d[0] + a, d[1] + b, d[2] + c]); q = [q[0] + d[0] * len / 3.0, q[1] + d[1] * len / 3.0, q[2] + d[2] * len / 3.0]; pts.push(q);
        }
        if depth == 0 { pts.push([q[0] + d[0] * r * 1.6, q[1] + d[1] * r * 1.6, q[2] + d[2] * r * 1.6]); }
        let df = depth as f64;
        if lo {
            let pp = if depth != 0 { vec![pts[0], pts[3]] } else { vec![pts[0], pts[3], pts[4]] };
            m.tube(&pp, &|k| (if k > 1 { r * 0.36 } else { r * (1.0 - 0.22 * k as f64) }) * 1.25, 3, true, Some(&|k| clamp(1.0 - (df + 1.0 - k as f64) / 5.0, 0.0, 1.0)));
        } else {
            m.tube(&pts, &|k| if k > 3 { r * 0.36 } else { r * (1.0 - 0.22 * k as f64 / 3.0) }, if fan { 5 } else { 6 }, true, Some(&|k| clamp(1.0 - (df + 1.0 - k as f64 / 3.0) / 5.0, 0.0, 1.0)));
        }
        if depth > 0 {
            let n = 2 + (rnd() < 0.5) as usize;
            for _ in 0..n {
                let a = rr(-0.8, 0.8); let b = rr(-0.1, 0.5); let c = if fan { rr(-0.08, 0.08) } else { rr(-0.8, 0.8) };
                br(m, q, nrm([d[0] + a, d[1] + b, d[2] + c]), r * 0.78, len * 0.8, depth - 1, fan, lo);
            }
        }
    }
    let n0 = if fan { 1 } else { 2 + (rnd() * 2.0) as usize };
    for _ in 0..n0 {
        let p = [rr(-0.08, 0.08), 0.0, rr(-0.08, 0.08)]; let a = rr(-0.5, 0.5); let c = if fan { 0.0 } else { rr(-0.5, 0.5) };
        br(&mut m, p, nrm([a, 1.0, c]), if fan { 0.06 } else { 0.08 }, if fan { 0.5 } else { 0.45 }, 3, fan, lo);
    }
    m
}
pub fn mesh_tubes() -> Mesh {
    rseed(210);
    let mut m = Mesh::new();
    for _ in 0..9 {
        let a = rnd() * TAU; let r0 = rr(0.0, 0.28); let h = rr(0.45, 1.1); let rad = rr(0.07, 0.12); let lean = rr(0.05, 0.3);
        let pts: Vec<V3> = (0..=4).map(|k| { let k = k as f64; [a.cos() * (r0 + lean * k / 4.0 * h), h * k / 4.0, a.sin() * (r0 + lean * k / 4.0 * h)] }).collect();
        m.tube(&pts, &|k| rad * (0.8 + 0.25 * k as f64 / 4.0), 12, false, Some(&|k| 0.35 + 0.65 * k as f64 / 4.0));
        // rim + dark throat
        let e = pts[4]; let n = m.nverts() as u32; let c = m.v(e[0], e[1] - rad * 1.2, e[2], 0.05);
        for s2 in 0..12 { m.tri(n - 12 + s2, n - 12 + (s2 + 1) % 12, c); }
    }
    m
}
/// stone whale fluke: SDF of tapered capsules -> surface nets
pub fn mesh_fluke() -> Built {
    let mut cs: Vec<[f64; 9]> = vec![];
    let mut chain = |pts: &[[f64; 4]], zs: f64| { for i in 0..pts.len() - 1 { let (a, b) = (pts[i], pts[i + 1]); cs.push([a[0], a[1], a[2], a[3], b[0], b[1], b[2], b[3], zs]); } };
    chain(&[[0., -1.6, 3.7, 1.2], [0., 1.8, 2.35, 0.98], [0., 4.6, 1.0, 0.76], [0., 6.9, 0.1, 0.6]], 0.8);
    chain(&[[0., 6.6, 0.1, 0.72], [1.9, 7.9, 0., 0.98], [3.9, 8.8, -0.1, 0.9], [5.7, 9.8, -0.25, 0.64], [7.0, 11.2, -0.4, 0.38], [7.7, 12.8, -0.55, 0.2], [7.9, 14.1, -0.65, 0.1]], 0.18);
    chain(&[[0., 6.6, 0.1, 0.72], [-1.7, 8.3, 0.05, 0.96], [-3.3, 9.7, -0.05, 0.88], [-4.7, 11.2, -0.2, 0.64], [-5.6, 12.9, -0.4, 0.38], [-6.0, 14.6, -0.5, 0.2], [-5.9, 15.9, -0.6, 0.1]], 0.18);
    let sdf = |x: f64, y: f64, z: f64| {
        let mut d = 1e9f64;
        for c in &cs {
            let zs = c[8]; let pz = z / zs; let az = c[2] / zs; let bz = c[6] / zs;
            let (bax, bay, baz, pax, pay, paz) = (c[4] - c[0], c[5] - c[1], bz - az, x - c[0], y - c[1], pz - az);
            let h = clamp((pax * bax + pay * bay + paz * baz) / (bax * bax + bay * bay + baz * baz), 0.0, 1.0);
            let e = (hypot3(pax - bax * h, pay - bay * h, paz - baz * h) - mix(c[3], c[7], h)) * zs;
            let k = 0.45; let hh = (k - (d - e).abs()).max(0.0) / k;
            d = d.min(e) - hh * hh * k * 0.25;
        }
        d
    };
    let (vs, ox, oy, oz, nx, ny, nz) = (0.09, -7.6, -2.6, -2.2, 186usize, 216usize, 76usize);
    let mut dd = vec![0f32; nx * ny * nz];
    for z in 0..nz { for y in 0..ny { for x in 0..nx { dd[x + nx * (y + ny * z)] = -sdf(ox + x as f64 * vs, oy + y as f64 * vs, oz + z as f64 * vs) as f32; } } }
    let sn = surface_nets(&dd, nx, ny, nz); let smp = Sampler3 { d: &dd, nx, ny, nz };
    let nv = sn.pos.len() / 3; let mut vb = vec![0u8; nv * 16];
    for k in 0..nv {
        let (x, y, z, e) = (sn.pos[k * 3] as f64, sn.pos[k * 3 + 1] as f64, sn.pos[k * 3 + 2] as f64, 0.7);
        let (ax, ay, az) = (smp.at(x - e, y, z) - smp.at(x + e, y, z), smp.at(x, y - e, z) - smp.at(x, y + e, z), smp.at(x, y, z - e) - smp.at(x, y, z + e));
        let l = nz1(hypot3(ax, ay, az));
        write_vert(&mut vb, k, [(ox + x * vs) as f32, (oy + y * vs) as f32, (oz + z * vs) as f32], [ax / l, ay / l, az / l, 1.0]);
    }
    Built { vb, ib: sn.idx, nv }
}
/// sunken ship (bow at +x)
pub fn mesh_ship() -> Mesh {
    rseed(220);
    let mut m = Mesh::new(); let (lh, b_, dp_) = (17.0, 5.6, 7.6);
    let hb = move |x: f64| b_ * (1.0 - (x / lh).abs().powf(if x < 0.0 { 3.6 } else { 2.6 })).max(0.0).powf(0.5);
    let dp = move |x: f64| dp_ * (1.0 - 0.36 * (x / lh).abs().powf(2.5));
    let sheer = move |x: f64| 0.5 + (if x > 0.0 { 3.0 } else { 3.8 }) * (x / lh).abs().powf(2.2);
    let pt = move |x: f64, ph: f64, off: f64| -> V3 { let b = hb(x) + off; let d = dp(x) + off; let s = sheer(x); [x, s - (d + s) * ph.cos().max(0.0).powf(0.66), b * ph.signum() * if ph == 0.0 { 0.0 } else { 1.0 } * ph.sin().abs().powf(0.8)] };
    let out = |p: V3, _k: usize| -> V3 { let l = nz1(hypot2(p[1] + 0.5, p[2] * 1.3)); [0.0, (p[1] + 0.5) / l, p[2] * 1.3 / l] };
    let up = |_: V3, _: usize| -> V3 { [0.0, 1.0, 0.0] };
    let c = |v: f64| move |_: usize| v;
    let mut keel = vec![];
    for i in 0..=44 { let x = mix(-lh - 1.0, lh + 2.4, i as f64 / 44.0); let e = ((x.abs() / lh) - 0.78).max(0.0) * 4.2; keel.push([x, -dp(clamp(x, -lh, lh)) - 0.3 + e * e * e * 4.2, 0.0]); }
    m.sweep(&keel, &c(0.55), &c(0.95), &up, true);
    let mut x = -lh + 0.7;
    while x < lh - 0.3 {
        let a0 = if rnd() < 0.1 { rr(-0.9, -0.2) } else { -PI / 2.0 }; let a1 = if rnd() < 0.1 { rr(0.2, 0.9) } else { PI / 2.0 };
        let pts: Vec<V3> = (0..=16).map(|i| pt(x, mix(a0, a1, i as f64 / 16.0), 0.0)).collect();
        m.sweep(&pts, &c(0.24), &c(0.3), &out, true);
        x += 0.8;
    }
    let hole_x = rr(-6.0, 2.0); let hole_s = if rnd() < 0.5 { 1.0 } else { -1.0 };
    let brs = [[hole_x, hole_s * 0.8, 3.2, 0.42], [7.0, -hole_s * 0.95, 2.4, 0.34], [12.0, hole_s * 0.5, 1.7, 0.28], [-11.0, -hole_s * 0.55, 2.1, 0.3]];
    let holed = |x: f64, ph: f64| brs.iter().any(|h| { let a = (x - h[0]) / h[2]; let b = (ph - h[1]) / h[3]; a * a + b * b < 1.0 + 0.45 * (x * 3.1 + ph * 9.0).sin() });
    let ns = 14; let mut xs = vec![]; let mut x = -lh + 0.35; while x <= lh - 0.3 { xs.push(x); x += 0.85; }
    for j in 0..ns {
        let p0 = mix(-1.5, 1.5, j as f64 / ns as f64); let p1 = mix(-1.5, 1.5, (j + 1) as f64 / ns as f64); let sg = if p0 + p1 < 0.0 { -1.0 } else { 1.0 };
        let (lo, hi) = if sg < 0.0 { (p1, p0) } else { (p0, p1) };
        let mut pv: Option<[u32; 4]> = None;
        for &x in &xs {
            let cu = [m.v1(pt(x, lo, 0.27)), m.v1(pt(x, hi, 0.14)), m.v1(pt(x, lo, 0.1)), m.v1(pt(x, hi, 0.1))];
            if let Some(pv) = pv { if !holed(x - 0.42, (p0 + p1) / 2.0) {
                if sg > 0.0 { m.quad(pv[0], pv[1], cu[1], cu[0]); m.quad(pv[2], cu[2], cu[3], pv[3]); } else { m.quad(pv[0], cu[0], cu[1], pv[1]); m.quad(pv[2], pv[3], cu[3], cu[2]); }
            } }
            pv = Some(cu);
        }
    }
    for sgn in [-1.0, 1.0] {
        let mut pts = vec![]; let mut x = -lh + 0.6; while x <= lh - 0.6 { pts.push(pt(x, sgn * PI / 2.0, 0.2)); x += 1.0; }
        m.sweep(&pts, &c(0.3), &c(0.28), &up, true);
        // stern castle bulwarks
        for j in 0..3 {
            let mut pts = vec![]; let mut x = -lh + 0.4;
            while x <= -lh + 8.5 - j as f64 { pts.push([x, sheer(x) + 0.55 + j as f64 * 0.86, sgn * (hb(x).max(2.6) + 0.15)]); x += 0.9; }
            m.sweep(&pts, &c(0.82), &c(0.09), &move |_, _| [0.0, 0.0, sgn], true);
        }
    }
    // transom
    let mut y = -3.0;
    while y < sheer(-lh) + 3.0 { let w = (2.9 * sstep(-4.5, 1.5, y)).max(1.2); if rnd() < 0.9 { m.sweep(&[[-lh + 0.35, y, -w], [-lh + 0.35, y, w]], &c(0.82), &c(0.1), &|_, _| [1.0, 0.0, 0.0], true); } y += 0.86; }
    let mut x = -lh + 2.0;
    while x < lh - 2.0 { m.sweep(&[[x, sheer(x) - 0.25, -hb(x)], [x, sheer(x) - 0.2, 0.0], [x, sheer(x) - 0.25, hb(x)]], &c(0.26), &c(0.26), &up, true); x += rr(1.8, 3.2); }
    let mut z: f64 = -3.9;
    while z <= 3.9 {
        let mut x = -lh + rr(1.0, 5.0);
        while x < lh - 4.0 {
            let x1 = (x + rr(3.0, 9.0)).min(lh - 3.5);
            if rnd() < 0.6 { let mut pts = vec![]; let mut xx = x; while xx <= x1 { if hb(xx) > z.abs() + 0.3 { pts.push([xx, sheer(xx) - 0.05, z]); } xx += 1.0; } if pts.len() > 1 { m.sweep(&pts, &c(0.56), &c(0.07), &up, true); } }
            x = x1 + rr(0.5, 3.0);
        }
        z += 0.62;
    }
    // foremast with yard, broken main mast, bowsprit
    let fx = |_: V3, _: usize| [1.0, 0.0, 0.0];
    m.sweep(&[[7., -dp_ + 0.6, 0.], [7.1, 1., 0.], [7.4, 8., 0.3], [7.8, 15., 0.8]], &|k| 0.62 - k as f64 * 0.1, &|k| 0.62 - k as f64 * 0.1, &fx, true);
    m.sweep(&[[7.5, 9.5, -5.2], [7.5, 9.6, 0.3], [7.5, 9.2, 5.6]], &c(0.26), &c(0.26), &up, true);
    m.sweep(&[[7.7, 13.2, -3.2], [7.7, 13.3, 3.6]], &c(0.2), &c(0.2), &up, true);
    m.sweep(&[[-3., -dp_ + 0.6, 0.], [-3.2, 1., 0.2], [-3.9, 6., 1.0], [-5.2, 9.5, 2.6]], &|k| 0.74 - k as f64 * 0.1, &|k| 0.74 - k as f64 * 0.1, &fx, true);
    m.sweep(&[[lh + 1.4, 2.6, 0.], [lh + 8., 5.2, 0.2]], &c(0.34), &c(0.34), &up, true);
    m
}
/// manta ray: span along x (half span = 1), nose +z; aux = span coordinate for the flap animation
pub fn mesh_manta() -> Mesh {
    let mut m = Mesh::new(); let (nu, mm) = (72, 22); let mut rings = vec![];
    for j in 0..=nu {
        let u = j as f64 / nu as f64 * 2.0 - 1.0; let au = u.abs();
        let mut zl = 0.52 - 0.74 * au.powf(1.35) + 0.07 * (-((au - 0.13) / 0.05).powi(2)).exp() - 0.05 * (-(au / 0.05).powi(2)).exp() - 0.08 * au.powi(5);
        let zt = -0.5 + 0.3 * au.powf(0.8) + 0.02 - 0.08 * au.powi(5);
        if zl < zt + 0.004 { zl = zt + 0.004; }
        let th = 0.08 * (1.0 - au).powf(1.15) + 0.004 + 0.12 * (-(u / 0.2).powi(2)).exp();
        let mut ring = vec![];
        for k in 0..mm { let a = k as f64 / mm as f64 * TAU; ring.push(m.v(u, th * a.sin() * (0.62 + 0.38 * a.cos()), mix(zt, zl, 0.5 + 0.5 * a.cos()), u)); }
        rings.push(ring);
    }
    for j in 0..nu { for k in 0..mm { let k2 = (k + 1) % mm; m.quad(rings[j][k], rings[j + 1][k], rings[j + 1][k2], rings[j][k2]); } }
    let tail: Vec<V3> = (0..=8).map(|i| [0.0, 0.0, -0.42 - i as f64 * 0.19]).collect();
    let n0 = m.nverts();
    m.tube(&tail, &|k| 0.016 * (1.0 - k as f64 / 9.0), 4, true, None);
    for k in n0..m.aux.len() { m.aux[k] = 0.0; }
    for sx in [-1.0, 1.0] { // cephalic fins
        let n1 = m.nverts();
        m.tube(&[[sx * 0.12, -0.02, 0.48], [sx * 0.135, -0.05, 0.6], [sx * 0.115, -0.09, 0.69], [sx * 0.085, -0.125, 0.74]], &|k| 0.045 * (1.0 - k as f64 / 4.4), 6, true, None);
        for k in n1..m.aux.len() { m.aux[k] = sx * 0.1; }
    }
    m
}
/// fish meshes (nose +x): 0 the original slender schooler, 1 a deep-bodied reef fish with fins, 2 a sleek jack.
/// aux = 0 on the body; 1 on fins (they flutter in the shader)
pub fn mesh_fish(species: usize) -> Mesh {
    let mut m = Mesh::new();
    let (xs, hs, ws): (Vec<f64>, Vec<f64>, Vec<f64>) = match species {
        0 => (vec![0.5, 0.3, 0.05, -0.22, -0.34], vec![0.01, 0.13, 0.17, 0.09, 0.035], vec![0.01, 0.05, 0.065, 0.03, 0.012]),
        1 => (vec![0.42, 0.34, 0.18, 0.0, -0.16, -0.27, -0.33], vec![0.01, 0.13, 0.27, 0.31, 0.24, 0.1, 0.05], vec![0.01, 0.04, 0.06, 0.065, 0.05, 0.025, 0.01]),
        _ => (vec![0.52, 0.42, 0.25, 0.0, -0.2, -0.33, -0.4], vec![0.01, 0.08, 0.14, 0.15, 0.1, 0.04, 0.02], vec![0.01, 0.045, 0.075, 0.08, 0.055, 0.025, 0.012]),
    };
    let seg = 8; let mut rings = vec![];
    for i in 0..xs.len() { let mut r = vec![]; for k in 0..seg { let a = k as f64 / seg as f64 * TAU; r.push(m.v(xs[i], a.sin() * hs[i], a.cos() * ws[i], 0.0)); } rings.push(r); }
    for i in 0..xs.len() - 1 { for k in 0..seg { let k2 = (k + 1) % seg; m.quad(rings[i][k], rings[i + 1][k], rings[i + 1][k2], rings[i][k2]); } }
    let xe = xs[xs.len() - 1];
    // tail fin (two-sided triangles)
    let (ty, tx) = match species { 0 => (0.2, 0.22), 1 => (0.17, 0.16), _ => (0.26, 0.24) };
    let a = m.v(xe + 0.01, 0.0, 0.0, 1.0); let b = m.v(xe - tx, ty, 0.0, 1.0); let c = m.v(xe - tx * 0.6, 0.0, 0.0, 1.0); let d = m.v(xe - tx, -ty, 0.0, 1.0);
    m.tri(a, b, c); m.tri(a, c, d); m.tri(a, c, b); m.tri(a, d, c);
    if species > 0 {
        // dorsal + anal fins, pectoral fins
        let (dh, dx0, dx1) = if species == 1 { (0.2, 0.15, -0.25) } else { (0.11, 0.1, -0.05) };
        let top = |x: f64| { let i = xs.iter().position(|&v| v < x).unwrap_or(xs.len() - 1).max(1); let t = (x - xs[i - 1]) / (xs[i] - xs[i - 1]); mix(hs[i - 1], hs[i], t) };
        for sgn in [1.0, -1.0] {
            let (h0, h1) = (top(dx0) * sgn, top(dx1) * sgn);
            let p0 = m.v(dx0, h0, 0.0, 1.0); let p1 = m.v(dx1, h1, 0.0, 1.0); let p2 = m.v(dx1 - 0.08, h1 + dh * sgn * if sgn < 0.0 { 0.6 } else { 1.0 }, 0.0, 1.0);
            m.tri(p0, p1, p2); m.tri(p0, p2, p1);
        }
        for sz in [1.0, -1.0] {
            let w = ws[2] * 0.9;
            let p0 = m.v(xs[2] - 0.02, -0.02, sz * w, 1.0); let p1 = m.v(xs[2] - 0.16, -0.06, sz * (w + 0.09), 1.0); let p2 = m.v(xs[2] - 0.12, -0.03, sz * w, 1.0);
            m.tri(p0, p1, p2); m.tri(p0, p2, p1);
        }
    }
    m
}

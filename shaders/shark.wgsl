// ---------------------------------------------------------------- Blahaj (Hajsaver)
// "Blahaj" by Kaine_G (https://sketchfab.com/Kaine_G), CC BY 4.0. The mesh has no skeleton, so it is bent here:
// a swimming wave down the body, turning / pitching bends, flapping pectoral fins, a dorsal fin and tail lobe that
// lag and curl. Mesh space: snout -x, tail +x, up +y, left +z.
struct SK { pos: vec4f, fwd: vec4f, up: vec4f, left: vec4f, a: vec4f, b: vec4f };
// a: swim phase, wave amplitude, turn bend, pitch bend; b: left flap, right flap, dorsal curl, tail curl
@group(1) @binding(0) var<uniform> sk: SK;
@group(1) @binding(1) var tCol: texture_2d<f32>;
@group(1) @binding(2) var tRough: texture_2d<f32>;
struct SI { @location(0) p: vec3f, @location(1) n: vec3f, @location(2) uv: vec2f, @location(3) part: f32 };
struct SO { @builtin(position) cp: vec4f, @location(0) wp: vec3f, @location(1) n: vec3f, @location(2) uv: vec2f, @location(3) @interpolate(flat) part: f32, @location(4) op: vec3f };

fn curl(k0: f32, d: f32) -> vec2f { let k = select(k0, .05, abs(k0) < .05); return vec2f(sin(k * d) / k - d, (1. - cos(k * d)) / k); }
struct DF { p: vec3f, n: vec3f };
fn deform(p: vec3f, n0: vec3f, teeth: bool) -> DF {
  var q = p; var n = n0;
  // swimming: a wave that runs down the body, growing toward the tail, with a little counter-sway at the head
  let env = sk.a.y * (.07 + .93 * pow(ss(-2.4, 3.9, p.x), 1.5));
  let ph = 1.15 * p.x - sk.a.x;
  q.z += env * sin(ph);
  let dz = env * 1.15 * cos(ph);
  // turning and pitching: both ends swing around the middle
  let nu = (p.x - .15) / 3.7; let nu2 = nu * nu;
  q.z += sk.a.z * nu2; q.y += sk.a.w * nu2;
  let dzb = sk.a.z * 2. * nu / 3.7;
  n = vec3f(n.x - (dz + dzb) * n.z, n.y, n.z);
  if (!teeth) {
    // pectoral fins, each on its own spring
    let fz = abs(p.z);
    let fk = ss(.8, 1.3, fz) * (1. - ss(.2, .7, p.y)) * step(-1.5, p.x) * step(p.x, .8);
    let flap = select(sk.b.x, sk.b.y, p.z < 0.);
    q.y += flap * fk * (fz - .8) * 1.6 - fk * (fz - .8) * .25;
    n = vec3f(n.x, n.y - flap * fk * 1.6 * sign(p.z) * n.z * .5, n.z);
    // dorsal fin: soft, lags behind turns
    let dm = ss(-1.5, -1.2, p.x) * (1. - ss(.3, .55, p.x));
    let dh = max(0., p.y - .92) * dm;
    q.z -= p.z * .35 * ss(0., .25, dh);
    let c1 = curl(sk.b.z + sk.a.z * .9, dh); q.y += c1.x; q.z += c1.y;
    // upper tail lobe
    let th = max(0., p.y - .5) * ss(2.2, 2.7, p.x);
    let c2 = curl(sk.b.w, th); q.y += c2.x; q.z -= c2.y;
  }
  var o: DF; o.p = q; o.n = normalize(n);
  return o;
}
fn toWorld(q: vec3f) -> vec3f {
  return sk.pos.xyz + (-(q.x - .15) * sk.fwd.xyz + (q.y - .3) * sk.up.xyz + q.z * sk.left.xyz) * sk.pos.w;
}
fn dirWorld(n: vec3f) -> vec3f { return -n.x * sk.fwd.xyz + n.y * sk.up.xyz + n.z * sk.left.xyz; }
@vertex fn vs(i: SI) -> SO {
  var o: SO;
  let d = deform(i.p, i.n, i.part > .5);
  let w = toWorld(d.p);
  o.cp = g.vp * vec4f(w, 1.); o.wp = w; o.n = dirWorld(d.n); o.uv = i.uv; o.part = i.part; o.op = i.p;
  return o;
}
@vertex fn vsSh(i: SI) -> @builtin(position) vec4f {
  let d = deform(i.p, i.n, i.part > .5);
  return g.vp * vec4f(toWorld(d.p), 1.);
}
// the teeth texture did not survive the model's export: soft white triangles cut from their UVs instead
fn tooth(uv: vec2f) -> f32 {
  let v0 = .0285; let v1 = .0588; let vm = (v0 + v1) * .5; let hw = (v1 - v0) * .5;
  let tl = (uv.x - .0605) / (.0855 - .0605); let tu = (.0505 - uv.x) / (.0505 - .0200);
  let t = select(tu, tl, uv.x > .0555);
  if (t < 0. || t > 1. || uv.y < v0 || uv.y > v1) { return 0.; }
  return step(abs(uv.y - vm), hw * pow(1. - t, .75));
}
@fragment fn fs(i: SO, @builtin(front_facing) ff: bool) -> @location(0) vec4f {
  let teeth = i.part > .5;
  let col = textureSample(tCol, sR, i.uv).rgb;
  let rough = textureSample(tRough, sR, i.uv).g;
  let fzz = textureSample(tD, sR, i.uv * vec2f(11., 17.)).rg - .5;
  if (teeth && tooth(i.uv) < .5) { discard; }
  var n = normalize(i.n) * select(-1., 1., ff);
  // plush fuzz: a little high-frequency bump
  let t1 = normalize(cross(n, vec3f(.31, .9, .3))); let t2 = cross(n, t1);
  n = normalize(n + (t1 * fzz.x + t2 * fzz.y) * select(.45, 0., teeth));
  let alb = select(col * vec3f(.92, .96, 1.04), vec3f(.92, .88, .85), teeth);
  let L = g.sun.xyz; let V = normalize(g.cam.xyz - i.wp);
  let dist = length(i.wp - g.cam.xyz);
  let z = zone(i.wp); let vv = vol(i.wp + n * .6);
  let ca = caus(i.wp, dist);
  let E = sunE(i.wp.y, z) * shadow4(i.wp + n * .08);
  let nl = dot(n, L);
  // fabric: wrapped diffuse, a soft sheen at grazing angles, almost no specular
  let wrap = sat((nl + .45) / 1.45);
  let ndv = sat(dot(n, V));
  let sheen = pow(1. - ndv, 4.) * (1.2 - rough);
  let am = amb(i.wp, n, vv.g, z) * (.55 + .45 * sat((vv.r - .5) * 6.)) + side(i.wp, n, 1., z) * .5;
  var c = alb * (E * wrap * wrap * (.12 + ca * 1.1) + am * 1.35);
  c += (E * (.1 + ca * .4) + am) * vec3f(.55, .72, 1.) * sheen * .55;
  c += E * pow(sat(dot(n, normalize(V + L))), 18.) * (1. - rough) * .12 * ca;
  // light through the thin fins from behind
  c += E * alb * pow(sat(dot(-V, L)), 4.) * sat(-nl) * .25;
  return fin(fog(c, i.wp, z));
}

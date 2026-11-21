// ---------------------------------------------------------------- plush animals and Christmas trees (Hajsaver)
// Meshes from src/creatures.rs. Mesh space: forward +z, up +y, left +x.
struct PL { pos: vec4f, fwd: vec4f, up: vec4f, left: vec4f, a: vec4f };
// pos.w scale; fwd.w kind (0 whale, 1 octopus, 2 tree); a: phase, jet (octopus), -, -
@group(1) @binding(0) var<uniform> pl: PL;
struct PI_ { @location(0) p: vec3f, @location(1) n: vec3f, @location(2) c: vec4f, @location(3) an: vec2f };
struct PO { @builtin(position) cp: vec4f, @location(0) wp: vec3f, @location(1) n: vec3f, @location(2) c: vec4f, @location(3) op: vec3f, @location(4) @interpolate(flat) tw: f32 };

fn rotX(v: vec3f, a: f32) -> vec3f { let c = cos(a); let s = sin(a); return vec3f(v.x, c * v.y - s * v.z, s * v.y + c * v.z); }
struct DF { p: vec3f, n: vec3f };
fn bend(i: PI_) -> DF {
  var p = i.p; var n = i.n;
  let kind = i32(pl.fwd.w + .5); let ph = pl.a.x;
  if (kind == 0) {
    // whale: a slow up-and-down wave that grows toward the fluke; the flippers paddle
    let s = i.an.x;
    let amp = .16 * pow(s, 1.8);
    let w = sin(ph - s * 2.6);
    p.y += amp * w;
    let slope = .16 * 1.8 * pow(max(s, 1e-3), .8) * w * .5 - amp * cos(ph - s * 2.6) * 2.6 * .5;
    n = normalize(vec3f(n.x, n.y + slope * n.z, n.z));
    p.y += i.an.y * (abs(p.x) - .3) * .5 * sin(ph * 1.3 + 1.);
  } else if (kind == 1) {
    // octopus: the tentacles curl and wave, and spread wide in a jet
    let s = i.an.x; let k = i.an.y * 6.2831853;
    let jet = pl.a.y;
    let wv = sin(ph * 1.7 - s * 4. + k * 2.) * .09 * s * s;
    let out = vec3f(cos(k + .2), 0., sin(k + .2));
    p += out * (wv + jet * .25 * s * s) + vec3f(0., (sin(ph + k) * .06 - jet * .18) * s * s, 0.);
    // the head swells a little as it breathes
    if (s == 0.) { p *= 1. + .03 * sin(ph * .8); }
  } else {
    // tree: a gentle sway in the current, more toward the top
    let h = i.an.x;
    p.x += sin(g.cam.w * .6 + pl.a.x) * .05 * h * h; p.z += cos(g.cam.w * .45 + pl.a.x) * .04 * h * h;
  }
  var o: DF; o.p = p; o.n = n; return o;
}
fn toW(q: vec3f) -> vec3f { return pl.pos.xyz + (q.x * pl.left.xyz + q.y * pl.up.xyz + q.z * pl.fwd.xyz) * pl.pos.w; }
fn dirW(n: vec3f) -> vec3f { return n.x * pl.left.xyz + n.y * pl.up.xyz + n.z * pl.fwd.xyz; }
@vertex fn vs(i: PI_) -> PO {
  var o: PO;
  let d = bend(i); let w = toW(d.p);
  o.cp = g.vp * vec4f(w, 1.); o.wp = w; o.n = dirW(d.n); o.c = i.c; o.op = i.p; o.tw = i.an.y;
  return o;
}
@vertex fn vsSh(i: PI_) -> @builtin(position) vec4f { return g.vp * vec4f(toW(bend(i).p), 1.); }
@fragment fn fs(i: PO, @builtin(front_facing) ff: bool) -> @location(0) vec4f {
  let fz = textureSample(tD, sR, i.op.xz * 7. + i.op.y * 3.).rg - .5;
  var n = normalize(i.n) * select(-1., 1., ff);
  let t1 = normalize(cross(n, vec3f(.31, .9, .3))); let t2 = cross(n, t1);
  n = normalize(n + (t1 * fz.x + t2 * fz.y) * .4);
  let alb = pow(i.c.rgb, vec3f(2.2));
  let L = g.sun.xyz; let V = normalize(g.cam.xyz - i.wp); let dist = length(i.wp - g.cam.xyz);
  let z = zone(i.wp); let vv = vol(i.wp + n * .6);
  let ca = caus(i.wp, dist);
  let E = sunE(i.wp.y, z) * shadow4(i.wp + n * .1);
  let wrap = sat((dot(n, L) + .45) / 1.45);
  let ndv = sat(dot(n, V));
  let am = amb(i.wp, n, vv.g, z) * (.55 + .45 * sat((vv.r - .5) * 6.)) + side(i.wp, n, 1., z) * .5;
  var c = alb * (E * wrap * wrap * (.12 + ca * 1.1) + am * 1.35);
  c += (E * (.1 + ca * .4) + am) * vec3f(.6, .75, 1.) * pow(1. - ndv, 4.) * .45;
  // the toys keep their colours a little better than real animals would: the depth's blue-green cast is eased
  c += alb * (dot(am, vec3f(.33)) * .9 + .012);
  // glowing parts: baubles shine, fairy lights twinkle, each on its own beat
  let glow = 1. - i.c.a;
  if (glow > .01) {
    let beat = select(1., .35 + .65 * pow(.5 + .5 * sin(g.cam.w * (1.3 + fract(i.tw * .37) * 2.) + i.tw * 2.1), 3.), i.tw > .5);
    c += alb * glow * beat * select(1.5, 9., i.tw > .5);
  }
  return fin(fog(c, i.wp, z));
}

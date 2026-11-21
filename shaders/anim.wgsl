struct VI { @location(0) p: vec3f, @location(1) n: vec4f, @location(2) a: vec4f, @location(3) b: vec4f };
struct VO { @builtin(position) cp: vec4f, @location(0) wp: vec3f, @location(1) n: vec3f, @location(2) op: vec3f, @location(3) @interpolate(flat) sd: f32, @location(4) lt: f32 };
override SP: f32 = 0.;
@vertex fn vsFish(i: VI) -> VO {
  var o: VO;
  let s = i.b.x; let h = vec3f(hs(s), hs(s + .37), hs(s + .71));
  let R = i.a.w; let t = g.cam.w;
  let ta = t * i.b.y + i.b.w * 6.3 + (h.x - .5) * (26. / R);
  let ph = i.b.w * 17.;
  let P = i.a.xyz + vec3f(R * cos(ta), 2.2 * sin(3. * ta + ph), R * .6 * sin(ta));
  let T = normalize(vec3f(-R * sin(ta), 6.6 * cos(3. * ta + ph), R * .6 * cos(ta))) * sign(i.b.y);
  let S = normalize(cross(vec3f(0., 1., 0.), T)); let Up = cross(T, S);
  let off = S * ((h.y - .5) * 5. + sin(t * .6 + s * 40.) * .5) + Up * ((h.z - .5) * 3.2 + cos(t * .5 + s * 31.) * .4);
  var l = i.p * i.b.z * (.75 + .5 * h.z);
  l.z += sin(t * 9. + s * 50. + l.x * 5.) * .22 * (.25 * i.b.z - l.x);
  l.z += sin(t * 14. + s * 30. + l.x * 9.) * .04 * i.n.w * i.b.z;
  // Hajsaver: make way for the shark
  var fp = P + off;
  let dv = fp - g.shk.xyz; let dl = max(length(dv), 1e-3);
  fp += dv / dl * ss(g.shk.w * 3.2, 0., dl) * g.shk.w * 2.4;
  let w = fp + T * l.x + Up * l.y + S * l.z;
  o.cp = g.vp * vec4f(w, 1.); o.wp = w; o.n = T * i.n.x + Up * i.n.y + S * i.n.z; o.op = i.p; o.sd = s;
  o.lt = shadow1(fp); // one sample per fish, at its centre
  return o;
}
@fragment fn fsFish(i: VO, @builtin(front_facing) ff: bool) -> @location(0) vec4f {
  // thin fins are seen from both sides
  let n = normalize(i.n) * select(-1., 1., ff); let L = g.sun.xyz; let V = normalize(g.cam.xyz - i.wp);
  let z = zone(i.wp); let vv = vol(i.wp);
  // the texture masks, mapped along the body (u nose to tail) and down the flank (v back to belly)
  let fuv = vec2f((.52 - i.op.x) / 1.1, .5 - i.op.y / .62);
  let tx = textureSample(tF, sR, fuv);
  var alb = mix(vec3f(.50, .68, .80), vec3f(.10, .22, .40), ss(-.05, .2, i.op.y));
  alb = mix(alb, vec3f(.95, .8, .3), step(.86, fract(i.sd * 7.7)) * .6);
  alb = mix(alb, vec3f(.75, .78, .8), tx.a * .35);
  if (SP > .5 && SP < 1.5) {
    // reef fish: royal blue with a yellow tail, yellow tangs, and the odd orange one
    let h = fract(i.sd * 13.1);
    let tailK = ss(-.12, -.26, i.op.x);
    alb = mix(vec3f(.05, .16, .62), vec3f(.95, .78, .06), tailK);
    alb = mix(alb, vec3f(.02, .03, .12), ss(.05, .2, i.op.y) * ss(.1, -.1, i.op.x) * (1. - tailK) * .8);
    if (h > .55) { alb = vec3f(.98, .82, .08) * (.85 + .15 * ss(-.1, .2, i.op.y)); }
    if (h > .85) { alb = mix(vec3f(1., .42, .05), vec3f(.95, .95, .9), step(.5, fract(i.op.x * 4.2 + .1))); }
  } else if (SP > 1.5) {
    // jacks: steel blue back, silver flanks, a yellow stripe and dark bars
    alb = mix(vec3f(.62, .70, .76), vec3f(.10, .20, .30), ss(-.02, .1, i.op.y));
    alb = mix(alb, vec3f(.9, .75, .2), ss(.025, 0., abs(i.op.y - .015)) * .7);
    alb *= 1. - .45 * tx.g * ss(-.02, .08, i.op.y);
  }
  if (SP > .5 && SP < 1.5 && fract(i.sd * 13.1) > .85) { alb = mix(vec3f(1., .42, .05), vec3f(.95, .95, .9), tx.g); }
  // scales catch the light, fins show their rays, and every fish gets an eye
  alb *= tx.r;
  alb = mix(alb, alb * .55, tx.a * step(.72, fuv.x));
  alb = mix(alb, mix(vec3f(.85, .8, .6), vec3f(.01), step(.9, tx.b)), step(.4, tx.b));
  let E = sunE(i.wp.y, z) * i.lt;
  var c = alb * (E * (.25 + .75 * sat(dot(n, L))) + amb(i.wp, n, vv.g, z) * 1.3);
  c += E * pow(sat(dot(n, normalize(V + L))), 24.) * (1.0 + 1.2 * tx.r * tx.r) + vec3f(.3, .6, .7) * pow(1. - sat(dot(n, V)), 3.) * .25 * vv.g;
  return fin(fog(c, i.wp, z));
}
@vertex fn vsManta(i: VI) -> VO {
  var o: VO;
  let t = g.cam.w; let R = i.a.w; let sz = i.b.z;
  let ta = t * i.b.y + i.b.x;
  let P = i.a.xyz + vec3f(R * cos(ta), 2.5 * sin(2. * ta + i.b.w), R * .75 * sin(ta));
  let T = normalize(vec3f(-R * sin(ta), 5. * cos(2. * ta + i.b.w) * 1., R * .75 * cos(ta))) * sign(i.b.y);
  var S = normalize(cross(vec3f(0., 1., 0.), T)); var Up = cross(T, S);
  let bank = -.28 * sign(i.b.y);
  let S2 = S * cos(bank) + Up * sin(bank); Up = Up * cos(bank) - S * sin(bank); S = S2;
  let u = i.n.w; let au = abs(u);
  let fp = t * 1.25 + i.b.w * 5. - au * 1.2;
  let amp = .36 * pow(au, 1.6);
  var l = i.p;
  l.y += amp * sin(fp) - .1 * sin(t * 1.25 + i.b.w * 5.);
  let tail = ss(-.5, -1.9, l.z) * step(au, .001);
  l.x += tail * sin(t * .8 + l.z * 2.) * .12;
  let sl = (.576 * pow(au, .6) * sin(fp) - .43 * pow(au, 1.6) * cos(fp)) * sign(u);
  let ln = normalize(vec3f(i.n.x - sl * i.n.y, i.n.y + sl * i.n.x, i.n.z));
  let w = P + (S * l.x + Up * l.y + T * l.z) * sz;
  o.cp = g.vp * vec4f(w, 1.); o.wp = w; o.n = S * ln.x + Up * ln.y + T * ln.z; o.op = i.p; o.sd = i.b.w;
  o.lt = 1.;
  return o;
}
@fragment fn fsManta(i: VO) -> @location(0) vec4f {
  let dist = length(i.wp - g.cam.xyz);
  let ca = caus(i.wp, dist);
  let tx = textureSample(tD, sR, i.op.xz * .9 + i.sd);
  let n = normalize(i.n + (vec3f(tx.r, 0., tx.g) - vec3f(.5, 0., .5)) * .5); let L = g.sun.xyz; let V = normalize(g.cam.xyz - i.wp);
  let z = zone(i.wp); let vv = vol(i.wp);
  let top = ss(-.2, .25, normalize(i.n).y);
  let au = abs(i.op.x);
  let gill = ss(.25, .0, abs(au - .13) * 8.) * ss(.04, .12, i.op.z) * ss(.34, .26, i.op.z) * ss(.2, .8, sin(i.op.z * 110.));
  var belly = vec3f(.13, .17, .21) * (.6 + .6 * tx.b) * (1. - .7 * gill);
  belly = mix(belly, vec3f(.05, .08, .11), sat(ss(.45, .9, au + tx.a * .35) + ss(-.1, -.4, i.op.z + tx.b * .2) * .7));
  var back = vec3f(.04, .065, .10) + vec3f(.10, .16, .2) * ss(.5, .75, tx.a);
  back = mix(back, vec3f(.36, .46, .52), ss(.25, .0, abs(au - .26) * 3.5 + abs(i.op.z - .12) * 5.) * .8);
  var alb = mix(belly, back, top);
  let E = sunE(i.wp.y, z) * shadow4(i.wp + n * .3);
  let ndv = sat(dot(n, V));
  let ir = .5 + .5 * cos(TAU * (vec3f(0., .33, .67) + ndv * 1.1 + tx.a * 1.5 + .55));
  let fillN = vec3f(n.x, abs(n.y) * .55 + .25, n.z);
  var c = alb * (E * sat(dot(n, L)) * (.3 + ca) + amb(i.wp, fillN, vv.g, z) * 1.1 + side(i.wp, n, 1., z) * .8);
  c += ir * vec3f(.35, .16, .6) * (.07 + .22 * tx.b) * (.4 + E.g * .2 * ca.g) * (1. - top * .3);
  c += E * pow(sat(dot(n, normalize(V + L))), 30.) * .5 * (.3 + ca);
  let sky = amb(i.wp, vec3f(0., 1., 0.), vv.g, z);
  c += alb * sky * ca * .45 * (1. - top) + sky * pow(1. - ndv, 3.) * .35;
  return fin(fog(c, i.wp, z));
}

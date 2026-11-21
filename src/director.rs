// The cinematographer: a run of shots that follow the shark, each a few seconds long, joined by short dips to
// black. Every shot keeps its camera in open water (the rock collision field) and the shark in plain view; a shot
// that loses him cuts early to the next one.
use crate::math::*;
use crate::render::Cam;
use crate::shark::{v3::*, Shark, LENGTH};
use crate::world::Field;

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Shot { Chase, Side, Ambush, Below, Orbit, HeadOn, Wide, Fly }
const ORDER: [Shot; 12] = [Shot::Chase, Shot::Ambush, Shot::Side, Shot::Below, Shot::Orbit, Shot::HeadOn, Shot::Wide, Shot::Chase, Shot::Side, Shot::Fly, Shot::Ambush, Shot::Below];

pub struct Director {
    pub shot: Shot, n: usize, t: f64, dur: f64, seed: f64,
    pos: V3, vel: V3, look: V3, look_v: V3, anchor: V3, side: f64, tanh: f64, lost: f64, time: f64,
}

fn spring(x: &mut V3, v: &mut V3, target: V3, w: f64, dt: f64) {
    // critically damped
    for k in 0..3 { let a = (target[k] - x[k]) * w * w - 2.0 * w * v[k]; v[k] += a * dt; x[k] += v[k] * dt; }
}
fn h(x: f64) -> f64 { let s = (x * 127.1).sin() * 43758.5453; s - s.floor() }

impl Director {
    pub fn new(shark: &Shark, field: &Field) -> Director {
        let mut d = Director { shot: Shot::Chase, n: 0, t: 0.0, dur: 9.0, seed: 0.0, pos: shark.pos, vel: [0.0; 3], look: shark.pos, look_v: [0.0; 3], anchor: shark.pos, side: 1.0, tanh: 0.6, lost: 0.0, time: 0.0 };
        d.begin(Shot::Chase, shark, field);
        d
    }
    fn ok_spot(field: &Field, p: V3, target: V3) -> bool {
        field.dist(p[0], p[1], p[2]) < -1.0 && p[1] < -2.5 && field.clear_line(p, target, 0.25) && field.kelp_dist(p, 2.4) >= 2.4 && field.clear_of_kelp(p, target, 1.0)
    }
    /// sets up a shot; returns false when it has no good camera position right now
    fn setup(&mut self, s: Shot, sk: &Shark, field: &Field) -> bool {
        let f = sk.fwd; let up = [0.0, 1.0, 0.0]; let left = norm(cross(up, f));
        let p = sk.pos; let side = if h(self.seed * 3.1) > 0.5 { 1.0 } else { -1.0 };
        self.side = side; self.dur = 7.0 + 5.0 * h(self.seed * 7.7); self.vel = [0.0; 3];
        let cand: V3 = match s {
            Shot::Chase => { self.tanh = 0.5; add(add(p, mul(f, -LENGTH * 1.9)), [0.0, 0.9, 0.0]) }
            Shot::Side => { self.tanh = 0.42; add(add(p, mul(left, side * LENGTH * 2.2)), add(mul(f, LENGTH * 0.4), [0.0, 0.3, 0.0])) }
            Shot::Ambush => {
                // waits ahead on his way, off to one side; telephoto
                self.tanh = 0.3 + 0.08 * h(self.seed); self.dur = 9.0 + 3.0 * h(self.seed * 2.0);
                let ahead = add(p, mul(sk.vel(), 6.0));
                add(add(ahead, mul(left, side * 9.0)), [0.0, -0.6 + 2.0 * h(self.seed * 5.0), 0.0])
            }
            Shot::Below => { self.tanh = 0.6; add(add(p, [0.0, -LENGTH * 1.3, 0.0]), add(mul(f, LENGTH * 1.4), mul(left, side * 1.2))) }
            Shot::Orbit => { self.tanh = 0.48; add(add(p, mul(left, side * LENGTH * 2.0)), [0.0, 0.8, 0.0]) }
            Shot::HeadOn => { self.tanh = 0.45; self.dur = 6.0 + 2.0 * h(self.seed); add(add(p, mul(f, LENGTH * 3.0)), [0.0, 0.4, 0.0]) }
            Shot::Wide => {
                self.tanh = 0.68; self.dur = 8.0;
                add(add(p, mul(f, 7.0)), add(mul(left, side * 10.0), [0.0, 3.0, 0.0]))
            }
            Shot::Fly => { self.tanh = 0.55; add(add(p, mul(f, -LENGTH * 1.2)), add(mul(left, side * 1.6), [0.0, 0.9, 0.0])) }
        };
        if !Self::ok_spot(field, cand, p) { return false; }
        // the low angle needs room under him, or the camera ends up in the plants on the floor
        if s == Shot::Below && field.dist(cand[0], cand[1] - 1.5, cand[2]) > -1.0 { return false; }
        self.pos = cand; self.anchor = cand; self.look = p; self.look_v = [0.0; 3];
        true
    }
    fn begin(&mut self, want: Shot, sk: &Shark, field: &Field) {
        self.t = 0.0; self.lost = 0.0;
        // try the wanted shot, then the rest of the rota, then a close chase (always possible)
        let mut s = want;
        for i in 0..ORDER.len() {
            self.seed += 1.37;
            if self.setup(s, sk, field) { self.shot = s; return; }
            s = ORDER[(self.n + i + 1) % ORDER.len()];
        }
        self.shot = Shot::Chase; self.tanh = 0.55; self.dur = 5.0;
        self.pos = add(sk.pos, add(mul(sk.fwd, -LENGTH * 1.4), [0.0, 0.6, 0.0])); self.look = sk.pos;
    }
    pub fn step(&mut self, dt: f64, sk: &Shark, field: &Field) -> Cam {
        let dt = dt.min(0.05);
        self.t += dt; self.time += dt;
        let f = sk.fwd; let up = [0.0, 1.0, 0.0]; let left = norm(cross(up, f)); let p = sk.pos;
        let tq = self.t / self.dur;
        // where the camera wants to be this frame
        let goal: Option<V3> = match self.shot {
            Shot::Chase => Some(add(add(p, mul(f, -LENGTH * (1.9 - 0.4 * tq))), [0.0, 0.9 - 0.3 * tq, 0.0])),
            Shot::Side => Some(add(add(p, mul(left, self.side * LENGTH * (2.2 - 0.3 * tq))), add(mul(f, LENGTH * (0.4 - 0.9 * tq)), [0.0, 0.3, 0.0]))),
            Shot::Below => Some(add(add(p, [0.0, -LENGTH * 1.3, 0.0]), add(mul(f, LENGTH * (1.4 - 2.4 * tq)), mul(left, self.side * 1.2)))),
            Shot::Orbit => {
                let a = self.side * (PI / 2.0 + tq * 1.9);
                let dir = add(mul(f, -a.cos()), mul(left, a.sin()));
                Some(add(add(p, mul(dir, LENGTH * 2.0)), [0.0, 0.8 + 0.6 * (tq * PI).sin(), 0.0]))
            }
            Shot::HeadOn => Some(add(add(p, mul(f, LENGTH * (3.0 - 1.2 * tq))), [0.0, 0.4 + 0.3 * tq, 0.0])),
            Shot::Fly => Some(add(add(p, mul(f, -LENGTH * 1.2)), add(mul(left, self.side * 1.6), [0.0, 0.9, 0.0]))),
            // static cameras drift a little, like a diver holding still
            Shot::Ambush | Shot::Wide => Some(add(self.anchor, [0.15 * (self.time * 0.4).sin(), 0.1 * (self.time * 0.33).sin(), 0.15 * (self.time * 0.27).cos()])),
        };
        if let Some(g) = goal {
            let w = match self.shot { Shot::Ambush | Shot::Wide => 2.0, Shot::Fly => 4.0, Shot::HeadOn | Shot::Below => 2.6, _ => 1.8 };
            let before = self.pos;
            let mut x = self.pos; let mut v = self.vel;
            spring(&mut x, &mut v, g, w, dt);
            // the camera is a 0.5 m ball that slides along the rock
            let m = sub(x, before); let mut p2 = before; let mut vv = v;
            field.collide(&mut p2, m, &mut vv, 0.5);
            if p2[1] > -1.5 { p2[1] = -1.5; }
            self.pos = p2; self.vel = vv;
        }
        // aim: a little ahead of him, smoothed so he drifts in the frame
        let lead = match self.shot { Shot::Ambush | Shot::Wide => 0.6, Shot::HeadOn => 0.0, _ => 1.0 };
        let target = add(p, mul(f, LENGTH * 0.25 * lead));
        spring(&mut self.look, &mut self.look_v, target, match self.shot { Shot::Ambush => 2.2, Shot::Wide => 1.5, _ => 3.0 }, dt);

        // he must stay in view: a blocked line of sight for a moment, or drifting too far away, cuts the shot
        let dist = len(sub(p, self.pos));
        let seen = field.clear_line(self.pos, p, 0.0) && field.clear_of_kelp(self.pos, p, 0.6) && dist < 40.0 && dist > 1.2;
        if seen { self.lost = 0.0; } else { self.lost += dt; }
        let leaving = matches!(self.shot, Shot::Ambush) && dot(sub(p, self.pos), f) > 0.0 && dist > 16.0 && self.t > 4.0;
        if self.t > self.dur || self.lost > 0.6 || leaving {
            self.n += 1;
            let next = ORDER[self.n % ORDER.len()];
            self.begin(next, sk, field);
            return self.step(0.0, sk, field);
        }
        // a short dip to black between shots
        let fade = sstep(0.0, 0.45, self.t) * sstep(self.dur, self.dur - 0.45, self.t);
        let d = sub(self.look, self.pos);
        // a hint of handheld breathing
        let br = 0.004 * (self.time * 0.9).sin() + 0.002 * (self.time * 2.3).sin();
        Cam { pos: self.pos, yaw: d[0].atan2(-d[2]) + br, pitch: clamp(d[1].atan2(hypot2(d[0], d[2])) + br * 0.7, -1.45, 1.45), tanh: self.tanh, fade }
    }
}

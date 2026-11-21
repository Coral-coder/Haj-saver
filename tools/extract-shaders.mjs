// One-off helper: splits abyssal-garden's source/src/shaders.js into standalone .wgsl pieces.
// Shared preludes (noise, fs, g, b) are written once; src/shaders.rs glues them back together.
// The quality-dependent template holes become {{NAME}} placeholders, filled in at runtime.
//   node tools/extract-shaders.mjs ../abyssal-garden/source/src/shaders.js shaders/
import { readFileSync, writeFileSync } from 'node:fs';
const [src, out] = process.argv.slice(2);
const js = readFileSync(src, 'utf8')
  .replace(/\$\{LV > 1\}/g, '{{LOW}}')
  .replace(/\$\{AA > 1 \? '_multisampled' : ''\}/g, '{{MS}}')
  .replace(/\$\{SHADOW\}/g, '{{SHADOW}}').replace(/\$\{WATER\}/g, '{{WATER}}').replace(/\$\{RAYN\}/g, '{{RAYN}}');
if (/\$\{/.test(js)) throw new Error('unhandled template hole');
const names = [...js.matchAll(/^const (W_\w+|S_\w+) = /gm)].map((m) => m[1]);
const o = new Function(js + `\nreturn {${names.join(',')}};`)();
const pre = [['G_B_', o.S_B], ['G_FS_', o.S_G + o.W_FS], ['NOISE_FS_', o.W_NOISE + o.W_FS], ['NOISE_', o.W_NOISE], ['FS_', o.W_FS]];
const w = (n, s) => writeFileSync(`${out}/${n}.wgsl`, s.trim() + '\n');
w('noise', o.W_NOISE); w('fs', o.W_FS); w('g', o.S_G); w('b', o.S_B.slice(o.S_G.length));
for (const k of names) {
  if (!/^W_/.test(k) || k === 'W_NOISE' || k === 'W_FS') continue;
  const [p, s] = pre.find(([, s]) => o[k].startsWith(s));
  w(k.slice(2).toLowerCase(), o[k].slice(s.length)); console.log(k, '<-', p);
}

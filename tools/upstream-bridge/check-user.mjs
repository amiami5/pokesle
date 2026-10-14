// ローカルの設定とボックス(%APPDATA%\pokesle-rs\data\)を本家の Worker に通し、ボックス一覧のスコアを出す。
// 本家の画面や Rust 版の画面と数字を突き合わせるための確認用(厳選テーブルは使わない簡易版)。
// 使い方: node --experimental-transform-types tools/upstream-bridge/run.mjs check-user.mjs
//   CFG=<config.json>  設定ファイルを差し替える(ブラウザ版の config を貼った JSON など)
//   UP_SRC=<dir>       本家の src を差し替える(別コミットとの比較用)
//   SPLIT=1            本家と同じく workerNum 個に分割して計算する / TOP=1 概算上位 / DET=1 内訳
import { readFileSync } from 'node:fs';
import { pathToFileURL, fileURLToPath } from 'node:url';
import path from 'node:path';
const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
const up = (p) => import(pathToFileURL(path.join(process.env.UP_SRC ?? path.join(root, 'upstream/src'), p)).href);
globalThis.self = globalThis;
let captured = null, posted = [];
globalThis.addEventListener = (t, fn) => { if (t === 'message') captured = fn; };
globalThis.postMessage = (m) => posted.push(m);
await up('models/pokemon-box/pokemon-box-worker.ts');
const fn = captured;
const call = async (data) => { posted = []; await fn({ data }); const l = posted.filter((x) => x.status !== 'progress').at(-1); if (l?.status === 'error') throw new Error(l.body); return l?.body; };
const dir = process.env.APPDATA + '/pokesle-rs/data/';
const config = JSON.parse(readFileSync((process.env.CFG ?? dir + 'config.json'), 'utf8'));
const box = JSON.parse(readFileSync(dir + 'box.json', 'utf8')).list.map((p, i) => ({ ...p, index: i }));
await call({ type: 'config', config });
// 本家は workerNum 個の Worker に分割して計算する(分割単位でボックス内の他メンバーを見る処理がある)
const wn = process.env.SPLIT ? config.workerNum : 1;
const basic = [];
for (let i = 0; i < wn; i++) {
  const s = Math.floor((box.length * i) / wn), e = Math.floor((box.length * (i + 1)) / wn);
  basic.push(...(await call({ type: 'basic', pokemonList: box.slice(s, e), evaluateTable: null })));
}
const assist = [];
for (let i = 0; i < wn; i++) {
  const s = Math.floor((basic.length * i) / wn), e = Math.floor((basic.length * (i + 1)) / wn);
  assist.push(...(await call({ type: 'assist', pokemonList: basic.slice(s, e), basedPokemonList: basic })));
}
console.log('field', config.simulation.field, 'cooking', config.simulation.cookingType);
const ids = process.env.IDS ? process.env.IDS.split(',').map((s) => Number(s.trim())).filter(Number.isFinite) : null;
const targets = ids ? assist.filter((p) => ids.includes(p.box.index + 1)) : assist.slice(0, 10);
for (const p of targets) console.log(p.box.index + 1, p.base.name, p.lv, (p.score ?? p.tmpScore)?.toFixed(1));
if (process.env.TOP) {
  const top = [...basic].sort((a, b) => b.tmpScore - a.tmpScore).slice(0, 12);
  for (const p of top) console.log('TOP', p.box.index + 1, p.base.name, p.lv, p.tmpScore.toFixed(1), p.box.skillLv, p.box.foodList.join('/'), p.box.subSkillList.join('/'), p.box.nature);
}
if (process.env.DET) {
  for (const p of targets)
    console.log('DET', p.box.index + 1, p.base.name, 'e', p.energyPerDay.toFixed(1), 'sup', p.supportEnergyPerDay.toFixed(1), 'shard', p.shard.toFixed(1), 'supShard', p.supportShardPerDay.toFixed(1), 'score', p.score.toFixed(1), 'skill', p.base.skill.name, 'sk/d', p.skillPerDay.toFixed(2));
}

// OCR 結果の解釈(parseData)を本家のまま Node で実行し、Rust 側の突き合わせ用データを書き出す。
// 使い方: node --experimental-transform-types tools/upstream-bridge/run.mjs golden-ocr.mjs
import { mkdirSync, writeFileSync } from 'node:fs';
import { fileURLToPath, pathToFileURL } from 'node:url';
import path from 'node:path';

const here = path.dirname(fileURLToPath(import.meta.url));
const root = path.resolve(here, '../..');
const outDir = path.join(root, 'crates/pokesle-ocr/tests/golden');
mkdirSync(outDir, { recursive: true });
const up = (p) => import(pathToFileURL(path.join(root, 'upstream/src', p)).href);

const { parseData, getTextSimilarity, normalizeText } = await up('models/ocr/pokemon-status-image-reader.ts');
const { default: Pokemon } = await up('data/pokemon.ts');
const { default: SubSkill } = await up('data/sub-skill.ts');
const { default: Nature } = await up('data/nature.ts');
const { default: Skill } = await up('data/skill.ts');

// ---- 乱数(シード固定) ----
let seed = 20261023;
const rand = () => (seed = (seed * 1103515245 + 12345) % 2147483648) / 2147483648;
const pick = (list) => list[Math.floor(rand() * list.length)];
const chance = (p) => rand() < p;

// ---- OCR の誤読を真似る ----
const confusions = { 'っ': 'つ', 'ッ': 'ツ', 'ー': '一', 'ぱ': 'は', 'ば': 'は', 'ガ': 'カ', 'ズ': 'ス', '0': 'O', '1': 'l' };
function corrupt(text, rate) {
  return [...text].map((c) => {
    if (!chance(rate)) return c;
    if (confusions[c] && chance(0.6)) return confusions[c];
    return chance(0.3) ? '' : chance(0.5) ? c + c : pick([...'ぁあいうえおアイウエオ0O1l']);
  }).join('');
}

// ---- 文字列の類似度・正規化 ----
const textPairs = [];
const pool = [...Pokemon.list.map((x) => x.name), ...Nature.list.map((x) => x.name), ...SubSkill.list.map((x) => x.name), ...Skill.list.map((x) => x.name)];
for (let i = 0; i < 400; i++) {
  const expected = pick(pool);
  const actual = chance(0.2) ? pick(pool) : (chance(0.3) ? `${pick(['', 'ab ', '｜'])}${corrupt(expected, 0.15)}${pick(['', ' 12', '|x'])}` : corrupt(expected, 0.2));
  textPairs.push({ expected, actual, similarity: getTextSimilarity(expected, actual), normalized: normalizeText(actual) });
}
for (const text of ['Ｌｖ．４５　ピカチュウ', '＄ｕｐ_', 'ボポポー', 'ボーーー', 'おてつだだい', 'おてつただだい', 'おてつだだだい', '１時間２分３秒', '。．']) {
  textPairs.push({ expected: text, actual: text, similarity: getTextSimilarity(text, text), normalized: normalizeText(text) });
}

// ---- parseData ----
function makeCase() {
  const pokemon = pick(Pokemon.list.filter((x) => !x.kaihou || chance(0.3)));
  const lv = 1 + Math.floor(rand() * 100);
  const nature = pick(Nature.list);
  const skill = pokemon.skill;
  const subSkillList = [];
  while (subSkillList.length < 5) {
    const s = pick(SubSkill.list);
    if (!subSkillList.includes(s.name)) subSkillList.push(s.name);
  }
  const nick = chance(0.3) ? pick(['もちこ', 'ぴか', 'ねむねむ', 'ポテト']) : pokemon.name;

  const lines = [];
  const add = (text, x0, y0, extra = {}) => lines.push({ text, x0, y0, x1: x0 + text.length * 22, y1: y0 + 40, ...extra });
  add(`Lv.${chance(0.15) ? corrupt(String(lv), 0.5) || '1' : lv} ${corrupt(nick, 0.1)}`, 80, 100);
  add(corrupt(skill.name.replace(/\(.+\)$/, chance(0.5) ? '' : '$&'), 0.08), 80, 300);
  add(`Lv.${1 + Math.floor(rand() * 7)}`, 600, 335);
  add(corrupt(`せいかく ${nature.name}`, 0.1), 80, 420);
  const rarityOf = (n) => SubSkill.map[n].rarity;
  for (let i = 0; i < 5; i += 2) {
    const a = subSkillList[i], b = subSkillList[i + 1];
    const y = 520 + i * 30;
    if (b && chance(0.5)) {
      // 2列が1行に結合された形
      add(`${corrupt(a, 0.08)}${pick(['  ', ' | ', '｜'])}${corrupt(b, 0.08)}`, 80, y, { subSkillRarity: rarityOf(a) });
    } else {
      add(corrupt(a, 0.08), 80, y, { subSkillRarity: chance(0.8) ? rarityOf(a) : 1 });
      if (b) add(corrupt(b, 0.08), 600, y + (chance(0.3) ? 10 : 0), { subSkillRarity: rarityOf(b) });
    }
  }
  add(corrupt('詳細ステータス', 0.1), 80, 800);
  add(`おてつだい時間 ${Math.floor(rand() * 90)}分${Math.floor(rand() * 60)}秒`, 80, 860);
  add(`持ち物 ${Math.floor(rand() * 40) + 5}個`, 80, 900);
  add('説明文 ほげほげ', 80, 950, { subSkillRarity: 1 });

  const withRarity = (l) => ({ ...l, subSkillRarity: l.subSkillRarity ?? undefined });
  const lineList = lines.map(withRarity);
  const primaryLineList = chance(0.7) ? lineList : lineList.filter(() => chance(0.8));
  const foodRecognitionList = [0, 1, 2].map((slot) => {
    const names = pokemon.foodNameList;
    const real = pokemon.foodList?.[slot]?.name ?? null;
    const matchList = (chance(0.8) ? [real ?? pick(names)] : [pick(names)])
      .concat([pick(names)]).map((n, i) => ({ name: n, score: 1 - i * 0.2 - rand() * 0.1 }));
    return {
      name: chance(0.7) ? matchList[0].name : (chance(0.5) ? null : pick(names)),
      num: chance(0.6) ? (pokemon.foodNumListMap[matchList[0].name]?.[slot] ?? null) : (chance(0.5) ? null : 1 + Math.floor(rand() * 10)),
      region: { x: 0, y: 0, width: 1, height: 1, occluded: chance(0.1) },
      matchList,
      empty: chance(0.1),
    };
  });
  const rawText = lines.map((l) => l.text).join('\n') + (chance(0.3) ? `\n${Math.floor(rand() * 600)}個` : '');
  const input = { rawText, lineList, foodRecognitionList, primaryLineList, sourceCoordinateLineList: lineList };
  const result = parseData(rawText, lineList, foodRecognitionList, primaryLineList, lineList);
  return { input, result, truth: { pokemon: pokemon.name, lv, nature: nature.name, subSkillList } };
}

const cases = Array.from({ length: 300 }, makeCase);
const hit = cases.filter((c) => c.result.pokemonName == c.truth.pokemon).length;
console.log(`parseData: ${cases.length} 件(種族の推定が正解 ${hit} 件)`);
writeFileSync(path.join(outDir, 'ocr-status.json'), JSON.stringify({ textPairs, cases }));
console.log('書き出しました:', path.join(outDir, 'ocr-status.json'));

// 本家の Worker をそのまま Node で実行し、Rust 側の突き合わせ用データを書き出す。
// 使い方: node --experimental-transform-types tools/upstream-bridge/run.mjs golden.mjs
import { mkdirSync, writeFileSync } from 'node:fs';
import { fileURLToPath, pathToFileURL } from 'node:url';
import path from 'node:path';

const here = path.dirname(fileURLToPath(import.meta.url));
const root = path.resolve(here, '../..');
const outDir = path.join(root, 'crates/pokesle-core/tests/golden');
mkdirSync(outDir, { recursive: true });
const up = (p) => import(pathToFileURL(path.join(root, 'upstream/src', p)).href);

// ---- Worker 実行環境のスタブ ----
globalThis.self = globalThis;
let captured = null;
let posted = [];
globalThis.addEventListener = (type, fn) => {
  if (type === 'message') captured = fn;
};
globalThis.postMessage = (msg) => posted.push(msg);

async function loadWorker(p) {
  captured = null;
  await up(p);
  if (!captured) throw new Error(`worker listener not found: ${p}`);
  const fn = captured;
  return async (data) => {
    posted = [];
    await fn({ data });
    const last = posted.filter((x) => x.status !== 'progress').at(-1);
    if (last?.status === 'error') throw new Error(last.body);
    return last?.body;
  };
}

const boxWorker = await loadWorker('models/pokemon-box/pokemon-box-worker.ts');
const evaluateWorker = await loadWorker('models/simulation/evaluate-simulator.ts');
const subSkillCombinationWorker = await loadWorker('models/sub-skill-combination-worker.ts');

const { default: Pokemon } = await up('data/pokemon.ts');
const { default: SubSkill } = await up('data/sub-skill.ts');
const { default: Nature } = await up('data/nature.ts');
const { Food } = await up('data/food_and_cooking.ts');
const { default: defaultConfig } = await up('data/default-config.ts');

// ---- 乱数(シード固定) ----
let seed = 20261008;
const rand = () => (seed = (seed * 1103515245 + 12345) % 2147483648) / 2147483648;
const pick = (list) => list[Math.floor(rand() * list.length)];

function makeBox(num) {
  const list = [];
  for (let i = 0; i < num; i++) {
    const p = pick(Pokemon.list);
    const abc = pick(['AAA', 'AAB', 'AAC', 'ABA', 'ABB', 'ABC']);
    const foodList = abc
      .split('')
      .map((c) => p.foodList[Math.min(c.charCodeAt(0) - 65, p.foodList.length - 1)].name);
    const subs = [...SubSkill.list]
      .map((x) => [rand(), x])
      .sort((a, b) => a[0] - b[0])
      .slice(0, 5)
      .map(([, x]) => x.name);
    list.push({
      name: p.name,
      lv: 1 + Math.floor(rand() * 65),
      skillLv: rand() < 0.3 ? 1 + Math.floor(rand() * p.skill.effect.length) : null,
      foodList,
      subSkillList: subs,
      nature: pick(Nature.list).name,
      shiny: false,
      fix: null,
      sleepTime: pick([null, 100, 600, 1200, 2500]),
      nextExp: null,
      memo: null,
      favorite: false,
      index: i,
    });
  }
  return list;
}

const clone = (x) => JSON.parse(JSON.stringify(x));
function variant(fn) {
  const c = clone(defaultConfig);
  fn(c);
  return c;
}
const eventBonus = (types, specialties, v) => ({
  target: { types, specialties },
  berry: 0,
  food: 0,
  skillRate: 1,
  skillLv: 0,
  bag: 0,
  bagRate: 1,
  ...v,
});

const configs = {
  default: variant(() => {}),
  cyan_camp_border: variant((c) => {
    c.simulation.field = 'シアンの砂浜';
    c.simulation.campTicket = true;
    c.simulation.expectType.food = 1;
    c.simulation.expectType['食材ゲットS'] = 1;
    c.simulation.expectType['エナジーチャージS'] = 1;
    c.simulation.cookingType = 'デザート';
  }),
  ex_food_event: variant((c) => {
    c.simulation.field = 'ワカクサ本島EX';
    c.simulation.fieldEx = 2;
    c.simulation.berryList = ['キー', 'ヒメリ', 'オレン'];
    c.simulation.eventBonusList = [
      eventBonus({ ノーマル: true }, { スキル: true }, { berry: 1, food: 1, skillRate: 1.5, skillLv: 1, bag: 5, bagRate: 1.1 }),
    ];
    c.simulation.eventBonus.skill.foodGet = 1.5;
    c.simulation.eventBonus.skill.berryBurst = 2;
  }),
  genkifull_day: variant((c) => {
    c.simulation.genkiFull = true;
    c.simulation.cookingType = 'サラダ';
    c.teamSimulation.day = 3;
    c.teamSimulation.initialCookingChange = 0.2;
  }),
  cooking_all: variant((c) => {
    c.simulation.cookingType = '';
  }),
  cyan_ex_skill: variant((c) => {
    c.simulation.field = 'シアンの砂浜EX';
    c.simulation.fieldEx = 3;
    c.simulation.berryList = ['オレン', 'ドリ', 'ウブ'];
    c.sleepTime = 7;
    c.checkFreq = 6;
    c.simulation.bagOverOperation = false;
  }),
};

const NUM_FIELDS = [
  'lv', 'berryEnergy', 'berryNum', 'bEpH', 'foodRate', 'foodNum', 'fEpH', 'pickupEnergyPerHelp', 'bag',
  'bagFullHelpNum', 'fixedSkillLv', 'skillRate', 'ceilSkillRate', 'speed', 'speedBonus', 'baseDayHelpNum',
  'morningHealGenki', 'selfHeal', 'otherHeal', 'dayHelpRate', 'nightHelpRate', 'normalHelpNum', 'berryHelpNum',
  'bEpD', 'fEpD', 'foodNumPerDay', 'skillPerDay', 'skillEnergy', 'skillEnergyPerDay', 'energyPerDay', 'shard',
  'burstBonus', 'cookingChanceEffect', 'tmpScore', 'score', 'supportEnergyPerDay', 'supportShardPerDay',
];
const num = (x) => (typeof x === 'number' && Number.isFinite(x) ? x : x == null ? null : String(x));
const summarize = (p) => ({
  name: p.base.name,
  boxIndex: p.box?.index ?? null,
  bagOverOperation: !!p.bagOverOperation,
  ...Object.fromEntries(NUM_FIELDS.map((k) => [k, num(p[k])])),
  foods: Object.fromEntries(Food.list.map((f) => [f.name, num(p[f.name] ?? 0)])),
});

// ---- ボックス概算(pokemon-box-worker の basic → assist) ----
const box = makeBox(150);
const boxCases = [];
for (const [name, config] of Object.entries(configs)) {
  await boxWorker({ type: 'config', config: clone(config) });
  const basic = await boxWorker({ type: 'basic', pokemonList: clone(box), evaluateTable: null });
  const assist = await boxWorker({ type: 'assist', pokemonList: basic, basedPokemonList: basic });
  boxCases.push({ name, config, result: assist.map(summarize) });
}
writeFileSync(path.join(outDir, 'box.json'), JSON.stringify({ box, cases: boxCases }));
console.log(`box: ${boxCases.length} configs x ${boxCases[0].result.length} pokemon`);

// ---- 厳選評価(evaluate-simulator) ----
const evalConfig = clone(defaultConfig);
const subSkillCombinationList = await subSkillCombinationWorker({ config: evalConfig });
const evalTargets = [
  'ピカチュウ', 'カメックス', 'フシギバナ', 'ゲンガー', 'ハピナス', 'カビゴン', 'ラティオス', 'トゲデマル',
  'メタモン', 'エーフィ', 'キュワワー', 'ニンフィア', 'ダークライ', 'ミミッキュ', 'プクリン',
]
  .filter((n) => Pokemon.map[n])
  .map((n) => Pokemon.map[n]);
const evaluateCases = [];
for (const lv of [30, 60]) {
  const foodCombinationList = lv < 60 ? ['00', '01'] : ['000', '012'];
  const subSkillNum = lv < 50 ? 2 : lv < 70 ? 3 : lv < 80 ? 4 : 5;
  const result = await evaluateWorker({
    lv,
    config: evalConfig,
    pokemonList: evalTargets,
    foodCombinationList,
    subSkillCombinationList: subSkillCombinationList[subSkillNum],
    scoreForHealerEvaluate: 10000,
    scoreForSupportEvaluate: 150,
  });
  const table = {};
  for (const [name, byFood] of Object.entries(result.result)) {
    table[name] = {};
    for (const [food, perc] of Object.entries(byFood)) {
      table[name][food] = Object.fromEntries(
        ['energy', 'berry', 'food', 'skill', 'food1', 'food2', 'food3'].map((k) => [k, perc[k].map((x) => num(x.score))]),
      );
    }
  }
  evaluateCases.push({
    lv,
    foodCombinationList,
    pokemon: evalTargets.map((p) => p.name),
    healer: result.scoreForHealerEvaluateList,
    support: result.scoreForSupportEvaluateList,
    table,
  });
}
writeFileSync(
  path.join(outDir, 'evaluate.json'),
  JSON.stringify({ config: evalConfig, subSkillCombinationList, cases: evaluateCases }),
);
console.log(`evaluate: ${evaluateCases.length} levels, ${evalTargets.length} pokemon`);

// ---- チームシミュレーション(simulation.vue の simulation() 相当 + team-simulator) ----
const teamWorker = await loadWorker('models/simulation/team-simulator.ts');
const { default: PokemonFilter } = await up('models/pokemon-filter.js');
const { default: Berry } = await up('data/berry.ts');

function teamBox() {
  const list = makeBox(36);
  const special = [
    ['トゲデマル', 'げんきエールS'], ['プラスル'], ['マイナン'], ['メタモン'], ['ハピナス'], ['ラティアス'], ['ラティオス'],
    ['カビゴン'], ['ニンフィア'], ['ダークライ'],
  ];
  for (const [name] of special) {
    const p = Pokemon.map[name];
    if (!p) continue;
    list.push({
      name, lv: 60, skillLv: null,
      foodList: [p.foodList[0].name, p.foodList[Math.min(1, p.foodList.length - 1)].name, p.foodList[Math.min(2, p.foodList.length - 1)].name],
      subSkillList: ['おてつだいボーナス', 'スキル確率アップM', 'げんき回復ボーナス', 'ゆめのかけらボーナス', 'おてつだいスピードM'],
      nature: pick(Nature.list).name, shiny: false, fix: null, sleepTime: 600, nextExp: null, memo: null, favorite: false,
      index: list.length,
    });
  }
  return list;
}

async function teamSimulation(baseConfig, boxList, { targetDay, targetHour, initialCookingChange, initialCookingPowerUp, beforeEnergy }) {
  const customConfig = clone({ ...baseConfig, teamSimulation: { ...baseConfig.teamSimulation, beforeEnergy } });
  if (baseConfig.foodUnlimited) customConfig.foodDefaultNum = Object.fromEntries(Food.list.map((f) => [f.name, 9999]));
  if (baseConfig.simulation.mode == 2) {
    customConfig.simulation.berryEnergyWeight = 0;
    customConfig.simulation.skillEnergyIgnore = 0;
    customConfig.simulation.shardWeight = 0;
  }
  if (targetDay != -1) {
    customConfig.simulation.shardToEnergy = customConfig.selectEvaluate.shardEnergyRate / (7 - targetDay);
    customConfig.teamSimulation.day = targetDay;
    customConfig.teamSimulation.timeLength = targetHour != 24 ? targetHour / 24 : null;
  }
  customConfig.teamSimulation.initialCookingChange = initialCookingChange / 100;
  customConfig.teamSimulation.initialCookingPowerUp = initialCookingPowerUp;

  const filtered = PokemonFilter.filter(boxList, customConfig.simulation.filter).pokemonList;
  await boxWorker({ type: 'config', config: clone(customConfig) });
  const basic = await boxWorker({ type: 'basic', pokemonList: clone(filtered), evaluateTable: null });
  const pokemonList = await boxWorker({ type: 'assist', pokemonList: basic, basedPokemonList: basic });

  const fixedPokemonList = clone(pokemonList.filter((x) => x.box?.fix == 1));
  let rest = pokemonList.filter((x) => x.box?.fix == null);
  const pickup = 5 - new Set(fixedPokemonList.map((x) => x.box.index)).size;
  const ts = customConfig.teamSimulation;
  let targets = [];
  let sorted;
  if (baseConfig.simulation.mode != 2) {
    sorted = rest.toSorted((a, b) => b.bEpD - a.bEpD); targets.push(...sorted.slice(0, ts.maxRankBerry)); rest = sorted.slice(ts.maxRankBerry);
    sorted = rest.toSorted((a, b) => b.fEpD - a.fEpD); targets.push(...sorted.slice(0, ts.maxRankFood)); rest = sorted.slice(ts.maxRankFood);
  } else {
    sorted = rest.toSorted((a, b) => b.fEpD - a.fEpD); targets.push(...sorted.slice(0, ts.maxRankFood + ts.maxRankBerry)); rest = sorted.slice(ts.maxRankFood + ts.maxRankBerry);
  }
  sorted = rest.toSorted((a, b) => b.score - a.score); targets.push(...sorted.slice(0, ts.maxRankAll)); rest = sorted.slice(ts.maxRankAll);
  sorted = rest.toSorted((a, b) => b.energyPerDay - a.energyPerDay); targets.push(...sorted.slice(0, ts.maxRankNotSupport)); rest = sorted.slice(ts.maxRankNotSupport);
  const targetNum = targets.length;
  targets = [...targets.filter((p) => p.otherHeal), ...targets.filter((p) => !p.otherHeal && p.selfHeal), ...targets.filter((p) => !p.otherHeal && !p.selfHeal)];
  for (const berry of Berry.list) {
    if (ts.require.typeNum[berry.type] && !targets.some((x) => x.base.type == berry.type)) {
      const top = rest.find((x) => x.base.type == berry.type);
      if (!top) return [];
      targets.push(top);
    }
  }
  targets = clone(targets);
  const topList = [];
  for (let i = 0; i <= targetNum - pickup; i++) topList.push(i);
  targets.sort((a, b) => b.score - a.score);
  return await teamWorker({
    type: 'simulate', targetNum, pickup, pattern: 1, topList,
    fixedPokemonList, targetPokemonList: targets, boxPokemonList: clone(boxList), config: customConfig,
  });
}

const tBox = teamBox();
const smallRank = (c) => {
  c.workerNum = 1;
  c.teamSimulation.maxRankBerry = 3;
  c.teamSimulation.maxRankFood = 4;
  c.teamSimulation.maxRankAll = 5;
  c.teamSimulation.maxRankNotSupport = 5;
};
const teamCases = [
  { name: 'week', config: variant(smallRank), req: { targetDay: -1, targetHour: 24, initialCookingChange: 0, initialCookingPowerUp: 0, beforeEnergy: 0 } },
  { name: 'sunday_half', config: variant((c) => { smallRank(c); c.simulation.cookingType = 'サラダ'; }), req: { targetDay: 6, targetHour: 12, initialCookingChange: 20, initialCookingPowerUp: 10, beforeEnergy: 50000 } },
  { name: 'cooking_all', config: variant((c) => { smallRank(c); c.simulation.cookingType = ''; }), req: { targetDay: -1, targetHour: 24, initialCookingChange: 0, initialCookingPowerUp: 0, beforeEnergy: 0 } },
  { name: 'wed_nightcap_req', config: variant((c) => { smallRank(c); c.teamSimulation.nightCapPikachu = 12; c.teamSimulation.require.typeNum['フェアリー'] = 1; c.simulation.remainFoodMode = 1; c.simulation.remainFoodRate = 0.5; }), req: { targetDay: 2, targetHour: 24, initialCookingChange: 0, initialCookingPowerUp: 0, beforeEnergy: 0 } },
  { name: 'fixed_genkifull', config: variant((c) => { smallRank(c); c.simulation.genkiFull = true; c.foodUnlimited = true; c.simulation.cookingType = 'デザート'; }), req: { targetDay: -1, targetHour: 24, initialCookingChange: 0, initialCookingPowerUp: 0, beforeEnergy: 0 }, fix: [36, 37] },
];
const teamOut = [];
for (const tc of teamCases) {
  const boxList = clone(tBox);
  for (const i of tc.fix ?? []) boxList[i].fix = 1;
  const result = await teamSimulation(tc.config, boxList, tc.req);
  teamOut.push({
    name: tc.name,
    config: tc.config,
    req: tc.req,
    box: boxList,
    result: result.map((r) => ({
      score: r.score,
      energy: r.energy,
      rawEnergy: r.rawEnergy,
      skillShard: r.skillShard,
      bonusShard: r.bonusShard,
      remainFoodEnergy: r.remainFoodEnergy,
      members: r.pokemonList.map((p) => ({ name: p.base.name, boxIndex: p.box?.index ?? null, bagOverOperation: !!p.bagOverOperation, skillPerDay: p.skillPerDay ?? null, energyPerDay: p.energyPerDay ?? null })),
      cookingList: r.cookingList.map((c) => ({ name: c.cooking.name, energy: c.energy, potSize: c.potSize })),
      foodNum: r.foodNum,
      cookingPowerUpEffectList: r.cookingPowerUpEffectList,
    })),
  });
  console.log(`team ${tc.name}: ${result.length} results`);
}
writeFileSync(path.join(outDir, 'team.json'), JSON.stringify(teamOut));

// ---- 厳選テーブルありのボックス概算(仮定計算・進化・純粋ミント) ----
{
  const fixBox = makeBox(30).map((p) => ({ ...p, lv: 10 + (p.lv % 40), nextExp: p.index % 3 == 0 ? 50 : null }));
  const lvList = [30, 50, 60, 70];
  const afterNames = [...new Set(fixBox.flatMap((p) => Pokemon.map[p.name].afterList))].filter((n) => !Pokemon.map[n].kaihou);
  const table = { scoreForHealerEvaluate: {}, scoreForSupportEvaluate: {} };
  const cfg = clone(defaultConfig);
  const combos = await subSkillCombinationWorker({ config: cfg });
  for (const lv of lvList) {
    const foodCombinationList = lv < 60 ? ['00', '01'] : ['000', '001', '002', '010', '011', '012'];
    const subSkillNum = lv < 50 ? 2 : lv < 70 ? 3 : lv < 80 ? 4 : 5;
    const healer = 10000 * (1.025 ** (lv - 1));
    const support = 150 * (1.025 ** (lv - 1));
    const r = await evaluateWorker({
      lv, config: cfg, pokemonList: afterNames.map((n) => Pokemon.map[n]), foodCombinationList,
      subSkillCombinationList: combos[subSkillNum], scoreForHealerEvaluate: healer, scoreForSupportEvaluate: support,
    });
    table.scoreForHealerEvaluate[lv] = { '': { energy: healer } };
    table.scoreForSupportEvaluate[lv] = { '': { energy: support } };
    for (const [name, byFood] of Object.entries(r.result)) {
      table[name] ??= {};
      table[name][lv] = {};
      for (const [food, perc] of Object.entries(byFood)) {
        table[name][lv][food] = Object.fromEntries(['energy', 'berry', 'food', 'skill', 'food1', 'food2', 'food3'].map((k) => [k, perc[k].map((x) => x.score)]));
      }
    }
  }
  const fixConfigs = {
    fix_lv_evolve: variant((c) => {
      c.simulation.fix = true; c.simulation.fixLv = 50; c.simulation.fixEvolve = true; c.simulation.fixBorder = 30;
      c.simulation.fixCheckList = false; c.simulation.fixSubSkillSeed = true; c.simulation.fixSkillSeed = 1; c.pureMint = true;
    }),
    fix_candy: variant((c) => {
      c.simulation.fix = true; c.simulation.fixResourceMode = 1; c.simulation.fixBorderSpecialty = 40; c.simulation.fixCheckList = false;
      c.candy.bag.s = 30; c.candy.bag.m = 10; c.candy.shard = 300000; c.simulation.selectType = 1;
      for (const k of Object.keys(c.candy.bag)) if (!['s', 'm', 'l'].includes(k)) c.candy.bag[k] = 200;
    }),
  };
  const fixCases = [];
  for (const [name, config] of Object.entries(fixConfigs)) {
    await boxWorker({ type: 'config', config: clone(config) });
    const basic = await boxWorker({ type: 'basic', pokemonList: clone(fixBox), evaluateTable: table });
    fixCases.push({
      name,
      config,
      result: basic.map((p) => ({
        ...summarize(p),
        useCandy: p.useCandy ?? null,
        useShard: p.useShard ?? null,
        best: Object.fromEntries(
          Object.entries(p.evaluateResult ?? {}).filter(([, v]) => v.best).map(([lv, v]) => [lv, Object.fromEntries(
            ['energy', 'berry', 'food', 'skill', 'specialty'].map((k) => [k, v.best[k] ? { name: v.best[k].name, score: num(v.best[k].score), value: num(v.best[k].value) } : null]),
          )]),
        ),
      })),
    });
  }
  writeFileSync(path.join(outDir, 'box-fix.json'), JSON.stringify({ box: fixBox, table, cases: fixCases }));
  console.log(`box-fix: ${fixCases.map((c) => c.result.length).join(',')}`);

  // ---- チェックリスト(check-list.vue の一覧 + 仮定計算の該当判定) ----
  const { CheckList } = await up('models/check-list.ts');
  const target = (v) => ({ type: 'condition', all: false, specialties: { きのみ: false, 食材: false, スキル: false, オール: false }, skillNameList: [], friendPoint: null, pokemonNameList: [], ...v });
  const cond = (t, v) => ({ target: t, aaa: true, aab: true, aac: true, aba: true, abb: true, abc: true, energyBorder: null, specialtyBorder: null, alsoFoodAndSkill: false, ...v });
  // とくい「オール」のポケモンはチェックリストの対象外のため除外する
  const lastAfter = afterNames.filter((n) => Pokemon.map[n].isLast && Pokemon.map[n].fieldList.length && Pokemon.map[n].specialty != 'オール');
  const checklistConfigs = {
    cl_a: variant((c) => {
      const cl = c.summary.checklist;
      cl.pokemonCondition.list = [
        cond(target({ specialties: { きのみ: true, 食材: false, スキル: false, オール: false } }), { energyBorder: 20 }),
        cond(target({ specialties: { きのみ: false, 食材: true, スキル: false, オール: false } }), { aac: false, aba: false, abb: false, abc: false, specialtyBorder: 30, alsoFoodAndSkill: true }),
        cond(target({ specialties: { きのみ: false, 食材: false, スキル: true, オール: false } }), { energyBorder: 10, specialtyBorder: 50, alsoFoodAndSkill: true }),
        cond(target({ type: 'pokemon', pokemonNameList: lastAfter.slice(0, 3) }), { energyBorder: 0 }),
      ];
      cl.pokemonCondition.selectLv = 'max';
      cl.pokemonCondition.disablePokemonMap[lastAfter[3]] = true;
      Object.assign(cl.food, { borderType: 0, borderLv: 60, borderRate: 95, borderValue: 50, targetValue: 60 });
      Object.assign(cl.skill, { borderLv: 60, borderRate: 95, borderValue: 40, targetValue: 70, skillSpecialtyOnly: false });
    }),
    cl_b: variant((c) => {
      const cl = c.summary.checklist;
      c.simulation.selectType = 1;
      cl.pokemonCondition.list = [
        cond(target({ specialties: { きのみ: true, 食材: true, スキル: true, オール: false } }), { energyBorder: 90, specialtyBorder: 95, alsoFoodAndSkill: true }),
      ];
      cl.pokemonCondition.selectLv = 50;
      Object.assign(cl.food, { borderType: 1, borderLv: 60, borderRate: null, borderValue: 30, targetValue: 50 });
      Object.assign(cl.skill, { borderLv: 70, borderRate: null, borderValue: 60, targetValue: 80, skillSpecialtyOnly: true });
      cl.food.enableMap[Food.list[0].name] = false;
    }),
  };
  const hitName = (h) => (h.type == 'pokemon' ? `pokemon:${h.pokemon.name}` : h.type == 'food' ? `food:${h.food.name}` : `skill:${h.skill.name}`);
  const checklistCases = [];
  for (const [name, config] of Object.entries(checklistConfigs)) {
    // 一覧(仮定計算なし・いつ育なし)
    const pageConfig = { ...config, simulation: { ...config.simulation, bagOverOperation: false, fix: false } };
    await boxWorker({ type: 'config', config: clone(pageConfig) });
    const basic = await boxWorker({ type: 'basic', pokemonList: clone(fixBox), evaluateTable: table });
    const all = await boxWorker({ type: 'assist', pokemonList: basic, basedPokemonList: basic });
    for (const p of all) {
      p.foodCombination = p.box.foodList.map((x) => String.fromCharCode(p.base.foodList.findIndex((f) => f.name == x) + 65)).join('');
    }
    const list = all;
    const checkList = new CheckList(config, table);
    const food = checkList.foodCheckList(config, list);
    const skill = checkList.skillCheckList(config, list);
    const pokemon = checkList.pokemonCheckList(config, list, null, food, skill);
    const idx = (p) => (p ? list.indexOf(p) : null);
    const page = {
      boxIndexList: list.map((p) => p.box.index),
      pokemon: {
        rows: pokemon.dataList.map((r) => ({
          name: r.name, subName: r.subName, checked: !!r.checked, hit: idx(r.pokemon), target: r.target,
          energyScore: num(r.energyScore), specialtyScore: num(r.specialtyScore),
          list: r.table.dataList.map((x) => [idx(x.pokemon), num(x.score)]),
        })),
        targetPokemonList: pokemon.targetPokemonList,
      },
      food: {
        rows: food.dataList.map((r) => ({
          food: r.food.name, bestFoodScore: num(r.bestFoodScore), maxNum: num(r.maxNum), border: num(r.border), score: num(r.score), lv: r.lv,
          checked: !!r.checked, checkedPokemonList: r.checkedPokemonList, hit: idx(r.pokemon), rate: num(r.rate), note: r.note,
          list: r.table.dataList.map((x) => [idx(x.pokemon), num(x.score), x.lv]),
        })),
        targetPokemonList: food.targetPokemonList,
      },
      skill: {
        rows: skill.dataList.map((r) => ({
          skill: r.skill.name, bestSkillScore: num(r.bestSkillScore), border: num(r.border), score: num(r.score), lv: r.lv,
          checked: !!r.checked, checkedPokemonList: r.checkedPokemonList, hit: idx(r.pokemon), rate: num(r.rate), note: r.note,
          list: r.table.dataList.map((x) => [idx(x.pokemon), num(x.score), x.lv]),
        })),
        targetPokemonList: skill.targetPokemonList,
      },
    };

    // 仮定計算(チェックリスト該当で進化)
    const fixConfig = clone(config);
    Object.assign(fixConfig.simulation, { fix: true, fixCheckList: true, fixBorder: null, fixBorderSpecialty: null, fixEvolve: true });
    await boxWorker({ type: 'config', config: clone(fixConfig) });
    const fixResult = await boxWorker({ type: 'basic', pokemonList: clone(fixBox), evaluateTable: table });
    checklistCases.push({
      name,
      config,
      fixConfig,
      page,
      fix: fixResult.map((p) => ({ name: p.base.name, boxIndex: p.box.index, lv: p.lv, energyPerDay: num(p.energyPerDay), hit: (p.hitCheckList ?? []).map(hitName) })),
    });
  }
  writeFileSync(path.join(outDir, 'check-list.json'), JSON.stringify({ box: fixBox, table, cases: checklistCases }));
  console.log(`check-list: ${checklistCases.map((c) => `${c.page.pokemon.rows.length}/${c.page.food.rows.length}/${c.page.skill.rows.length}`).join(',')}`);

  // ---- ボックス集計(box-summary の きのみ・食材・スキル・ポケモン) ----
  {
    const { default: Berry } = await up('data/berry.ts');
    const { default: Skill } = await up('data/skill.ts');
    const { Cooking } = await up('data/food_and_cooking.ts');
    const { default: PokemonSimulator } = await up('models/simulation/pokemon-simulator.ts');
    const summaryConfigs = {
      sum_default: variant(() => {}),
      sum_fix: variant((c) => {
        c.simulation.fixEvolve = true; c.simulation.fixSubSkillSeed = true; c.simulation.fixSkillSeed = 1; c.simulation.selectType = 1;
        c.selectEvaluate.levelList = { 30: false, 50: true, 60: true, 70: true };
      }),
    };
    const simulate = async (config, list, tbl) => {
      await boxWorker({ type: 'config', config: clone(config) });
      const basic = await boxWorker({ type: 'basic', pokemonList: clone(list), evaluateTable: tbl });
      return await boxWorker({ type: 'assist', pokemonList: basic, basedPokemonList: basic });
    };
    const withSim = (config, extra) => ({
      ...config,
      simulation: { ...config.simulation, field: 'ワカクサ本島', fieldBonus: 0, berryList: ['', '', ''], mode: 0, ...extra },
    });
    const settings = [{ fix: false }, { fix: true, fixLv: 30 }, { fix: true, fixLv: 50 }, { fix: true, fixLv: 60 }, { fix: true, fixLv: 70 }];
    const ident = (p) => (p ? { name: p.base.name, index: p.box?.index ?? null } : null);
    const summaryCases = [];

    for (const [cname, config] of Object.entries(summaryConfigs)) {
      const c = { name: cname, config };

      // きのみ
      const sims = [];
      for (const s of settings) sims.push(await simulate(withSim(config, s), fixBox, table));
      c.berry = Berry.list.map((berry) => ({
        name: berry.name,
        cols: sims.map((list) => {
          const l = list.filter((x) => x.base.berry.name == berry.name).sort((a, b) => b.bEpD - a.bEpD);
          return { value: num(l[0]?.bEpD), pokemon: ident(l[0]) };
        }),
      }));

      // 食材
      const foodSims = [];
      for (const s of settings) foodSims.push(await simulate(withSim(config, { ...s, genkiFull: true, helpBonus: 3 }), fixBox, table));
      const foodMax = Object.fromEntries(Food.list.map((x) => [x.name, { num: 0, pokemon: null, foodList: [] }]));
      Pokemon.list.forEach((pokemon) => {
        const foodHelpNum = pokemon.foodRate * 86400 / pokemon.help / 0.45;
        for (const food1 of pokemon.foodList.filter((x) => x?.name && x.numList[0])) {
          for (const food2 of pokemon.foodList.filter((x) => x?.name && x.numList[1])) {
            for (const food3 of pokemon.foodList.filter((x) => x?.name && x.numList[2])) {
              const numMap = {};
              numMap[food1.name] = (numMap[food1.name] ?? 0) + food1.numList[0] / 3;
              numMap[food2.name] = (numMap[food2.name] ?? 0) + food2.numList[1] / 3;
              numMap[food3.name] = (numMap[food3.name] ?? 0) + food3.numList[2] / 3;
              for (let [name, n] of Object.entries(numMap)) {
                n *= foodHelpNum;
                if (foodMax[name].num < n) foodMax[name] = { num: n, pokemon, foodList: [food1.name, food2.name, food3.name] };
              }
            }
          }
        }
      });
      c.food = Food.list.map((food) => ({
        name: food.name,
        require: Math.max(...Cooking.list.map((x) => x.foodList.find((f) => f.name == food.name)?.num ?? 0)),
        max: num(foodMax[food.name].num),
        maxPokemon: foodMax[food.name].pokemon?.name ?? null,
        maxFoodList: foodMax[food.name].foodList,
        cols: foodSims.map((list) => {
          const l = [...list].sort((a, b) => b[food.name] - a[food.name]);
          return { value: num(l[0]?.[food.name]), pokemon: ident(l[0]) };
        }),
      }));

      // スキル
      const subSkillList = ['おてつだいボーナス', 'おてつだいスピードM', 'スキル確率アップM', 'スキル確率アップS', '所持数アップL'];
      const boxPokemonList = [];
      for (const base of Pokemon.list.filter((p) => p.afterList.length == 1 && p.afterList[0] == p.name)) {
        for (const combo of ['000', '001', '002', '010', '011', '012']) {
          const foodList = combo.split('').map(Number).map((f) => (Food.map[base.foodList[f]?.name] ? base.foodList[f].name : null));
          if (foodList.includes(null)) continue;
          boxPokemonList.push({ name: base.name, lv: 100, foodList, subSkillList, nature: 'なまいき' });
        }
      }
      const skillPokemonList = await simulate({ ...config, simulation: { ...config.simulation, expectType: { food: 0, skill: 0 }, helpBonus: 5 } }, boxPokemonList, null);
      const skillNum = Object.fromEntries(Skill.list.map((x) => [x.name, { num: 0, pokemon: null }]));
      for (const p of skillPokemonList) {
        if ((skillNum[p.base.skill.name]?.num ?? 0) < p.skillPerDay) skillNum[p.base.skill.name] = { num: p.skillPerDay, pokemon: p.base.name };
      }
      c.skill = Skill.list.map((skill) => ({
        name: skill.name,
        max: num(skillNum[skill.name]?.num ?? 0),
        maxPokemon: skillNum[skill.name]?.pokemon ?? null,
        cols: sims.map((list) => {
          const l = list.filter((x) => x.base.skill.name == skill.name).sort((a, b) => b.skillPerDay - a.skillPerDay);
          return { value: num(l[0]?.skillPerDay), pokemon: ident(l[0]) };
        }),
      }));

      // ポケモン(食材構成ごと・総合/とくい分野)
      const pokemonSim = await simulate(withSim(config, { fix: false }), fixBox, table);
      const lvs = Object.entries(config.selectEvaluate.levelList).filter(([, e]) => e).map(([lv]) => Number(lv));
      await PokemonSimulator.isReady;
      const foodNumSim = new PokemonSimulator({ ...config, simulation: { ...config.simulation, expectType: { food: 0, skill: 0 } } }, PokemonSimulator.MODE_ABOUT);
      const foodNumList = [];
      for (const base of Pokemon.list.filter((p) => p.afterList.length == 1 && p.afterList[0] == p.name)) {
        for (const combo of ['000', '001', '002', '010', '011', '012']) {
          const foodList = combo.split('').map(Number).map((f) => (Food.map[base.foodList[f]?.name] ? base.foodList[f].name : null));
          if (foodList.includes(null)) continue;
          const pokemon = foodNumSim.fromBox({ name: base.name, lv: 60, foodList, subSkillList: [], nature: 'まじめ' });
          foodNumSim.calcStatus(pokemon, 0);
          foodNumSim.calcTeamHeal([pokemon]);
          foodNumSim.calcHelp(pokemon, 0, 0);
          foodNumList.push(pokemon);
        }
      }
      c.pokemon = {};
      for (const [key, eachFood, type] of [['all_energy', false, 'energy'], ['food_specialty', true, 'specialty']]) {
        const targets = eachFood ? foodNumList.map((p) => ({ base: p.base, foodList: p.box.foodList })) : Pokemon.list.filter((x) => x.isLast).map((x) => ({ base: x, foodList: null }));
        c.pokemon[key] = targets.map((target) => {
          let max = null;
          let maxP = null;
          const byLv = {};
          for (const lv of lvs) {
            let l = pokemonSim.filter((x) => x.evaluateResult[lv]?.[target.base.name]);
            if (eachFood) l = l.filter((p) => target.foodList.every((f, i) => f == p.box?.foodList[i]));
            l.sort((a, b) => b.evaluateResult[lv][target.base.name][type].score - a.evaluateResult[lv][target.base.name][type].score);
            const score = l[0]?.evaluateResult[lv][target.base.name][type].score;
            byLv[lv] = { value: num(score), pokemon: ident(l[0]) };
            if (max == null || max < score) {
              max = score;
              maxP = l[0];
            }
          }
          return { name: target.base.name, foodList: target.foodList, max: { value: num(max), pokemon: ident(maxP) }, byLv };
        });
      }
      summaryCases.push(c);
    }
    writeFileSync(path.join(outDir, 'box-summary.json'), JSON.stringify({ cases: summaryCases }));
    console.log(`box-summary: ${summaryCases.length}`);
  }
}

// ---- 育成に必要なアメ・ゆめのかけら(Exp.calcRequireInfo) ----
{
  const { default: Exp } = await up('data/exp.ts');
  const seeds = [...new Set(Pokemon.list.map((p) => p.seed).filter((x) => x))];
  const candyList = [
    { boostMultiply: 2, boostShard: 5, bag: {} },
    { boostMultiply: 2, boostShard: 5, bag: Object.fromEntries(seeds.map((s) => [s, Math.floor(rand() * 400)])) },
    { boostMultiply: 1.5, boostShard: 4, bag: Object.fromEntries(seeds.map((s) => [s, pick([0, 10, 50, 100, 300, 1000, 5000])])) },
  ];
  const expCases = [];
  for (const candy of candyList) {
    for (let i = 0; i < 300; i++) {
      const p = pick(Pokemon.list);
      const lv = 1 + Math.floor(rand() * 70);
      const pokemon = {
        name: p.name,
        lv,
        nature: pick(Nature.list).name,
        training: pick([null, 1, lv, lv + 1, lv + 5, 25, 30, 50, 60, 65, 70, 75]),
        nextExp: pick([null, 0, 1, 30, 120, 500]),
      };
      const result = Exp.calcRequireInfo(pokemon, Nature.map[pokemon.nature], { candy });
      expCases.push({ candy, pokemon, result: Object.fromEntries(Object.entries(result).map(([k, v]) => [k, num(v)])) });
    }
  }
  writeFileSync(path.join(outDir, 'exp.json'), JSON.stringify(expCases));
  console.log(`exp: ${expCases.length}`);
}

// ---- 食材ストック(food-stock-worker と food_stock.vue の有効料理・候補数) ----
{
  const foodStockWorker = await loadWorker('models/simulation/food-stock-worker.ts');
  const { Cooking } = await up('data/food_and_cooking.ts');
  const cases = [];
  const variants = [
    { name: 'default', fn: () => {} },
    { name: 'weights', fn: (c) => { c.foodStock.weight = { カレー: 100, サラダ: 30, デザート: 0 }; } },
    { name: 'small-bag', fn: (c) => { c.foodStock.bagSize = 300; c.foodStock.cookingNum = 12; } },
    { name: 'range', fn: (c) => { c.foodStock.minFoodNum = 20; c.foodStock.maxFoodNum = 40; c.foodStock.candidateNum = 5; } },
    { name: 'disabled', fn: (c) => {
      for (const cooking of Cooking.list.filter((_, i) => i % 3 == 0)) c.simulation.enableCooking[cooking.name] = false;
    } },
    { name: 'exclude-max', fn: (c) => {
      for (const cooking of Cooking.list.filter((_, i) => i % 4 == 0)) c.simulation.cookingSettings[cooking.name].lv = Cooking.maxRecipeLv;
      c.foodStock.excludeMaxRecipeLv = true;
      c.simulation.mode = 1;
    } },
    { name: 'lv-type', fn: (c) => { c.simulation.cookingRecipeLvType1 = false; c.simulation.cookingRecipeLvType2 = true; c.simulation.cookingRecipeFixLv = 30; c.foodStock.cookingNum = 7.9; } },
  ];
  for (const [vi, v] of variants.entries()) {
    const config = clone(defaultConfig);
    v.fn(config);
    const enable = Cooking.getEnableCookingList({ ...config, simulation: { ...config.simulation, mode: 0 } })
      .filter((c) => !config.foodStock.excludeMaxRecipeLv || config.simulation.cookingSettings[c.name].lv < Cooking.maxRecipeLv);
    const types = ['カレー', 'サラダ', 'デザート'];
    const maxCandidateNum = Math.min(...types.filter((t) => config.foodStock.weight[t] > 0).map((t) => enable.filter((c) =>
      c.type == t && c.foodNum >= Math.max(Number(config.foodStock.minFoodNum) || 0, 0) && c.foodNum <= Math.max(Number(config.foodStock.maxFoodNum) || 0, 0)).length));
    const candidates = Math.min(Math.max(Math.floor(Number(config.foodStock.candidateNum) || 1), 1), maxCandidateNum);
    for (const seed of [1, 12345, 1760000000000 + vi * 100003]) {
      const input = {
        bagSize: Math.max(Math.floor(Number(config.foodStock.bagSize) || 0), 0),
        cookingNum: Math.min(Math.max(Math.floor(Number(config.foodStock.cookingNum) || 1), 1), 21),
        candidateNum: candidates,
        minFoodNum: Math.max(Math.floor(Number(config.foodStock.minFoodNum) || 0), 0),
        maxFoodNum: Math.max(Math.floor(Number(config.foodStock.maxFoodNum) || 0), 0),
        weights: clone(config.foodStock.weight),
        cookingList: enable.map((c) => ({ name: c.name, type: c.type, foodNum: c.foodNum, foodList: c.foodList.map((f) => ({ name: f.name, num: f.num })), energy: c.fixEnergy })),
        seed,
      };
      const result = await foodStockWorker(clone(input));
      cases.push({ variant: v.name, config, enableNames: enable.map((c) => c.name), maxCandidateNum, input, result });
    }
  }
  writeFileSync(path.join(outDir, 'food-stock.json'), JSON.stringify(cases));
  console.log(`food-stock: ${cases.length}`);
}

// ---- データ閲覧(data/*.vue の派生値) ----
{
  const { Cooking } = await up('data/food_and_cooking.ts');
  const out = {};

  // ポケモンの表(ceil / skillRate2 / skillNum)
  out.pokemon = Pokemon.list.map((p) => {
    const ceil = p.specialty == 'スキル' ? 144000 / p.help : 78;
    const skillRate2 = p.skillRate / (1 - (1 - p.skillRate) ** ceil);
    return { name: p.name, ceil: num(ceil), skillRate2: num(skillRate2), skillNum: num(skillRate2 * 86400 / p.help) };
  });

  // 食材の表
  const foodConfigs = {
    default: variant(() => {}),
    mode1: variant((c) => {
      c.simulation.mode = 1;
      c.simulation.cookingSettings[Cooking.list[0].name].lv = Cooking.maxRecipeLv;
      for (const cooking of Cooking.list.filter((_, i) => i % 5 == 0)) c.simulation.enableCooking[cooking.name] = false;
      c.simulation.cookingRecipeLvType1 = false; c.simulation.cookingRecipeLvType2 = true; c.simulation.cookingRecipeFixLv = 20;
    }),
  };
  const types = ['カレー', 'サラダ', 'デザート'];
  out.food = Object.entries(foodConfigs).map(([name, config]) => {
    const enable = Cooking.getEnableCookingList(config);
    return {
      name, config,
      rows: Food.list.map((food) => {
        const r = { name: food.name };
        for (const type of types) {
          const l = enable.filter((x) => x.type == type && x.foodList.some((f) => f.name == food.name));
          r[`require_${type}`] = Math.max(...l.map((x) => x.foodList.find((f) => f.name == food.name)?.num ?? 0), 0);
          r[`bestTypeRate_${type}`] = Math.max(...l.map((x) => x.rate), 1);
          r[`maxCookingEnergy_${type}`] = Math.max(...l.map((x) => x.energy), 0);
          r[`maxEnergy_${type}`] = food.energy * r[`bestTypeRate_${type}`] * (r[`bestTypeRate_${type}`] > 1 ? Cooking.maxRecipeBonus : 1);
        }
        r.average = (r['maxCookingEnergy_カレー'] + r['maxCookingEnergy_サラダ'] + r['maxCookingEnergy_デザート']) / 3;
        r.geometricMean = Math.cbrt(r['maxCookingEnergy_カレー'] * r['maxCookingEnergy_サラダ'] * r['maxCookingEnergy_デザート']);
        r.maxEnergy = Math.max(r['maxEnergy_カレー'], r['maxEnergy_サラダ'], r['maxEnergy_デザート']);
        for (const k of Object.keys(r)) if (k != 'name') r[k] = num(r[k]);
        return r;
      }),
    };
  });

  // 食材期待値・スキル期待値
  const expectCases = [];
  for (const [kind, p] of [
    ['food', { lv: 60, nature: 'れいせい', subSkill: 3, speed: 35, genkiFull: true }],
    ['food', { lv: 40, nature: 'おっとり', subSkill: 1, speed: 20, genkiFull: false }],
    ['skill', { lv: 60, nature: 'なまいき', subSkill: 3, speed: 35, genkiFull: false }],
    ['skill', { lv: 25, nature: 'せっかち', subSkill: 2, speed: 10, genkiFull: true }],
  ]) {
    const targetList = Pokemon.list.filter((x) => x.afterList.length == 1 && x.afterList[0] == x.name);
    const combos = p.lv < 30 ? ['0'] : p.lv < 60 ? ['00', '01'] : ['000', '001', '002', '010', '011', '012'];
    const sub = kind == 'food' ? ['食材確率アップS', '食材確率アップM'] : ['スキル確率アップS', 'スキル確率アップM'];
    const subSkillList = p.subSkill == 1 ? [sub[0]] : p.subSkill == 2 ? [sub[1]] : p.subSkill == 3 ? sub : [];
    const list = [];
    for (const base of targetList) {
      for (const combo of combos) {
        const foodList = combo.split('').map(Number).map((f) => (Food.map[base.foodList[f]?.name] ? base.foodList[f].name : null));
        if (foodList.includes(null)) continue;
        list.push({ name: base.name, lv: p.lv, foodList, subSkillList, nature: p.nature });
      }
    }
    const config = clone({
      ...defaultConfig,
      simulation: { ...defaultConfig.simulation, expectType: { food: 0, skill: 0 }, helpBonus: (p.speed ?? 0) / 5, genkiFull: p.genkiFull },
    });
    await boxWorker({ type: 'config', config: clone(config) });
    const basic = await boxWorker({ type: 'basic', pokemonList: clone(list), evaluateTable: null });
    const result = await boxWorker({ type: 'assist', pokemonList: basic, basedPokemonList: basic });
    expectCases.push({
      kind, param: p,
      rows: result.map((x) => ({
        name: x.base.name, foodList: x.box.foodList,
        foods: Food.list.map((f) => num(x[f.name])),
        skillRate: num(x.skillRate), ceilSkillRate: num(x.ceilSkillRate), skillCeil: num(x.skillCeil), skillPerDay: num(x.skillPerDay),
      })),
    });
  }
  out.expect = expectCases;
  writeFileSync(path.join(outDir, 'data-view.json'), JSON.stringify(out));
  console.log(`data-view: ${out.pokemon.length} pokemon, ${out.food.length} food configs, ${expectCases.map((c) => c.rows.length).join('/')}`);
}

// ---- 簡易診断(EvaluateTable.simulateTemporary + 入力個体の概算) ----
{
  const cases = [];
  const defs = [
    { pokemon: { name: 'ピカチュウ', lv: 1, skillLv: 1, foodList: ['とくせんリンゴ', 'とくせんリンゴ', 'あったかジンジャー'], subSkillList: ['きのみの数S', 'おてつだいスピードS', '食材確率アップS', 'スキル確率アップM', '最大所持数アップS'], nature: 'がんばりや' }, tweak: () => {} },
    { pokemon: { name: 'イーブイ(ホリデー)', lv: 1, skillLv: 1, foodList: ['モーモーミルク', 'モーモーミルク', 'リラックスカカオ'], subSkillList: ['おてつだいボーナス', 'スキルレベルアップM', '', 'おてつだいスピードM', 'スキル確率アップS'], nature: 'いじっぱり' }, tweak: (c) => { c.tmpEvaluate.scoreForHealerEvaluate = 20000; c.tmpEvaluate.scoreForSupportEvaluate = 300; c.tmpEvaluate.sleepTime = 7; c.tmpEvaluate.checkFreq = 4; } },
  ];
  for (const d of defs) {
    const config = clone(defaultConfig);
    d.tweak(config);
    // simulateTemporary(config, name) の再現
    const fixed = clone(config);
    const evaluateConfig = fixed.tmpEvaluate;
    fixed.selectEvaluate = evaluateConfig;
    fixed.selectEvaluate.levelList = { 10: false, 25: false, 30: true, 50: true, 60: true, 70: true, 80: true };
    const after = Pokemon.map[d.pokemon.name].afterList.map((n) => Pokemon.map[n]).filter((x) => x != null);
    const combos = await subSkillCombinationWorker({ config: fixed });
    const table = { scoreForHealerEvaluate: {}, scoreForSupportEvaluate: {} };
    for (const lv of [30, 50, 60, 70, 80]) {
      const foodCombinationList = lv < 60 ? ['00', '01'] : ['000', '001', '002', '010', '011', '012'];
      const n = lv < 50 ? 2 : lv < 70 ? 3 : lv < 80 ? 4 : 5;
      const rate = 1.025 ** (lv - 1) / (1 - 0.002 * (lv - 1));
      const healer = evaluateConfig.scoreForHealerEvaluate * rate;
      const support = evaluateConfig.scoreForSupportEvaluate * rate;
      const r = await evaluateWorker({ lv, config: fixed, pokemonList: after, foodCombinationList, subSkillCombinationList: combos[n] ?? [[1]], scoreForHealerEvaluate: healer, scoreForSupportEvaluate: support });
      table.scoreForHealerEvaluate[lv] = { '': { energy: healer } };
      table.scoreForSupportEvaluate[lv] = { '': { energy: support } };
      for (const [name, res] of Object.entries(r.result)) {
        table[name] ??= {};
        table[name][lv] = {};
        for (const [food, perc] of Object.entries(res)) {
          table[name][lv][food] = Object.fromEntries(['energy', 'berry', 'food', 'skill'].map((k) => [k, perc[k].map((x) => x.score)]));
        }
      }
    }
    // evaluate() の後半
    const c = clone(config);
    c.tmpEvaluate.levelList = { 10: false, 25: false, 30: true, 50: true, 60: true, 70: true, 80: true };
    c.selectEvaluate = c.tmpEvaluate;
    c.sleepTime = c.tmpEvaluate.sleepTime;
    c.checkFreq = c.tmpEvaluate.checkFreq;
    c.workerNum = c.tmpEvaluate.workerNum;
    c.pureMint = false;
    await boxWorker({ type: 'config', config: clone(c) });
    const [sim] = await boxWorker({ type: 'basic', pokemonList: [clone(d.pokemon)], evaluateTable: table });
    const evolutions = {};
    for (const [name, byKey] of Object.entries(sim.evaluateResult[30] ?? {})) {
      if (name == 'best') continue;
      evolutions[name] = Object.fromEntries([30, 50, 60, 70, 80].map((lv) => {
        const e = sim.evaluateResult[lv]?.[name];
        return [lv, {
          rate: Object.fromEntries(['energy', 'berry', 'food', 'skill'].map((k) => [k, num(e?.[k]?.rate)])),
          berryEnergy: num(e?.diagnostic?.berryEnergy),
          skillActivationNum: num(e?.diagnostic?.skillActivationNum),
          foodNumMap: Object.fromEntries(Object.entries(e?.diagnostic?.foodNumMap ?? {}).map(([k, v]) => [k, num(v)])),
        }];
      }));
    }
    cases.push({ config, pokemon: d.pokemon, evolutions });
  }
  writeFileSync(path.join(outDir, 'tmp-evaluate.json'), JSON.stringify(cases));
  console.log(`tmp-evaluate: ${cases.map((c) => Object.keys(c.evolutions).length).join(',')}`);
}

// ---- 旧形式設定の変換(config.ts の読み込み時の移行処理) ----
{
  const { migrateEvaluateConfig } = await up('models/evaluate-setting.ts');
  const { migratePokemonTargetFriendPoint, migrateCheckListPokemonCondition, needsCheckListPokemonConditionMigration } = await up('models/pokemon-target.ts');
  const { default: Skill } = await up('data/skill.ts');
  const migrate = (cookieConfig) => {
    // config.ts と同じ手順(mergeObject の前まで)
    const savedConfigVersion = Number(cookieConfig.v) || 0;
    if (cookieConfig.selectEvaluate.silverSeedUse != null) {
      cookieConfig.selectEvaluate.silverSeed = Object.fromEntries(
        SubSkill.list.filter((x) => x.next != null).map((x) => [x.name, cookieConfig.selectEvaluate.silverSeedUse]),
      );
    }
    if (cookieConfig.pokemonList?.cleaning?.mintCalculation == null) {
      cookieConfig.pokemonList ??= {};
      cookieConfig.pokemonList.cleaning ??= {};
      cookieConfig.pokemonList.cleaning.mintCalculation = cookieConfig.pokemonList.cleaning.useMint ? 2 : 0;
    }
    if (savedConfigVersion < 20260919) {
      migrateEvaluateConfig(cookieConfig.selectEvaluate);
      migrateEvaluateConfig(cookieConfig.tmpEvaluate);
    }
    migratePokemonTargetFriendPoint(cookieConfig.selectEvaluate?.ruleList);
    migratePokemonTargetFriendPoint(cookieConfig.tmpEvaluate?.ruleList);
    if (savedConfigVersion < 20260928 || needsCheckListPokemonConditionMigration(cookieConfig.summary?.checklist)) {
      migrateCheckListPokemonCondition(cookieConfig.summary?.checklist);
    }
    return cookieConfig;
  };

  const skills = Skill.list.map((s) => s.name);
  const specialties = ['きのみ', '食材', 'スキル', 'オール'];
  // 旧形式: とくいごとの skillLv 設定
  const oldSpecialty = (assign) => {
    const sp = Object.fromEntries(specialties.map((s) => [s, { berryEnergyRate: 100, foodEnergyRate: 100, foodGetRate: 100, skillLv: {} }]));
    assign(sp);
    return sp;
  };
  const legend = Pokemon.list.filter((p) => p.isLast && p.fieldList.length && p.legend).map((p) => p.name);
  const lastNames = Pokemon.list.filter((p) => p.isLast && p.fieldList.length).map((p) => p.name);
  const inputs = {
    old_specialty: (() => {
      const c = clone(defaultConfig);
      c.v = 20260101;
      c.selectEvaluate.silverSeedUse = true;
      delete c.selectEvaluate.silverSeed;
      delete c.selectEvaluate.ruleList;
      delete c.selectEvaluate.maxSkillLvSkillNameList;
      c.selectEvaluate.specialty = oldSpecialty((sp) => {
        for (const s of specialties) sp[s].skillLv[skills[0]] = { type: 2 };
        for (const s of specialties) sp[s].skillLv[skills[1]] = { type: 3, lv: 4 };
        sp['きのみ'].skillLv[skills[2]] = { type: 3, lv: 2 };
        sp['食材'].skillLv[skills[2]] = { type: 1 };
        sp['スキル'].skillLv[skills[2]] = { type: 2 };
        sp['オール'].skillLv[skills[2]] = { type: 3, lv: 5 };
        sp['きのみ'].skillLv[skills[3]] = { type: 2 };
        sp['食材'].skillLv[skills[3]] = { type: 2 };
        sp['スキル'].skillLv[skills[3]] = { type: 1 };
        sp['オール'].skillLv[skills[3]] = { type: 3, lv: 3 };
        // 一部のとくいにしか設定が無い(変換しない)
        sp['きのみ'].skillLv[skills[4]] = { type: 3, lv: 2 };
      });
      c.tmpEvaluate = clone(c.selectEvaluate);
      c.pokemonList.cleaning = { useMint: true };
      return c;
    })(),
    old_rulelist: (() => {
      const c = clone(defaultConfig);
      c.v = 20260101;
      c.selectEvaluate.ruleList = [
        { target: { all: true, specialties: {}, skillNameList: [], pokemonNameList: [] }, setting: { type: 'skillLv', skillName: skills[0], value: null, skillLvType: 2, skillLv: 1 } },
        { target: { all: true, specialties: {}, skillNameList: [], pokemonNameList: [] }, settingList: [{ type: 'berryEnergyRate', value: 50 }, { type: 'skillLv', skillName: skills[1], skillLvType: 2 }] },
        { target: { all: false, specialties: { きのみ: true }, skillNameList: [], pokemonNameList: [] }, setting: { type: 'skillLv', skillName: skills[2], skillLvType: 3, skillLv: 3 } },
        { target: { all: false, specialties: { きのみ: true }, skillNameList: [], pokemonNameList: [] }, setting: { type: 'skillLv', skillName: skills[3], skillLvType: 3, skillLv: 2 } },
        { target: { all: false, specialties: {}, skillNameList: [], pokemonNameList: [lastNames[0]] }, setting: { type: 'foodEnergyRate', value: 120 } },
        { target: { all: false, specialties: {}, skillNameList: [], pokemonNameList: [] }, settingList: [] },
      ];
      c.tmpEvaluate.ruleList = [{ target: { type: 'condition', all: false, specialties: { 食材: true }, skillNameList: [], pokemonNameList: [] }, settingList: [{ type: 'foodGetRate', value: 90 }] }];
      return c;
    })(),
    old_checklist: (() => {
      const c = clone(defaultConfig);
      c.v = 20260926;
      c.summary.checklist.pokemonCondition.list = [
        { type: 1, target: 'きのみ', aaa: true },
        { type: 1, target: 'スキル', aaa: false },
        { type: 2, target: lastNames[1] },
        { type: 2, target: null },
        { type: 3, target: null },
        { type: 2, target: legend[0] },
        { target: { all: true, specialties: {}, skillNameList: [], pokemonNameList: [] } },
        { target: { type: 'pokemon', specialties: { きのみ: true, 食材: false }, skillNameList: ['x'], pokemonNameList: legend, friendPoint: 20 } },
        { target: { all: false, specialties: { スキル: true }, skillNameList: [], pokemonNameList: [lastNames[0]], friendPoint: null } },
      ];
      return c;
    })(),
    current_checklist: (() => {
      const c = clone(defaultConfig);
      c.v = 20260928;
      c.summary.checklist.pokemonCondition.list = [
        { target: { type: 'pokemon', all: false, specialties: { きのみ: false, 食材: false, スキル: false, オール: false }, skillNameList: [], friendPoint: null, pokemonNameList: legend }, aaa: true },
        { target: { type: 'condition', all: false, specialties: { きのみ: true, 食材: false, スキル: false, オール: false }, skillNameList: [], friendPoint: 10, pokemonNameList: [] } },
      ];
      c.selectEvaluate.ruleList = [{ target: { type: 'condition', all: false, specialties: { きのみ: true }, skillNameList: [], pokemonNameList: [] }, settingList: [] }];
      return c;
    })(),
  };
  const cases = Object.entries(inputs).map(([name, input]) => ({ name, input: clone(input), output: migrate(clone(input)) }));
  writeFileSync(path.join(outDir, 'config-migrate.json'), JSON.stringify(cases));
  console.log(`config-migrate: ${cases.length}`);
}

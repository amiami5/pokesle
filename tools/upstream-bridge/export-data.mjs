// 本家のマスターデータと初期設定を、Rust 側が読む JSON に書き出す。
// 使い方: node tools/upstream-bridge/run.mjs export-data.mjs
// 派生値(料理倍率・進化段階など)も本家の計算結果をそのまま出力し、Rust 側では再計算しない。
import { writeFileSync } from 'node:fs';
import { fileURLToPath, pathToFileURL } from 'node:url';
import path from 'node:path';
import { execSync } from 'node:child_process';
import { parseHistory } from './history-parser.mjs';

const here = path.dirname(fileURLToPath(import.meta.url));
const root = path.resolve(here, '../..');
const outDir = path.join(root, 'crates/pokesle-core/data');
const up = (p) => import(pathToFileURL(path.join(root, 'upstream/src', p)).href);

const { default: Berry } = await up('data/berry.ts');
const { Food, Cooking } = await up('data/food_and_cooking.ts');
const { default: Skill, skillCategoryList } = await up('data/skill.ts');
const { default: Pokemon } = await up('data/pokemon.ts');
const { default: SubSkill } = await up('data/sub-skill.ts');
const { default: Nature } = await up('data/nature.ts');
const { default: Field } = await up('data/field.ts');
const { default: Exp } = await up('data/exp.ts');
const { default: NightCapPikachu } = await up('data/nightcap_pikachu.js');
const { default: defaultConfig } = await up('data/default-config.ts');
const { default: Version } = await up('models/version.js');

const imgName = (x) => (typeof x === 'string' ? path.basename(x) : null);

const master = {
  upstreamCommit: execSync('git -C upstream rev-parse HEAD', { cwd: root }).toString().trim(),
  version: { history: Version.HISTORY, evaluate: Version.EVALUATE },
  berries: Berry.list.map((b) => ({ ...b, img: imgName(b.img) })),
  berryTypeOrder: Berry.typeList.map((b) => b.type),
  foods: Food.list.map((f) => ({
    name: f.name,
    energy: f.energy,
    img: imgName(f.img),
    bestRate: f.bestRate,
    bestCooking: f.bestCooking?.name ?? null,
    bestTypeRate: f.bestTypeRate,
    maxEnergyByType: { カレー: f['maxEnergy_カレー'], サラダ: f['maxEnergy_サラダ'], デザート: f['maxEnergy_デザート'] },
    maxEnergy: f.maxEnergy,
  })),
  foodStats: {
    maxEnergy: Food.maxEnergy,
    averageEnergy: Food.averageEnergy,
    averageMaxCookedEnergy: Food.averageMaxCookedEnergy,
  },
  cookings: Cooking.list.map((c) => ({ ...c })),
  cookingStats: {
    recipeLvs: Object.entries(Cooking.recipeLvs).map(([lv, v]) => ({ lv: Number(lv), ...v })),
    maxRecipeLv: Cooking.maxRecipeLv,
    maxRecipeBonus: Cooking.maxRecipeBonus,
    potMax: Cooking.potMax,
    maxFoodNum: Cooking.maxFoodNum,
    maxEnergy: Cooking.maxEnergy,
    cookingPowerUpEnergy: Cooking.cookingPowerUpEnergy,
    cookingPowerUpEnergyAverage: Cooking.cookingPowerUpEnergyAverage,
    cookingPowerUpEnergyMap: Cooking.cookingPowerUpEnergyMap,
  },
  skillCategories: skillCategoryList,
  skills: Skill.list.map((s) => ({
    ...s,
    foodList: s.foodList ? s.foodList.map((f) => f.name) : null,
  })),
  metronomeTarget: Skill.metronomeTarget.map((s) => s.name),
  subSkills: SubSkill.list,
  natures: Nature.list,
  fields: Field.list,
  exp: Exp.list,
  nightCapPikachu: NightCapPikachu.list,
  pokemon: Pokemon.list.map((p) => ({
    ...p,
    skill: p.skill.name,
    berry: p.berry.name,
  })),
};

writeFileSync(path.join(outDir, 'master.json'), JSON.stringify(master, null, 1) + '\n');
const history = parseHistory(path.join(root, 'upstream/src/pages/history.vue'));
writeFileSync(path.join(outDir, 'history.json'), JSON.stringify(history, null, 1) + '\n');
writeFileSync(path.join(outDir, 'default-config.json'), JSON.stringify(defaultConfig, null, 1) + '\n');
console.log(`history: ${history.entries.length} entries`);
console.log(`exported: pokemon=${master.pokemon.length} skills=${master.skills.length} cookings=${master.cookings.length} (upstream ${master.upstreamCommit.slice(0, 7)})`);

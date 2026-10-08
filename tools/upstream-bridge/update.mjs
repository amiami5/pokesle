// 本家の更新を取り込む。
//
// 使い方(リポジトリのルートで):
//   node tools/upstream-bridge/update.mjs           本家の最新を取得 → 変更の分類表示 → データ書き出し → 正解データ生成 → テスト
//   node tools/upstream-bridge/update.mjs --check   取得だけ行い、前回取り込み以降の変更を表示する(何も書き換えない)
//   node tools/upstream-bridge/update.mjs --no-pull 本家は更新せず、今の upstream/ で書き出し・テストだけ行う
//   --from <commit>  変更一覧の基準コミットを指定する(既定は前回取り込んだコミット)
//
// 「前回取り込んだコミット」は crates/pokesle-core/data/master.json の upstreamCommit を基準にする。
import { execSync, spawnSync } from 'node:child_process';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import path from 'node:path';

const here = path.dirname(fileURLToPath(import.meta.url));
const root = path.resolve(here, '../..');
const args = new Set(process.argv.slice(2));
const BRANCH = 'master';

const sh = (cmd) => execSync(cmd, { cwd: root, encoding: 'utf8' }).trim();
function run(title, cmd, cmdArgs) {
  console.log(`\n== ${title}`);
  const r = spawnSync(cmd, cmdArgs, { cwd: root, stdio: 'inherit', shell: false });
  return r.status === 0;
}

// 本家の変更ファイルの分類。上から順に判定する。
const CATEGORIES = [
  {
    name: '計算ロジック(Rust への移植が必要)',
    action: 'diff を確認して crates/pokesle-core の対応箇所へ移植する',
    patterns: [
      /^src\/models\/simulation\//,
      /^src\/models\/help-rate\.ts$/,
      /^src\/models\/utils\/prob-border\.js$/,
      /^src\/models\/evaluate-setting\.ts$/,
      /^src\/models\/pokemon-target\.ts$/,
      /^src\/models\/pokemon-filter\.js$/,
      /^src\/models\/check-list\.ts$/,
      /^src\/models\/sub-skill-combination-worker\.ts$/,
      /^src\/models\/pokemon-box\//,
      /^src\/models\/config\.ts$/,
      /^src\/models\/utils\/merge-object\.js$/,
    ],
  },
  {
    name: '計算仕様書',
    action: '仕様の変更点を確認する(計算ロジックの変更とセットのことが多い)',
    patterns: [/^copilot\/pokemon-sleep-calculation-spec\.md$/],
  },
  {
    name: 'マスターデータ・初期設定(書き出しで自動反映)',
    action: '新しいスキル・サブスキルが追加されていないかだけ確認する',
    patterns: [/^src\/data\//, /^public\/version\.json$/, /^src\/type\.ts$/],
  },
  {
    name: '本家のビルド成果物・開発用ファイル',
    action: '無視してよい',
    hideFiles: true,
    patterns: [
      /^docs\//,
      /^components\.d\.ts$/,
      /auto-imports\.d\.ts$/,
      /^package(-lock)?\.json$/,
      /^copilot\/coding-guidelines\.md$/,
      /^(genki-learning|experiment|tools|test)\//,
    ],
  },
  {
    name: '画面(参考)',
    action: 'UI は独自設計のため任意。計算処理が画面側に書かれていないかだけ確認する',
    patterns: [/^src\/pages\//, /^src\/components\//],
  },
];

const master = JSON.parse(readFileSync(path.join(root, 'crates/pokesle-core/data/master.json'), 'utf8'));
const argv = process.argv.slice(2);
const fromIndex = argv.indexOf('--from');
const base = fromIndex >= 0 ? sh(`git -C upstream rev-parse ${argv[fromIndex + 1]}`) : master.upstreamCommit;

if (!args.has('--no-pull')) {
  console.log(`== 本家(${BRANCH})を取得`);
  sh(`git -C upstream fetch origin ${BRANCH}`);
  if (!args.has('--check')) sh(`git -C upstream checkout --detach origin/${BRANCH}`);
}
const target = args.has('--check') ? sh(`git -C upstream rev-parse origin/${BRANCH}`) : sh('git -C upstream rev-parse HEAD');

console.log(`\n前回取り込み: ${base.slice(0, 7)}  →  対象: ${target.slice(0, 7)}`);
if (base === target) {
  console.log('本家に変更はありません。');
} else {
  console.log('\n== 本家のコミット');
  console.log(sh(`git -C upstream log --oneline ${base}..${target}`) || '(なし)');

  const files = sh(`git -C upstream -c core.quotepath=false diff --name-only ${base} ${target}`).split('\n').filter(Boolean);
  const rest = new Set(files);
  for (const cat of CATEGORIES) {
    const hit = files.filter((f) => rest.has(f) && cat.patterns.some((p) => p.test(f)));
    hit.forEach((f) => rest.delete(f));
    if (!hit.length) continue;
    console.log(`\n== ${cat.name}: ${hit.length}件`);
    console.log(`   → ${cat.action}`);
    if (!cat.hideFiles) for (const f of hit) console.log(`   - ${f}`);
  }
  if (rest.size) {
    console.log(`\n== その他: ${rest.size}件`);
    for (const f of rest) console.log(`   - ${f}`);
  }
  console.log(`\n詳細: git -C upstream diff ${base.slice(0, 7)} ${target.slice(0, 7)} -- <ファイル>`);
}

if (args.has('--check')) process.exit(0);

const node = [process.execPath, '--experimental-transform-types', '--no-warnings', 'tools/upstream-bridge/run.mjs'];
let ok = run('マスターデータ・初期設定の書き出し', node[0], [...node.slice(1), 'export-data.mjs']);
ok &&= run('正解データ(本家の計算結果)の生成', node[0], [...node.slice(1), 'golden.mjs']);
ok &&= run('突き合わせテスト', 'cargo', ['test', '-p', 'pokesle-core', '--release']);

console.log('\n== 結果');
if (ok) {
  console.log('すべて一致しました。cargo build --release で反映できます。');
  console.log('upstream/ のコミットを記録する場合: git add upstream crates/pokesle-core/data crates/pokesle-core/tests/golden');
} else {
  console.log('失敗した項目があります。上のログを確認してください。');
  console.log('- load_master が失敗: 未対応のスキル/サブスキル/性格効果が追加された → data.rs の SkillKind 等と計算処理を追加');
  console.log('- golden が不一致: 計算ロジックが変わった → 上の「計算ロジック」の diff を移植');
  process.exit(1);
}

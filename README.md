# ポケスリシミュレーター (Rust / Windows)

[reimer0204/pokesle-simulator](https://github.com/reimer0204/pokesle-simulator)(ブラウザ版のポケスリシミュレーター)の計算処理を Rust へ移植した、**非公式**の Windows デスクトップアプリです。

- 本家の作者、および株式会社ポケモン等の各権利者とは関係ありません。
- 計算仕様・マスターデータは本家に準拠しています。ブラウザ版と計算結果を一致させるため、細かな挙動も含めて本家の実装を忠実に再現しています。
- ブラウザ版を普段使いしている方は、本家をお使いください: https://reimer0204.github.io/pokesle-simulator/

## 構成

| パス | 内容 |
| --- | --- |
| `crates/pokesle-core` | 計算・マスターデータ(UI 非依存) |
| `crates/pokesle-ocr` | スクリーンショット取得・文字認識(OCR) |
| `crates/pokesle-app` | Windows アプリ(egui / eframe) |
| `upstream/` | 本家リポジトリ(git submodule、追従元) |
| `tools/upstream-bridge` | 本家の TypeScript を Node.js で直接読み込み、データ書き出しと正解データ生成を行うスクリプト |

## ビルド

```sh
git clone --recursive https://github.com/amiami5/pokesle.git
cargo run -p pokesle-app --release
```

日本語フォントは同梱せず、Windows 標準のフォント(游ゴシック等)を実行時に読み込みます。

Rust 1.89 以降(edition 2024)が必要です。`is_multiple_of` と let チェーンを使っているため、古い Rust ではビルドできません。`rustup update` で更新してください。

## ブラウザ版(本家)からのデータ移行

ブラウザ版で使っていた**設定**と**ボックス**は、コンソールでコピーして貼り付けるだけで取り込めます。
ボックスだけでなく、ブラウザ版の localStorage に保存されている設定(シミュレーション設定、厳選基準、料理設定、所持食材、
アメ・ゆめのかけらの所持数、表示設定など)も一緒に移ります。

1. 普段使っているブラウザで本家(https://reimer0204.github.io/pokesle-simulator/)を開きます。
   **データが保存されているのと同じブラウザ・同じ URL** で開いてください(別のブラウザや別の URL では空になります)。
2. `F12` で開発者ツールを開き、「コンソール」タブに次の1行を貼り付けて Enter を押します。
   結果が自動でクリップボードにコピーされます(`undefined` と表示されれば成功です)。

   ```js
   copy(JSON.stringify({ config: JSON.parse(localStorage.getItem('config')), pokemonBox: JSON.parse(localStorage.getItem('pokemonBox')) }))
   ```

3. このアプリの「設定」画面の「ブラウザ版からの移行」欄に貼り付けます(コマンドは画面の「コマンドをコピー」でもコピーできます)。
4. 読み取り結果(ボックスの匹数など)を確認し、置き換える項目にチェックを入れて「取り込む」を押します。
   **現在の設定・ボックスは置き換わります。** 先に `%APPDATA%\pokesle-rs` の `config.json` と `box.json` をコピーしておくと安心です。

補足:

- 設定だけ、ボックスだけを貼り付けても取り込めます(`localStorage.getItem('config')` の値そのままでも可)。
- 厳選テーブル(基準生成の結果)はブラウザの IndexedDB に保存されていて移行できません。取り込み後に基準生成で作り直してください。
- 古い形式の設定(とくいごとのスキルLv設定、チェックリストの旧対象など)は、本家と同じ変換をして取り込みます。
- ボックスだけならTSVでも移せます(本家のボックス画面の書き出し → このアプリのボックス画面から TSV 取り込み)。
- 保存先は `%APPDATA%\pokesle-rs` です(`config.json` `box.json` `evaluate-table.json`)。

## 本家への追従

### 仕組み

ビルド時に使うマスターデータ(`crates/pokesle-core/data/master.json`)と初期設定(`default-config.json`)は、
本家の `src/data/` などの TypeScript を **Node.js で直接読み込んで書き出したもの**です(`tools/upstream-bridge/export-data.mjs`)。
進化段階・食材の個数・料理倍率などの派生値も本家の計算結果をそのまま書き出すため、Rust 側では再計算しません。

計算処理は Rust へ移植していますが、本家の Worker(`pokemon-box-worker` / `evaluate-simulator` / `team-simulator` など)を
Node.js 上でそのまま実行して正解データを作り(`golden.mjs`)、Rust の結果と突き合わせるテスト(`cargo test`)で一致を確認しています。

※ `upstream/` を更新しただけでは反映されません。書き出しを行ってからビルドしてください(下の更新スクリプトがまとめて行います)。

### 更新手順

Node.js 24 以降が必要です。リポジトリのルートで実行します。

```sh
# 前回取り込み以降の本家の変更を確認するだけ(何も書き換えない)
node tools/upstream-bridge/update.mjs --check

# 本家の最新(master)を取得 → 変更の分類表示 → データ書き出し → 正解データ生成 → テスト
node tools/upstream-bridge/update.mjs

# テストが通ったら反映
cargo build --release
```

| オプション | 内容 |
| --- | --- |
| `--check` | 本家を取得し、変更ファイルを分類して表示するだけ |
| `--no-pull` | 本家を更新せず、今の `upstream/` で書き出し・テストだけ行う |
| `--from <commit>` | 変更一覧の基準コミットを指定する(既定は前回取り込んだコミット = `master.json` の `upstreamCommit`) |

変更ファイルは「計算ロジック(移植が必要)」「計算仕様書」「マスターデータ・初期設定(自動反映)」「画面(参考)」に分類して表示します。

### 自動で反映されるもの・されないもの

| 本家の変更 | 書き出し＋ビルドで反映 |
| --- | --- |
| 既存のスキル・きのみ・食材を使う新ポケモンの追加 | される |
| 数値の調整(おてつだい時間、確率、所持数、スキル効果量、料理エナジー、レシピレベル表、経験値表など) | される |
| 新しい食材・料理・フィールドの追加 | ほぼされる(EX フィールドの新しい効果の種類はコードが必要) |
| 更新履歴の追加(`history.vue`) | される(`history.json` に書き出し。画面の「更新履歴」に反映。Rust 版の初版より前の分と、この版に無い機能の項目は `history.rs` で除外。こちらの更新情報は `data/history-local.json` に足す) |
| 初期設定の値の変更・項目の追加 | 値はされる(新しい項目を計算で使う場合はコードが必要) |
| 厳選テーブルのバージョン更新 | される(古いテーブルは再作成が必要と表示) |
| 新しいメインスキル・サブスキル・性格補正 | **されない**(`data.rs` の `SkillKind` 等と計算処理の追加が必要) |
| 計算ロジックの変更(`src/models/simulation/` など) | **されない**(差分を見て `crates/pokesle-core` へ移植) |
| 特定のポケモン名を見る処理の変更(エレズン、ラティアス/ラティオスなど) | **されない** |

新しい食材のアイコンは自動では増えません(アイコンが無い食材は名前の先頭2文字を表示します)。
`tools/food-icons/make.py` の `MAPPING` に本家の画像名と Noto Emoji のコードポイントを1行足し、
`python tools/food-icons/make.py` を実行してから `crates/pokesle-app/src/views/food_icon.rs` の `FOOD_ICONS` に追加してください。

### テストが失敗したとき

- `data::tests::load_master` が失敗: 未対応のスキル・サブスキル・性格補正が追加されています。メッセージに名前が出るので、
  `crates/pokesle-core/src/data.rs` の enum と、`simulator.rs` 等の計算処理を追加します。
- `golden` のテストが不一致: 計算ロジックが変わっています。`--check` で「計算ロジック」に分類されたファイルの差分
  (`git -C upstream diff <前回> <今回> -- <ファイル>`)を見て移植します。
- 計算結果をブラウザ版と一致させるため、細かな実装上の挙動や丸め処理も含めて本家に揃えています。
  移植時は独自の最適化や変更で結果が変わらないよう、本家の挙動に合わせてください。
- 正解データはテスト用のボックスに含まれるポケモン・スキルだけを検証します。新しいスキルを移植した場合は、
  `golden.mjs` のテスト用ボックスにそのポケモンを加えてください。

### 取り込んだ状態を記録する

```sh
git add upstream crates/pokesle-core/data crates/pokesle-core/tests/golden
git commit -m "本家 <コミット> に追従"
```

## ライセンス

[MIT License](LICENSE)。本家 (c) 2024 reimer0204 の著作権表示を含みます。

- 依存ライブラリのライセンスは [THIRD_PARTY_LICENSES.md](THIRD_PARTY_LICENSES.md)(`cargo about generate about.hbs > THIRD_PARTY_LICENSES.md` で生成)を参照してください。
- 本アプリはゲームの画像を同梱しません。アプリのアイコンは自作です(`tools/app-icon/make.py`)。食材アイコンは [Noto Emoji](https://github.com/googlefonts/noto-emoji)(OFL-1.1、Copyright 2013 Google LLC)と自作の絵です。詳しくは [crates/pokesle-app/assets/food/README.md](crates/pokesle-app/assets/food/README.md) を参照してください。
- 「ポケモン」「ポケモンスリープ」等の名称に関する権利は各権利者に帰属します。

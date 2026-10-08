// 本家モジュールを読み込める状態で、指定スクリプトを実行する。
// 使い方: node --experimental-transform-types tools/upstream-bridge/run.mjs <script.mjs> [args...]
import { register } from 'node:module';
import { fileURLToPath, pathToFileURL } from 'node:url';
import path from 'node:path';

register('./loader-hooks.mjs', import.meta.url);
const here = path.dirname(fileURLToPath(import.meta.url));
await import(pathToFileURL(path.resolve(here, process.argv[2])).href);

// 本家(upstream/)の Vite 向け TS/JS を Node で直接読み込むためのローダーフック。
// - 拡張子なし import の補完、`@/` エイリアス
// - 画像・Worker(?worker)・.vue はスタブ化
import { existsSync, statSync } from 'node:fs';
import { fileURLToPath, pathToFileURL } from 'node:url';
import path from 'node:path';

const here = path.dirname(fileURLToPath(import.meta.url));
const upstreamSrc = path.resolve(here, '../../upstream/src');
const STUB_PREFIX = 'pokesle-stub:';

function tryFile(p) {
  for (const cand of [p, p + '.ts', p + '.js', path.join(p, 'index.ts'), path.join(p, 'index.js')]) {
    if (existsSync(cand) && statSync(cand).isFile()) return cand;
  }
  return null;
}

export async function resolve(specifier, context, nextResolve) {
  if (specifier.startsWith(STUB_PREFIX)) return { url: specifier, shortCircuit: true };
  const [bare, query] = specifier.split('?');
  if (query === 'worker' || /\.(png|jpe?g|svg|webp|gif|vue)$/.test(bare)) {
    return { url: STUB_PREFIX + encodeURIComponent(bare), shortCircuit: true };
  }
  let target = null;
  if (bare.startsWith('@/')) target = path.join(upstreamSrc, bare.slice(2));
  else if ((bare.startsWith('./') || bare.startsWith('../')) && context.parentURL?.startsWith('file:')) {
    target = path.resolve(path.dirname(fileURLToPath(context.parentURL)), bare);
  }
  if (target) {
    const file = tryFile(target);
    if (file) {
      return {
        url: pathToFileURL(file).href,
        shortCircuit: true,
        importAttributes: file.endsWith('.json') ? { type: 'json' } : context.importAttributes,
      };
    }
  }
  return nextResolve(specifier, context);
}

export async function load(url, context, nextLoad) {
  if (url.startsWith(STUB_PREFIX)) {
    const name = decodeURIComponent(url.slice(STUB_PREFIX.length));
    return {
      format: 'module',
      shortCircuit: true,
      source: `export default ${JSON.stringify(name)};`,
    };
  }
  if (url.endsWith('.json')) {
    return nextLoad(url, { ...context, importAttributes: { type: 'json' } });
  }
  return nextLoad(url, context);
}

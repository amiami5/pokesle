// 本家の更新履歴(src/pages/history.vue のテンプレート)を JSON にする。
// 構造: { senden: [{text, href?}], entries: [{ date, items: [Item] }] }
//   Item = { segments: [{ text, href?, route? }], children: [Item] }
// 本家の history.vue は <HistoryItem date="..."> と <h3>日付</h3><ul> の2通りで書かれている。
import { readFileSync } from 'node:fs';

/** 簡易 HTML パース(本家の history.vue で使われるタグだけを想定) */
function parse(html) {
  const root = { tag: '#root', attrs: {}, children: [] };
  const stack = [root];
  const re = /<!--[\s\S]*?-->|<\/([a-zA-Z][\w-]*)\s*>|<([a-zA-Z][\w-]*)((?:\s+[^<>]*?)?)\s*(\/?)>|([^<]+)/g;
  const voids = new Set(['br']);
  let m;
  while ((m = re.exec(html))) {
    if (m[0].startsWith('<!--')) continue;
    if (m[1]) {
      // 閉じタグ: 対応する開始までさかのぼる
      const i = stack.map((n) => n.tag).lastIndexOf(m[1]);
      if (i > 0) stack.length = i;
    } else if (m[2]) {
      const attrs = {};
      for (const a of m[3].matchAll(/([:@\w-]+)\s*=\s*"([^"]*)"/g)) attrs[a[1]] = a[2];
      const node = { tag: m[2], attrs, children: [] };
      stack.at(-1).children.push(node);
      if (!m[4] && !voids.has(m[2])) stack.push(node);
    } else if (m[5] !== undefined) {
      stack.at(-1).children.push({ tag: '#text', text: m[5] });
    }
  }
  return root;
}

const decode = (s) => s.replace(/&nbsp;/g, ' ').replace(/&lt;/g, '<').replace(/&gt;/g, '>').replace(/&amp;/g, '&').replace(/&quot;/g, '"');

/** インライン要素(テキスト・リンク・br)をセグメントにする。連続する空白は1つにまとめる */
function segments(nodes) {
  const out = [];
  const push = (text, extra = {}) => {
    if (text === '') return;
    out.push({ text, ...extra });
  };
  const walk = (n, extra = {}) => {
    if (n.tag === '#text') push(decode(n.text).replace(/[ \t\r\n]+/g, ' '), extra);
    else if (n.tag === 'br') push('\n', extra);
    else if (n.tag === 'a') n.children.forEach((c) => walk(c, { href: n.attrs.href }));
    else if (n.tag === 'router-link') n.children.forEach((c) => walk(c, { route: n.attrs.to }));
    else if (n.tag !== 'ul') n.children.forEach((c) => walk(c, extra));
  };
  nodes.forEach((n) => walk(n));
  // 先頭・末尾・改行まわりの空白を落とす(空になったセグメントを除いて2回行い、末尾の空白を確実に落とす)
  let list = out;
  for (let pass = 0; pass < 2; pass++) {
    for (let i = 0; i < list.length; i++) {
      if (i == 0 || list[i - 1].text.endsWith('\n') || list[i - 1].text.endsWith(' ')) list[i].text = list[i].text.replace(/^ +/, '');
      if (i == list.length - 1 || list[i + 1]?.text.startsWith('\n')) list[i].text = list[i].text.replace(/ +$/, '');
    }
    list = list.filter((s) => s.text !== '');
  }
  return list;
}

function items(ul) {
  return ul.children
    .filter((n) => n.tag === 'li')
    .map((li) => ({
      segments: segments(li.children),
      children: li.children.filter((n) => n.tag === 'ul').flatMap(items),
    }));
}

export function parseHistory(vuePath) {
  const src = readFileSync(vuePath, 'utf-8');
  const template = src.slice(src.indexOf('<template>') + '<template>'.length, src.lastIndexOf('</template>'));
  const page = parse(template).children.find((n) => n.tag === 'div');
  const result = { senden: [], entries: [] };
  let pending = null;
  for (const n of page.children) {
    if (n.tag === 'div' && n.attrs.class === 'senden') {
      // 本文(リンク集の div を含む)
      result.senden = segments(n.children.flatMap((c) => (c.tag === 'div' ? c.children : [c])));
    } else if (n.tag === 'HistoryItem') {
      const ul = n.children.find((c) => c.tag === 'ul');
      result.entries.push({ date: n.attrs.date, items: ul ? items(ul) : [] });
    } else if (n.tag === 'h3') {
      pending = segments(n.children).map((s) => s.text).join('').trim();
    } else if (n.tag === 'ul' && pending != null) {
      result.entries.push({ date: pending, items: items(n) });
      pending = null;
    }
  }
  return result;
}

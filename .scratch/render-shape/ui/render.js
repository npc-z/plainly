// Render the real detail card for a screenshot, the way
// `.scratch/gloss-examples/ui/README.md` describes: take the actual
// `esc` / `bodyView` / `detailView` out of `app.js`, feed them one wire-shaped
// `DetailView`, lay the result inside the window shell with the actual
// `style.css`, and let Firefox draw it. Not a test — evidence for the manual
// checklist in `docs/desktop-manual-checks.md`.
//
//   nix develop -c node .scratch/render-shape/ui/render.js
//   nix develop -c firefox --headless --window-size=1253,865 \
//     --screenshot=.scratch/render-shape/ui/desktop-detail.png \
//     file://$PWD/.scratch/render-shape/ui/detail.html

const fs = require('fs');
const path = require('path');

const repo = path.resolve(__dirname, '..', '..', '..');
const appJs = path.join(repo, 'crates/plainly-desktop/ui/app.js');
const styleCss = path.join(repo, 'crates/plainly-desktop/ui/style.css');
const src = fs.readFileSync(appJs, 'utf8');

function extract(name) {
  const start = src.indexOf(`function ${name}(`);
  if (start < 0) throw new Error(`missing function ${name}`);
  let depth = 0;
  for (let i = src.indexOf('{', start); i < src.length; i++) {
    if (src[i] === '{') depth++;
    else if (src[i] === '}') {
      depth--;
      if (depth === 0) return src.slice(start, i + 1);
    }
  }
  throw new Error(`unbalanced ${name}`);
}

const escMatch = src.match(/const esc = \(value\) =>[\s\S]*?&quot;"\);/);
if (!escMatch) throw new Error('missing esc');
const esc = new Function(`${escMatch[0]}\nreturn esc;`)();

const factory = new Function(
  'esc',
  'app',
  'when',
  `${extract('bodyView')}\n${extract('detailView')}\nreturn detailView;`,
);

const app = {
  strings: {
    local_badge: '本机',
    provenance: '出处',
    copy_markdown: '复制 markdown',
    export_this: '导出这一条',
    delete_record: '删除',
    add_tag: '加标签',
    tag_placeholder: '标签，回车确认',
    prompt_version_note:
      '同一段落可能有多个提示词版本的记录——这是可以比对的特性，所以这里显示版本号。',
  },
  settingsStrings: {
    thinking_label: '思考',
    thinking_on_short: '开',
    thinking_off_short: '关',
    provenance_format: '记录格式',
    provenance_created: '首次记录',
    provenance_generated: '生成',
    provenance_seen: '最后查看',
    remove_tag: '移除',
  },
};

const when = () => '10月 9日 08:53';

// One wire-shaped record. The Key Help deliberately holds one Gloss with an
// Example and one with `example: null`, and the Grammar section is a `note`, so
// the screenshot shows every material the template can choose.
const record = {
  id: 1,
  level: 'B2',
  provider: 'deepseek',
  model: 'deepseek-flash',
  thinking: 'off',
  prompt_label: 'v8-examples',
  prompt_version:
    '9f2c1d4b6a8e0f3c5d7b9a1e2f4c6d8b0a2e4f6c8d0b2a4e6f8c0d2b4a6e8f0c',
  prompt_short: '9f2c1d4b',
  last_seen_unix: 1760000000,
  local: false,
  artifact_version: 2,
  created_at: '2025-10-09T08:53:20Z',
  generated_at: '2025-10-09T08:53:20Z',
  last_seen: '2025-10-09T08:53:20Z',
  tags: [],
  markdown: '',
  sections: [
    {
      kind: 'original',
      title: '原文',
      body: { type: 'prose', text: 'The report stops short of apportioning blame.' },
    },
    {
      kind: 'comprehensible-english',
      title: '改写',
      body: {
        type: 'prose',
        text: 'The report does not go as far as saying who was to blame.',
      },
    },
    {
      kind: 'key-help',
      title: '关键帮助',
      body: {
        type: 'glosses',
        glosses: [
          {
            expression: 'stop short of',
            gloss: 'not go as far as',
            example: 'The report stops short of blaming anyone.',
          },
          {
            expression: 'apportion blame',
            gloss: 'say who is responsible',
            example: null,
          },
        ],
      },
    },
    { kind: 'grammar', title: '语法', body: { type: 'note', text: '本段不需要' } },
    {
      kind: 'translation',
      title: '翻译',
      body: { type: 'prose', text: '报告没有说明谁该负责。' },
    },
  ],
};

const html = `<!doctype html><html lang="zh-CN"><head><meta charset="utf-8">
<link rel="stylesheet" href="file://${styleCss}">
</head><body><div class="win"><header class="tbar"><span class="name">Plainly</span><nav><a class="on">历史</a><a>设置</a></nav></header>
<div class="body"><div class="list"></div><div class="detail">${factory(esc, app, when)(record)}</div></div></div></body></html>`;

const out = path.join(__dirname, 'detail.html');
fs.writeFileSync(out, html);
console.log(`written ${out}`);

/** Zero-dependency packaging step. Run with Node.js 18+ from any directory. */
import fs from 'node:fs';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'..');
const read=(p)=>fs.readFileSync(path.join(root,p),'utf8');
const data=JSON.parse(read('data/atlas.json'));
const content=read('src/content.js');
const script=[read('src/icons.js'),content,read('src/demo-engine.js'),read('src/app.js')].join('\n;\n');
const escapeScript=s=>s.replace(/<\/script/gi,'<\\/script');
const css=read('src/styles.css');
const html=`<!doctype html>
<html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1"><meta name="color-scheme" content="light"><meta name="theme-color" content="#142f29"><meta name="description" content="A self-contained, interactive atlas of the household financial planning product requirements."><title>Financial Plan Atlas</title><link rel="icon" href="data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 32 32'%3E%3Crect width='32' height='32' rx='8' fill='%23142f29'/%3E%3Cpath d='m16 5 10 6v10l-10 6-10-6V11Zm0 11v11M6 11l10 5 10-5' fill='none' stroke='%23c3dab3' stroke-width='1.3'/%3E%3C/svg%3E"><style>${css}</style></head><body><noscript><main style="max-width:650px;margin:60px auto;font:16px/1.7 system-ui;padding:24px"><h1>Financial Plan Atlas</h1><p>This offline overview needs JavaScript enabled for its diagrams and search. The complete Markdown source is included at <a href="docs/requirements-v3.2.md">docs/requirements-v3.2.md</a>.</p></main></noscript><script>window.ATLAS_DATA=${JSON.stringify(data).replace(/</g,'\\u003c')};</script><script>${escapeScript(script)}</script></body></html>`;
fs.writeFileSync(path.join(root,'index.html'),html);
console.log(`Built index.html (${(Buffer.byteLength(html)/1024/1024).toFixed(2)} MB). No runtime dependencies or network requests.`);

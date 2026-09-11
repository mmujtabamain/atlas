const {test}=require('node:test');
const assert=require('node:assert/strict');
const fs=require('node:fs');
const path=require('node:path');
const crypto=require('node:crypto');
const root=path.join(__dirname,'..');
const data=JSON.parse(fs.readFileSync(path.join(root,'data/atlas.json'),'utf8'));
function complete(items,prefix,n,digits){assert.deepEqual(items.map(x=>x.id).sort(),Array.from({length:n},(_,i)=>prefix+String(i+1).padStart(digits,'0')).sort());}
test('All 162 feature IDs are present exactly once',()=>complete(data.features,'F',162,3));
test('All 55 models are present exactly once',()=>complete(data.models,'M',55,2));
test('All 80 product acceptance cases are preserved',()=>complete(data.tests,'V',80,3));
test('All 33 research and 10 product sources are preserved',()=>{
 complete(data.references.filter(r=>r.id[0]==='R'),'R',33,2);complete(data.references.filter(r=>r.id[0]==='P'),'P',10,2);
});
test('All 47 numbered chapters, reading guide and 12 deferred items are present',()=>{
 assert.equal(data.sections.length,48);assert.equal(data.deferred.length,12);assert.equal(data.decisions.length,15);
 for(let i=0;i<=47;i++)assert.ok(data.sections.find(s=>s.number===i));
});
test('Included Markdown is byte-for-byte the source used to build the atlas',()=>{
 const source=fs.readFileSync(path.join(root,'docs/requirements-v3.2.md'),'utf8');
 assert.equal(source,data.source);assert.equal(crypto.createHash('sha256').update(source).digest('hex'),data.meta.sourceSha256);
});
test('Feature ↔ model and model → reference edges are valid and bidirectional',()=>{
 const ms=Object.fromEntries(data.models.map(m=>[m.id,m]));const rs=new Set(data.references.map(r=>r.id));
 for(const f of data.features)for(const id of f.models){assert.ok(ms[id]);assert.ok(ms[id].features.includes(f.id));}
 for(const m of data.models){m.references.forEach(r=>assert.ok(rs.has(r)));m.features.forEach(id=>assert.ok(data.features.find(f=>f.id===id).models.includes(m.id)));}
});
test('Equations are compiled into MathML with no parse errors',()=>{
 const validation=JSON.parse(fs.readFileSync(path.join(root,'data/content-validation.json'),'utf8'));
 assert.equal(validation.mathParseErrors.length,0);assert.equal(validation.uniqueEquations,148);
 const text=JSON.stringify(data);assert.ok(text.includes('<math'));assert.ok(!text.includes('ATLASMATHTOKEN'));assert.ok(!text.includes('<merror'));
});
test('All original reference records retain external links',()=>{
 for(const r of data.references){assert.ok(r.links.length>0,r.id);for(const l of r.links)assert.match(l.url,/^https:\/\//);}
});
test('Built page has no external scripts, stylesheets, frames or runtime requests',()=>{
 const built=fs.readFileSync(path.join(root,'index.html'),'utf8');
 assert.ok(!/<script[^>]+\bsrc=/i.test(built));assert.ok(!/<link[^>]+rel=["']stylesheet/i.test(built));assert.ok(!/<iframe/i.test(built));
 const app=fs.readFileSync(path.join(root,'src/app.js'),'utf8');assert.ok(!/\bfetch\s*\(|XMLHttpRequest|WebSocket/.test(app));
});

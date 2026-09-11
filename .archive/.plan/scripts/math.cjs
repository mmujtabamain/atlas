// Optional content regeneration only; the shipped page has no runtime dependencies.
const fs = require('node:fs');
const cp = require('node:child_process');
const path = require('node:path');
let root;
try { root = path.dirname(require.resolve('mathjax-full/package.json')); }
catch { root = path.join(cp.execSync('npm root -g', {encoding:'utf8'}).trim(), 'mathjax-full'); }
const {mathjax}=require(root+'/js/mathjax.js');
const {TeX}=require(root+'/js/input/tex.js');
const {SVG}=require(root+'/js/output/svg.js');
const {liteAdaptor}=require(root+'/js/adaptors/liteAdaptor.js');
const {RegisterHTMLHandler}=require(root+'/js/handlers/html.js');
const {AllPackages}=require(root+'/js/input/tex/AllPackages.js');
const {SerializedMmlVisitor}=require(root+'/js/core/MmlTree/SerializedMmlVisitor.js');
const {STATE}=require(root+'/js/core/MathItem.js');
const adaptor=liteAdaptor(); RegisterHTMLHandler(adaptor);
const document=mathjax.document('',{InputJax:new TeX({packages:AllPackages}),OutputJax:new SVG({fontCache:'none'})});
const visitor=new SerializedMmlVisitor();
const input=JSON.parse(fs.readFileSync(0,'utf8'));
const results=input.map(({tex,display})=>{
  const root=document.convert(tex,{display,end:STATE.CONVERT});
  const html=visitor.visitTree(root).replace(/\n\s*/g,'');
  return {html,error:html.includes('<merror')};
});
process.stdout.write(JSON.stringify(results));

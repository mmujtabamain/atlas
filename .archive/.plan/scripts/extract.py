"""Regenerate the complete atlas dataset from the included Markdown specification.
Optional dependencies: mistune 3.2.1 and MathJax 3.2.1 (Node package).
Normal viewing and rebuilding use the already generated data/atlas.json.
"""
from pathlib import Path
import re, json, html, subprocess, hashlib
import mistune

ROOT=Path(__file__).resolve().parents[1]
source=(ROOT/'docs/requirements-v3.2.md').read_text(encoding='utf8')
math_entries=[]
math_keys={}
def math_token(tex,display):
    key=(tex,display)
    if key not in math_keys:
        math_keys[key]=len(math_entries)
        math_entries.append({'tex':tex,'display':display})
    i=math_keys[key]
    tag='div' if display else 'span'
    return f'<{tag} class="math-{ "block" if display else "inline"}" data-equation="{i}">ATLASMATHTOKEN{i}END</{tag}>'
class Renderer(mistune.HTMLRenderer):
    def block_math(self,text): return math_token(text,True)+'\n'
    def inline_math(self,text): return math_token(text,False)
    def link(self,text,url,title=None):
        if not (url.startswith(('http://','https://','#','mailto:'))):
            return text
        extra=' target="_blank" rel="noopener noreferrer"' if url.startswith(('http://','https://')) else ''
        title_attr=f' title="{html.escape(title,quote=True)}"' if title else ''
        return f'<a href="{html.escape(url,quote=True)}"{extra}{title_attr}>{text}</a>'
    def block_html(self,text):
        if re.fullmatch(r'\s*<a id="[a-zA-Z0-9-]+"></a>\s*',text):return text
        return html.escape(text)
    def inline_html(self,text):
        if re.fullmatch(r'<a id="[a-zA-Z0-9-]+">|</a>',text):return text
        return html.escape(text)
md=mistune.create_markdown(renderer=Renderer(),plugins=['table','strikethrough','math'])
def clean(s):
    s=re.sub(r'\[([^\]]+)\]\([^)]+\)',r'\1',s)
    s=re.sub(r'<[^>]+>','',s)
    return re.sub(r'\s+',' ',re.sub(r'[*#`|]',' ',s)).strip()
def refs(s,prefix='R',digits=2):
    found=set()
    for a,b in re.findall(rf'{prefix}(\d{{{digits}}})\s*[–-]\s*(?:{prefix})?(\d{{{digits}}})',s):
        found.update(f'{prefix}{x:0{digits}d}' for x in range(int(a),int(b)+1))
    found.update(re.findall(rf'\b{prefix}\d{{{digits}}}\b',s))
    return sorted(found)
sections=[]
heads=list(re.finditer(r'^## (\d+)\. (.+)$',source,re.M))
intro=source[:heads[0].start()]
sections.append({'id':'s0','number':0,'title':'Reading guide & document scope','markdown':intro,'html':md(intro),'plain':clean(intro)})
for i,m in enumerate(heads):
    body=source[m.end():heads[i+1].start() if i+1<len(heads) else len(source)].strip()
    sections.append({'id':f's{m[1]}','number':int(m[1]),'title':m[2],'markdown':body,'html':md(body),'plain':clean(body)})
bysection={s['number']:s for s in sections}
models=[]
for sec in sections:
    if 33<=sec['number']<=41:
        matches=list(re.finditer(r'^### (M\d{2}) — (.+)$',sec['markdown'],re.M))
        for i,m in enumerate(matches):
            body=sec['markdown'][m.end():matches[i+1].start() if i+1<len(matches) else len(sec['markdown'])].strip()
            proposed=re.search(r'\*\*Proposed features\.?\*\*\s*([^\n]+)',body)
            summary=clean(proposed[1]) if proposed else clean(body.split('\n\n')[0])
            models.append({'id':m[1],'title':m[2],'group':sec['title'],'section':sec['id'],'summary':summary,'markdown':body,'html':md(body),'references':refs(body),'features':[]})
featuregroups=[];features=[]
for chunk in re.split(r'^### ',bysection[43]['markdown'],flags=re.M)[1:]:
    title=chunk.split('\n')[0].strip()
    group={'id':f'g{len(featuregroups)+1}','title':title,'features':[]}
    for line in chunk.splitlines():
        m=re.match(r'^\| (F\d{3}) \| (.*?) \| (.*?) \|',line)
        if m:
            f={'id':m[1],'title':m[2],'models':refs(m[3],'M'),'group':group['id']}
            features.append(f);group['features'].append(f['id'])
    featuregroups.append(group)
for model in models:
    model['features']=[f['id'] for f in features if model['id'] in f['models']]
verifications=[];testgroups=[]
for chunk in re.split(r'^### ',bysection[45]['markdown'],flags=re.M)[1:]:
    title=chunk.split('\n')[0].strip()
    rows=re.findall(r'^\| (V\d{3}) \| (.*?) \|',chunk,re.M)
    if rows:
        group={'id':f't{len(testgroups)+1}','title':re.sub(r'^45\.\d+ ','',title),'tests':[]}
        for ident,text in rows:
            verifications.append({'id':ident,'title':text,'group':group['id']});group['tests'].append(ident)
        testgroups.append(group)
references=[]
ref_matches=list(re.finditer(r'^\*\*((?:R|P)\d{2}) — (.+?)\*\*(.*)$',bysection[47]['markdown'],re.M))
for i,m in enumerate(ref_matches):
    body=bysection[47]['markdown'][m.start():ref_matches[i+1].start() if i+1<len(ref_matches) else len(bysection[47]['markdown'])]
    body=re.sub(r'<a id="[^"]+"></a>','',body)
    body=re.split(r'^### ',body,flags=re.M)[0].strip()
    body=body.split('\n---')[0].strip()
    links=[{'label':clean(label),'url':url} for label,url in re.findall(r'\[([^\]]+)\]\((https?://[^)]+)\)',body)]
    references.append({'id':m[1],'title':clean(m[2]),'kind':'Research & official sources' if m[1].startswith('R') else 'Product references','html':md(body),'plain':clean(body),'links':links,'models':[x['id'] for x in models if m[1] in x['references']]})
refs_by_id={r['id']:r for r in references}
unknown={r for m in models for r in m['references'] if r not in refs_by_id}
assert not unknown, unknown
deferred=[]
part=bysection[22]['markdown'].split('### 22.1 Explicitly deferred')[1].split('### 22.2')[0]
for title,body in re.findall(r'^- \*\*(.+?)\*\*\s*(.*)$',part,re.M):
    deferred.append({'title':title.rstrip('.'),'text':body})
notdeferred=[clean(x) for x in re.findall(r'^- (.+)$',bysection[22]['markdown'].split('### 22.2')[1],re.M)]
decisions=[]
for line in bysection[46]['markdown'].splitlines():
    row=re.match(r'^\| (.*?) \| (.*?) \| (.*?) \|',line)
    if row and row[1] not in ['Decision','---']:decisions.append({'title':row[1],'reason':row[2],'resolution':row[3]})
labels=[]
part=bysection[32]['markdown'].split('### 32.2')[1].split('### 32.3')[0]
for line in part.splitlines():
    m=re.match(r'^\| \*\*(.*?)\*\* \| (.*?) \| (.*?) \|',line)
    if m:labels.append({'title':m[1],'claim':m[2],'limit':m[3]})
anchors={}
for sec in sections:
    for anchor in re.findall(r'<a id="([^"]+)"></a>',sec['markdown']):anchors[anchor]=sec['id']
# These anchors precede their following chapter, not the previous chapter body.
anchors.update({'research-method':'s31','mathematical-contract':'s32','model-catalogue':'s33','feature-register':'s43','worked-examples':'s44','verification':'s45','open-decisions':'s46','references':'s47'})
for r in references:anchors[r['id'].lower()]=r['id']
for model in models:anchors[model['id'].lower()]=model['id']
examples=[]
for chunk in re.split(r'^### ',bysection[44]['markdown'],flags=re.M)[1:]:
    first,body=chunk.split('\n',1)
    ident,title=first.split(' — ',1)
    examples.append({'id':ident,'title':title,'html':md(body),'markdown':body})
data={'meta':{'version':'3.2','name':'Household Financial Planning & Decision Engine','researchDate':'2026-09-10','sourceSha256':hashlib.sha256(source.encode()).hexdigest()},'sections':sections,'models':models,'featureGroups':featuregroups,'features':features,'testGroups':testgroups,'tests':verifications,'references':references,'deferred':deferred,'notDeferred':notdeferred,'decisions':decisions,'labels':labels,'anchors':anchors,'examples':examples,'source':source}
assert len(features)==162 and len(models)==55 and len(verifications)==80 and len(references)==43
print(f'Rendering {len(math_entries)} unique equations…',flush=True)
proc=subprocess.run(['node',str(ROOT/'scripts/math.cjs')],input=json.dumps(math_entries),capture_output=True,text=True,check=True)
rendered=json.loads(proc.stdout)
errors=[math_entries[i] for i,x in enumerate(rendered) if x['error']]
(ROOT/'data/math-cache.json').write_text(json.dumps({'input':math_entries,'output':rendered},ensure_ascii=False),encoding='utf8')
def substitute(obj):
    if isinstance(obj,str):return re.sub(r'ATLASMATHTOKEN(\d+)END',lambda m:rendered[int(m[1])]['html'],obj)
    if isinstance(obj,list):return [substitute(x) for x in obj]
    if isinstance(obj,dict):return {k:substitute(v) for k,v in obj.items()}
    return obj
data=substitute(data)
(ROOT/'data/atlas.json').write_text(json.dumps(data,ensure_ascii=False,separators=(',',':')),encoding='utf8')
report={'features':len(features),'models':len(models),'acceptanceCases':len(verifications),'references':len(references),'sections':len(sections),'deferredDecisions':len(deferred),'uniqueEquations':len(math_entries),'mathParseErrors':errors,'sourceSha256':data['meta']['sourceSha256']}
(ROOT/'data/content-validation.json').write_text(json.dumps(report,indent=2),encoding='utf8')
print(json.dumps(report,indent=2))
if errors:raise SystemExit('Equation parse errors; review before delivery.')

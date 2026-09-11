"""Browser smoke tests for the atlas, not the 80 financial-product acceptance cases.
Requires Playwright and a Chromium browser. Run from any directory.
Use --inject when a managed environment blocks navigation to local files.
"""
from pathlib import Path
from playwright.sync_api import sync_playwright
import argparse, json, shutil, os, time

ROOT=Path(__file__).resolve().parents[1]
p=argparse.ArgumentParser();p.add_argument('--inject',action='store_true');p.add_argument('--screenshots',action='store_true');args=p.parse_args()
OUT=ROOT/'preview';OUT.mkdir(exist_ok=True)
checks=[];errors=[];requests=[]
def record(name,condition=True):
    if not condition:raise AssertionError(name)
    checks.append(name)

def go(page,name):
    page.evaluate('(name)=>{location.hash=name}',name)
    page.wait_for_function('(name)=>window.AtlasApp.getView()===name',arg=name)
    page.evaluate('new Promise(resolve=>requestAnimationFrame(()=>requestAnimationFrame(resolve)))')

def overflow(page):
    return page.evaluate('document.documentElement.scrollWidth > innerWidth + 1')

with sync_playwright() as p:
    executable=os.environ.get('BROWSER_EXECUTABLE') or shutil.which('chromium') or p.chromium.executable_path
    browser=p.chromium.launch(headless=True,executable_path=executable,args=['--no-sandbox'])
    context=browser.new_context(viewport={'width':1440,'height':1050},device_scale_factor=1,accept_downloads=True,reduced_motion='reduce')
    page=context.new_page();page.on('pageerror',lambda e:errors.append(str(e)));page.on('request',lambda req:requests.append(req.url))
    html=(ROOT/'index.html').read_text()
    if args.inject:
        page.set_content(html,wait_until='load');mode='HTML bytes injected into a fresh browser document'
    else:
        page.goto((ROOT/'index.html').as_uri(),wait_until='load');mode='Direct file:// navigation'
    record('Overview renders with source inventory counts',page.locator('.stat-value').all_text_contents()==['162','55','80','43'])
    record('Overview fits desktop viewport without page overflow',not overflow(page))
    if args.screenshots:page.locator('#toast').wait_for(state='hidden');page.screenshot(animations='disabled',path=str(OUT/'overview-desktop.png'))
    for name in ['map','capabilities','math','scenarios','privacy','deferred','spec','sources']:
        go(page,name)
        record(f'{name}: nonempty page renders',len(page.locator('h1').inner_text())>5)
        record(f'{name}: no desktop horizontal overflow',not overflow(page))
    go(page,'capabilities')
    record('All 13 feature groups are visible',page.locator('.group-card').count()==13)
    page.fill('#feature-search','F041')
    record('Feature ID search returns exactly F041',page.locator('.requirement-row').count()==1)
    page.locator('.requirement-row').click();page.wait_for_selector('#detail-dialog[open]')
    record('Feature opens correct detail',page.locator('#detail-title').inner_text()=='Gross-up to a required net receipt after real taxes and fees')
    page.locator('#detail-body [data-id="M25"]').first.click()
    page.wait_for_function('document.querySelector("#detail-header .id-link")?.textContent==="M25"')
    record('Feature-to-model drilldown works',page.locator('#detail-body math').count()>0)
    page.locator('[data-action="bookmark"]').click()
    page.fill('#review-note','Verify gross-up rounding against each configured tax pack.')
    record('Review note can be entered',page.input_value('#review-note').startswith('Verify gross-up'))
    page.keyboard.press('Escape');page.wait_for_function('!document.querySelector("#detail-dialog").open')
    page.keyboard.press('/')
    page.wait_for_selector('#search-dialog[open]');page.fill('#global-search','M55')
    record('Global search prioritizes exact model ID',page.locator('.search-result').first.get_attribute('data-id')=='M55')
    page.locator('.search-result').first.click();page.wait_for_selector('#detail-dialog[open]')
    record('Global search opens model detail',page.locator('#detail-title').inner_text().startswith('Authorization-aware'))
    page.keyboard.press('Escape')
    go(page,'math')
    page.locator('[data-action="all-models"]').click()
    record('All models are reachable in the catalogue',page.locator('.model-card').count()==55)
    page.fill('#math-search','M46');page.locator('.model-card').first.click()
    page.wait_for_selector('#detail-dialog[open]')
    record('Actuarial formulas render as MathML',page.locator('#detail-body math').count()>0)
    if args.screenshots:page.locator('#toast').wait_for(state='hidden');page.screenshot(animations='disabled',path=str(OUT/'model-detail.png'))
    page.keyboard.press('Escape')
    page.locator('[data-action="math-mode"][data-mode="tests"]').click()
    record('Five acceptance-test groups are preserved',page.locator('.test-group').count()==5)
    page.locator('.test-group summary').first.click()
    record('Acceptance cases expand without being marked as passed',page.locator('.test-group[open] .requirement-row').count()==12)
    go(page,'map')
    page.locator('[data-action="graph-node"][data-node="companies"]').first.click()
    record('Entity map selection updates the inspector','Companies' in page.locator('.inspector h3').inner_text())
    record('Companies visibly connect to their own accounts','Owns separate company accounts' in page.locator('.relationships').inner_text())
    page.locator('[data-action="map-mode"][data-mode="flow"]').click()
    record('Calculation graph renders',page.locator('.node-box').count()==8)
    page.locator('[data-action="map-mode"][data-mode="trace"]').click()
    page.select_option('#trace-model','M26')
    record('Trace diagram shows the selected model','Multi-year tax-aware funding' in page.locator('.trace-node.center').inner_text())
    page.select_option('#trace-model','M55')
    record('Source-free models do not get invented citations','No external research ID' in page.locator('#map-view').inner_text())
    page.locator('[data-action="map-mode"][data-mode="entities"]').click()
    if args.screenshots:page.locator('#toast').wait_for(state='hidden');page.screenshot(animations='disabled',path=str(OUT/'relationships-desktop.png'))
    go(page,'scenarios')
    record('Default E03 result matches the source',page.locator('.result-stat .value').all_text_contents()==['1.08m','+80,000','2.00m'])
    page.select_option('#purchase-date','2026-11-15')
    record('Earlier purchase shows the original 20,000 shortfall',page.locator('.result-stat .value').all_text_contents()==['0.98m','20,000','2.00m'])
    page.locator('#down-payment').fill('1400000')
    record('Slider recalculates deterministically',page.locator('.result-stat .value').all_text_contents()==['0.88m','120,000','2.00m'])
    page.locator('[data-action="reset-demo"]').click()
    record('Reset restores fixture defaults',page.input_value('#down-payment')=='1300000')
    record('Original six comparison rows are included',page.locator('#comparison-table tbody tr').count()==6)
    page.locator('#demo-results details summary').click()
    record('Dated calculation trail is inspectable',page.locator('#demo-results tbody tr').count()==15)
    page.locator('#demo-results details summary').click()
    if args.screenshots:page.locator('#toast').wait_for(state='hidden');page.evaluate('window.scrollTo(0,0)');page.screenshot(animations='disabled',path=str(OUT/'decision-example-desktop.png'))
    go(page,'privacy')
    record('Authorized contribution reconciles to the displayed total','1,200,000' in page.locator('#privacy-output').inner_text())
    page.uncheck('#privacy-grant')
    record('Missing disclosure grant blocks the shared result','No shared result published' in page.locator('#privacy-output').inner_text())
    page.uncheck('#privacy-use')
    record('Exclusion recalculates visible-only sources','700,000' in page.locator('.ledger-line.total').inner_text())
    page.check('#privacy-use');page.check('#privacy-grant')
    go(page,'deferred')
    record('All 12 deferred areas are present',page.locator('.deferred-item').count()==12)
    record('All 15 configuration decisions are present',page.locator('.open-decision-row').count()==15)
    page.locator('.deferred-item summary').first.click()
    record('Deferred scope explanation expands',page.locator('.deferred-item[open]').count()==1)
    go(page,'spec')
    record('Complete specification is navigable',page.locator('.reader-index button').count()==48)
    page.locator('.reader-index [data-action="section"][data-section="0"]').click()
    page.locator('.prose a[href="#research-method"]').click()
    page.wait_for_function('location.hash.includes("section=31") && document.querySelector(".reader-title")?.textContent.includes("Research Method")')
    record('Reading-guide anchors point to the correct following section','Research Method' in page.locator('.reader-title').inner_text())
    go(page,'sources')
    record('All research and product references appear',page.locator('.reference-card').count()==43)
    page.select_option('#reference-kind','product')
    record('Product-reference filter is exact',page.locator('.reference-card').count()==10)
    record('Source links use HTTPS and safe external-link attributes',page.locator('.reference-card a').evaluate_all('(as)=>as.every(a=>a.href.startsWith("https://")&&a.target==="_blank"&&a.rel.includes("noopener"))'))
    # The injected mode has an opaque origin; actual local-file storage is browser-dependent.
    page.locator('[data-action="open-saved"]').click()
    record('Saved review items are available during navigation','M25' in page.locator('#search-results').inner_text())
    page.locator('[data-action="close-search"]').click()
    for width,height in [(390,844),(768,1024)]:
        page.set_viewport_size({'width':width,'height':height})
        for name in ['overview','map','capabilities','math','scenarios','privacy','deferred','spec','sources']:
            go(page,name)
            record(f'{name}: viewport {width}px contains the page',not overflow(page))
            if args.screenshots and width==390 and name in ['overview','scenarios','map']:
                page.locator('#toast').wait_for(state='hidden')
                page.screenshot(animations='disabled',path=str(OUT/f'{name}-mobile.png'),full_page=name=='overview')
    page.set_viewport_size({'width':390,'height':844});go(page,'overview')
    page.locator('.mobile-toggle').click()
    record('Mobile navigation opens',page.locator('.sidebar').evaluate('(el)=>el.classList.contains("open")'))
    page.locator('.nav-link[data-nav="privacy"]').click()
    page.wait_for_function('window.AtlasApp.getView()==="privacy" && !document.querySelector(".sidebar").classList.contains("open")')
    record('Mobile navigation closes after choosing a page',not page.locator('.sidebar').evaluate('(el)=>el.classList.contains("open")'))
    record('No uncaught browser JavaScript errors',not errors)
    remote=[r for r in requests if r.startswith(('https://','http://'))]
    record('No network requests are needed to render or interact',not remote)
    browser.close()
report={'passed':len(checks),'mode':mode,'checks':checks,'javascriptErrors':errors,'networkRequests':requests,'notes':['This validates the atlas and its E03 illustration, not the 80 future-product acceptance cases.','Injected mode exercises the exact built HTML, scripts and styles but does not certify file-protocol or localhost navigation in this managed environment.','External research links were structurally checked, not re-audited or followed.']}
(ROOT/'tests/browser-report.json').write_text(json.dumps(report,indent=2))
print(json.dumps({'passed':len(checks),'mode':mode,'javascriptErrors':errors,'networkRequests':len(requests)},indent=2))

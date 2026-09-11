# Atlas verification

This report concerns the interactive requirements atlas, not an implementation of the future financial product. In particular, the **80 V-series acceptance cases are preserved requirements, not 80 tests that the financial product has passed**.

## Source identity and coverage

The included `requirements-v3.2.md` is byte-for-byte identical to the supplied source. SHA-256:

```text
d63f3cb885c0707667711ea7ee96b1d552dcc755e1ababdcc0a662edfd092282
```

The parser and integrity tests confirm 162 feature IDs, 55 model IDs, 80 acceptance-case IDs, 33 research/official records, 10 product records, 47 numbered sections plus the reading guide, 12 deferred areas and 15 configuration decisions. All 148 unique TeX expressions converted to native MathML without a reported parse error. This verifies parsing and coverage, not the truth of every research statement or the correctness of every proposed financial model.

## Automated checks run

| Check | Result | Scope |
|---|---:|---|
| Node tests | 24 passed, 0 failed | Inventory completeness; source identity; valid feature/model/reference links; no unresolved math; offline packaging; E03 integer calculations, ordering and bounds |
| Browser checks | 74 passed | Nine main views; search and filtering; dialogs; diagrams; source navigation; mathematical markup; privacy illustration; notes/bookmarks; responsive containment |
| Uncaught browser JavaScript errors | 0 | During the included interaction checks |
| Runtime HTTP(S) requests | 0 | During rendering and the included interaction checks; external citations were not followed |

Detailed results are in `tests/node-report.txt`, `tests/browser-report.json` and `data/content-validation.json`. The repeatable tests themselves are included.

### E03 calculation verification

The fixture tests reproduce the six original down-payment/date comparisons, with matching minimum balances and dates. They also verify that current confirmed cash remains unchanged by expected future salaries, salaries end after November, earlier same-day events precede the modeled down payment, reserve thresholds do not subtract cash twice, replay is deterministic, and invalid or unsafe numerical inputs are rejected.

The illustration uses integer minor units within its supported input range. It does not implement taxation, financing, fees, investment returns or complete car ownership costs. Its pointwise bound is valid only for the simple additive, independent-bound cash-flow fixture stated in the example. It does not establish guarantees for the full future product.

## Browser verification method and limitation

The managed browser environment blocked navigation to both the local server URL and the local file URL with `ERR_BLOCKED_BY_ADMINISTRATOR`. The optional `agent-browser` command-line tool was unavailable. Playwright with installed Chromium was used instead.

The browser checks loaded the **exact built `index.html` bytes** into a fresh browser document using `page.set_content(...)`. All embedded JavaScript, CSS, SVG, MathML, navigation and tested interactions ran in that document. Desktop, 768-pixel tablet and 390-pixel mobile views were checked. Reduced-motion rendering was used to avoid screenshots of transient animation frames.

This method verifies the built document's rendering and interactions but does **not** certify a direct `file://` launch, browser navigation to the optional localhost server, or persistent storage in the user's browser. The test document has an opaque origin; the app's session-only storage fallback was exercised. Bookmark/note persistence across restarts was not verified. The README explains how to export review notes.

The single-file build contains no external scripts, styles, runtime fetches or font files. The source scripts are classic inlined JavaScript rather than modules that require a server. These are deliberate packaging choices for direct-file use, not a substitute for testing every browser configuration.

## Visual review

Desktop overview, entity relationships, a mathematical detail drawer, the decision example and narrow-screen views were rendered. The included `preview/` screenshots document those results. Main-page horizontal containment was checked across all nine views at tablet and mobile widths. Large relationship diagrams and source tables intentionally retain internal scrolling rather than becoming illegible.

Keyboard-oriented controls, native dialogs, text descriptions of diagram connections, accessible labels, print styles and reduced-motion styles are included. A complete accessibility conformance audit, screen-reader matrix and cross-browser rendering audit have not been performed.

## Outside this verification

- No bank, payment, payroll, tax-filing, financial-data or authentication integration was implemented or tested.
- No production tax pack, advanced optimization solver or real authorization policy engine was implemented.
- The privacy illustration is explanatory UI, not enforced security. Do not enter confidential financial information into this atlas.
- External research and product links were preserved and structurally checked; their destinations and current contents were not independently re-audited.
- The full mathematical catalogue remains proposed requirements, not 55 implemented algorithms.
- Browser restart persistence, every download/print configuration, every device and every browser policy were not certified.

## Repeating the checks

```sh
npm run build
npm test
```

Browser tests require separately installed Python Playwright and a Chromium browser. With a normal environment and the optional server running:

```sh
npm start
# In another terminal:
python tests/browser_smoke.py --screenshots
```

For the same injected-document method used for the recorded report:

```sh
python tests/browser_smoke.py --inject --screenshots
```

Inspect the browser script before adapting its Chromium executable path to another operating system. Browser-test dependencies are not needed to open or use the atlas.

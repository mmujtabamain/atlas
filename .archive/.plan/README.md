# Financial Plan Atlas

An interactive, offline overview of **Household Financial Planning & Decision Engine — requirements v3.2**.

## Start here

**Extract the ZIP, then open `index.html` in a modern browser.**

Everything the overview needs is embedded in that one file. No account, API key, installation, build step, database or internet connection is needed to use it. Following an external research reference requires internet access.

This is a **requirements atlas**, not a proposed final product interface, delivery roadmap or working financial platform. It does not connect to banks, compute production tax liabilities, enforce real financial privacy or execute transactions. The complete source requirements are included unchanged.

## A useful route through the atlas

Start at **Overview** for the conceptual picture. Use **System map** to select an object and inspect its connections. **Capabilities** leads from each feature to the models it depends on; **Models & tests** leads to the equations, assumptions, limitations and original research.

The **Decision example** turns the document’s E03 fixture into a small interactive cash-timing example. Changing the down payment or payment date updates the chart, reserve headroom and calculation trail. Its exclusions are prominently stated: it is not a complete car purchase, financing or tax simulator.

**Privacy & trust** illustrates the difference between ownership, permission to calculate and permission to disclose. **Open decisions** preserves all 12 deferred areas, the explicitly non-deferred foundations and the 15 configuration decisions. **Full specification** and **Research sources** retain the detailed original material.

## What is included

| Area | Content |
|---|---|
| Complete source coverage | 162 feature candidates; 55 mathematical models; 80 future-product acceptance cases; 47 numbered sections plus the reading guide |
| Relationship diagrams | Entity/legal boundaries; calculation flow; feature → model → research traceability |
| Search | Features, models, tests, full specification chapters, research references and worked examples |
| Equations | 148 unique TeX expressions preconverted into native MathML; no runtime math library, remote stylesheet or bundled font |
| Source references | 33 research/official records and 10 product references, including the original evidence-access notes |
| Review support | Bookmark items, write local review notes and export those annotations as JSON |
| Worked example | Exact E03 defaults and six original comparisons, plus an adjustable down payment within the same fixture |
| Source code | Readable JavaScript, CSS, editorial graph data, structured source data, extraction scripts, build scripts and tests |

Search with the top-right button, `/`, or `Ctrl+K` / `Cmd+K`. `Escape` closes the open detail or search dialog. The browser’s Back button works with page and detail links. Most graph nodes can be reached with the keyboard; the relationships are also written as text beneath the diagram.

## Review notes and local storage

Notes and bookmarks are annotations only; they never edit the specification. They are stored in your browser when it permits local storage. Some file-protocol or managed/private-browser environments disable persistent storage. In that case, the atlas explicitly says that notes last only for the current session.

Use **Saved for review → Export review notes** to keep a portable JSON copy before clearing browser data, moving the file or changing browsers. There is no account, synchronization service or server backup. Do not enter sensitive financial information into this planning overview.

## Optional development commands

Node.js 18 or newer is sufficient for the commands below. There are no npm dependencies to install for the normal build, server or unit tests.

```sh
npm run build   # Rebuild index.html from src/ and data/atlas.json
npm start       # Optional loopback-only server at http://127.0.0.1:4173
npm test        # Run the atlas's own 24 data-integrity and E03 fixture tests
```

Equivalent commands without npm:

```sh
node scripts/build.mjs
node scripts/serve.mjs
node --test tests/*.test.cjs
```

The optional server listens only on `127.0.0.1`. Set `PORT` to use a different port. Opening `index.html` directly remains the simplest viewing option. The architecture of this atlas does **not** choose the implementation stack for the actual financial product.

## File layout

```text
financial-plan-atlas/
├── index.html                       # Start here; self-contained built overview
├── README.md
├── START-HERE.txt
├── package.json                     # No runtime/build dependencies
├── docs/
│   ├── requirements-v3.2.md          # Complete, unchanged source document
│   ├── VERIFICATION.md              # Scope and limitations of testing
│   └── THIRD-PARTY-NOTICES.md
├── src/
│   ├── app.js                       # Navigation, search, catalogues and detail views
│   ├── content.js                   # Editorial summaries and explicit graph edges
│   ├── demo-engine.js               # Integer-minor-unit E03 calculation
│   ├── icons.js                     # Inline vector icons
│   └── styles.css                   # Responsive and print styles
├── data/
│   ├── atlas.json                   # Complete parsed text, HTML, relationships and references
│   ├── math-cache.json              # Preconverted math expressions
│   └── content-validation.json
├── scripts/
│   ├── build.mjs                    # Zero-dependency single-file packaging
│   ├── serve.mjs                    # Optional local server
│   ├── extract.py                   # Optional Markdown re-extraction
│   └── math.cjs                     # Optional build-time TeX → MathML conversion
├── tests/
│   ├── content.test.cjs
│   ├── engine.test.cjs
│   ├── browser_smoke.py
│   └── browser-report.json
└── preview/                         # Desktop and mobile screenshots
```

## Editing the overview

Edit `src/styles.css` to change styling, `src/content.js` to change editorial navigation/graph connections, and `src/app.js` to change interactions. Then run `npm run build`.

The source document remains authoritative. Short domain names and relationship maps are editorial navigation aids. Feature-to-model edges come directly from the original feature register. Research links follow the model citations; no missing citation is silently invented.

### Regenerating from a changed Markdown document

This optional path needs content-conversion tools; it is not needed to view, rebuild or normally edit the overview.

```sh
python -m pip install -r requirements-build.txt
npm install --no-save mathjax-full@3.2.1
python scripts/extract.py
npm run build
npm test
```

Edit `docs/requirements-v3.2.md` before re-extraction. The extraction script is tailored to the numbered sections and register formats in this version. If the specification changes its IDs or structure, update the parser, counts, editorial mappings and integrity tests together. Do not treat a changed document as validated merely because it builds.

MathJax is used **only during optional content extraction**, not in the shipped runtime. The shipped MathML uses the browser's own mathematical rendering. No font files are included.

## Verification and limits

The package includes:

- **24 passing Node tests** for content completeness, provenance links, source identity, offline packaging and the E03 arithmetic/ordering/bounds.
- **74 passing browser checks** covering navigation, filtering, diagrams, source drill-down, mathematical rendering, privacy illustrations, review annotations, desktop/tablet/mobile containment and zero runtime network requests.

These are tests of **the atlas**, not passes against the 80 acceptance cases for the future financial product.

The managed test environment blocked browser navigation to both local server and file URLs. Browser checks therefore rendered the **exact built HTML bytes** in a fresh Chromium document and exercised the scripts, styles and interactions there. This verifies rendering and interaction but does not certify direct file-protocol launches, local-storage persistence or every browser. The included optional server was not navigated successfully from that managed browser. See `docs/VERIFICATION.md` for the precise scope.

The research bibliography is carried forward from the source requirements. External destinations have not been independently re-audited for this atlas. There is no claim that its mathematical models have all been implemented, that the privacy demonstrator enforces real security, or that any real-world financial action is guaranteed safe.

# Screen refresh: what the SVG mockups changed

A set of 88 SVG mockups (32 canonical screens in both themes plus 56 supporting views) was
reviewed against the shipped screens. They are drawn from the same written contracts this app
was built from, so the difference between them and the app is **placement**, not content — which
is the only thing taken from them here.

## What was taken, and what was not

| Taken | Not taken |
|---|---|
| Where an element sits, and next to what | Their palette. The app keeps its own light and dark theme tokens |
| How a band of figures is divided into columns | Their shape language, except where noted below |
| Which control belongs in the header, the footer or beside a heading | Pill-shaped buttons in the title bar — an icon button is right |
| Master–detail where the app had a long list | The command palette, which this app does not have |

The mockups are illustrations, not a pixel specification: several have defects (a group label on
one sidebar group only, captions that collide, a fictitious forecast fingerprint). Where one
contradicts the written contract or the engine, the contract wins.

## The recurring layout ideas

These came up on most screens and are now shared scaffolding rather than per-screen code.

1. **An even column grid, not a wrapping row of fixed cards.** Six supporting figures read as one
   grid across the full width; the same six as two ragged rows of three leave the right third of
   a 1600 px window empty. `widgets::states::columns` and `columns_leading`.
2. **The equation, not the table.** A screen states the one-line equation behind its leading
   figure (`4,400,000 liquid cash − 1,750,000 reserved cash = 2,650,000 free current cash`) with
   `Full calculation…` beside it. The chain as a table belongs in the calculation sheet, which
   that command opens. `widgets::explain::render_equation`. This alone gave Today and Forecast
   back about a third of their height.
3. **An `ⓘ` at the trailing edge of a figure**, not an `Explain…` link after its label. Six
   figures in a row have no room for six links, and the label already names the figure. The
   element id is unchanged, so the tests and the tooltip still find it.
4. **The leading figure outweighs its supporting ones** (`text_3xl` against `text_xl`), so the
   number the screen answers with is obvious before anything is read.
5. **Bands are ruled off from each other** (`Section::divider`), and a heading may carry the
   reading it is under as a badge (`Expected · Baseline`) instead of repeating it in prose.
6. **A fact about the screen goes in a bordered card**, not in a muted sentence that reads as an
   afterthought: `No hard-floor breach`, `Precedence is explicit`, `Ownership is not a viewer
   switch`. `widgets::states::info_card`. An `Alert` stays for something actually wrong.
7. **The screen's own commands sit in a bar at its foot**, left for where it can take you, right
   for what it can do. `widgets::states::action_bar`.
8. **A detail says where it sits** — `Accounts / Joint savings` above the title.
   `widgets::states::detail_header`.
9. **Master–detail for a register you inspect one of** (policies, scenarios): the list on the
   left, the selected object's detail beside it, instead of a detail below a list you have to
   scroll past.
10. **Filters are one inline row** with the count line under them, not a row of stacked
    label-over-control pairs above the count.

## Layout rules that still hold

`docs/perf.md` §3 is unchanged and every one of these is written to obey it. In particular the
new column grid gives each cell a **definite** width (a fraction of a definite parent) and puts
the gutter inside the cell as padding, because a `gap` on top of relative widths overflows the
row and an auto-width cell is re-measured at every ancestor's sizing pass.

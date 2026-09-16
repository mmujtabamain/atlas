# gpui-pre 0.3.4, with a paint-time scale

This is the `gpui-pre` crate the gpui-kit 0.6 stack builds on, exactly as published
(the 43 examples left out), plus one addition the workspace needs and the engine does
not offer. `Cargo.toml` at the workspace root points the whole dependency graph here
through `[patch.crates-io]`.

## What was added

- `Window::with_scale(scale, origin, f)` — paints everything drawn inside `f` scaled
  by `scale` about `origin` (window coordinates), the way a CSS `transform: scale()`
  with a `transform-origin` does. The subtree is laid out and prepainted at its normal
  size; only its painted result is scaled. Scales nest.
- `Window::paint_scale`, `is_paint_scaled`, `transform_point`, `transform_bounds`,
  `transform_length` — the scale in force and how it moves a point, a rectangle, a
  length.
- The scale is applied where primitives enter the scene — `paint_quad`,
  `paint_drop_shadows`, `paint_inset_shadows`, `paint_path`, `paint_underline`,
  `paint_strikethrough`, `paint_glyph`, `paint_emoji`, `paint_svg`, `paint_image`,
  `paint_surface`, `paint_layer` — so boxes, borders, radii, shadows, text (rasterized
  at the scaled size, so it stays crisp), images and paths all scale together. Content
  masks pushed inside the scope (`with_content_mask`) scale with their content; masks
  pushed outside it keep clipping where they were.
- `Path::transform_points` (`scene.rs`), for the path case.
- The glyph culling in `text_system/line.rs` compares the glyph's *painted* rectangle
  with the content mask, so scaled text is not clipped at the start of its runs.
- A mouse-up counts as mouse input for the input modality (`Window::dispatch_event`):
  hitboxes are not hovered while the last input was a key, and without this a key
  pressed mid-drag (Space cycles a drop target) made the release that followed miss
  its drop target unless the pointer moved first.

Untouched on purpose: layout, hitboxes and scroll geometry. A scaled subtree's
controls still respond where they were laid out, so a caller that scales a subtree
visibly takes its clicks and its wheel for the duration (the workspace's `Scaled`
element does, in the capture phase, and its keystroke interceptor swallows keys) while
drags and drops pass.

## Where it is used

`crates/atlas-app/src/workspace/scaled.rs` — the `Scaled` element — and
`crates/atlas-app/src/workspace/skin.rs`, which scales each pane card about its centre
while a tab is held.

## Keeping it in step

The change is one commit on this repository ("build: vendor gpui-pre with a paint-time
scale"); the diff against the published crate is that commit. When upstream gpui gains a
subtree transform, or when the gpui-kit authors take the patch, delete `vendor/gpui-pre`
and the `[patch.crates-io]` entry and switch `Scaled` to the upstream API.

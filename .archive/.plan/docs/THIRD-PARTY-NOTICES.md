# Third-party material and tooling

## User-provided requirements

`requirements-v3.2.md` is the complete source document provided in the conversation. It is reproduced unchanged. Research citations and descriptions in the atlas come from that document; its original evidence-access and limitation notes are retained. The cited research works are not bundled as full papers.

## Runtime

The overview uses custom JavaScript, CSS, inline SVG icons/diagrams and precompiled native MathML. There is no runtime framework dependency, external CDN, analytics SDK, remote font service or bundled font file.

## Optional content-conversion tooling

- **Mistune 3.2.1** — BSD-3-Clause; used to render the provided Markdown during extraction. Project: <https://github.com/lepture/mistune>.
- **MathJax 3.2.1** (`mathjax-full`) — Apache-2.0; used during extraction to convert the TeX expressions into native MathML. Project: <https://github.com/mathjax/MathJax-src>. Documentation: <https://docs.mathjax.org/en/v3.2/server/start.html>.

The libraries and their font files are not included in the ZIP. Only the generated mathematical markup is included. A later upgrade to a content-conversion tool should be tested against all source equations.

## Technical reference

Native MathML concepts and browser rendering are documented by MDN: <https://developer.mozilla.org/en-US/docs/Web/MathML>. This is a technical reference for the atlas, not an additional financial-research source.

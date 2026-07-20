# TerminalBench landing page

Pure static landing page for the TerminalBench terminal-emulator performance reporting and comparability protocol.

## Preview

From the repository root:

```sh
python3 -m http.server 4173 --directory site
```

Then open `http://localhost:4173/` for English or `http://localhost:4173/ja/` for Japanese.

## Files

- `index.html` — English default page, localized metadata, and visible-parity JSON-LD
- `ja/index.html` — fully static Japanese page with the same evidence and interaction model
- `styles.css` — local system-font design, responsive layout, system dark mode, reduced-motion and forced-colors support
- `script.js` — optional command-copy enhancement and quickstart focus transfer
- `tokens.tokens.json` — DTCG-shaped design token source
- `favicon.svg` — reduced Pairwise Gate mark
- `og.svg` / `og.png` — editable and crawler-compatible social preview assets
- `robots.txt` / `sitemap.xml` — crawl and canonical discovery policy

There are no build steps, external fonts, third-party scripts, trackers, locale redirects, cookies, or runtime dependencies. Both locales and their full safe sequences remain available with JavaScript disabled. Language selection is always explicit through reciprocal links.

## Canonical URL

The current canonical is `https://simota.github.io/TerminalBench/`. It is a reversible deployment assumption. If the origin changes, update these together:

1. canonical, hreflang, Open Graph, X/Twitter, and JSON-LD URLs in `index.html` and `ja/index.html`;
2. the sitemap URL in `robots.txt`;
3. both locale URLs and XHTML alternates in `sitemap.xml`.

## Verification

Validate both HTML files, their JSON-LD, reciprocal hreflang and language links, `tokens.tokens.json`, SVG/XML, and JavaScript syntax. Then test both locales at desktop and 320px layouts, keyboard-only navigation, 200%/400% zoom, system dark mode, reduced motion, forced colors, and JavaScript-disabled use. Lighthouse lab scores do not verify field Core Web Vitals.

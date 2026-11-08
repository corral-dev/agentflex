# Embedded bitmap fonts

- `spleen-16x32-subset.json` — subset of [Spleen](https://github.com/fcambus/spleen) 16×32, BSD-2-Clause; see `SPLEEN-LICENSE.txt`.
- `fusion-pixel-12px-subset.json` — 24px (2×) subset of [Fusion Pixel Font](https://github.com/TakWolf/fusion-pixel-font) 12px Mono zh_Hans, release 2026.10.25, SIL Open Font License 1.1; see `FUSION-PIXEL-LICENSE.txt`.

The CJK subset contains only the characters used by `src/`. Regenerate it after changing Chinese copy:

```bash
python3 scripts/build-cjk-subset.py /path/to/fusion-pixel-12px-monospaced-zh_hans.bdf
```

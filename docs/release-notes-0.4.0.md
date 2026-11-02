# Flea 0.4.0

- Ship the GPUI interface and 50 language catalogs with 2,219 source strings.
- Repair picker input and menu behavior (#324, @gjouret).
- Decode Quick Look pictures at their displayed physical size and preserve animated GIF and WebP frames (#336, @gjouret).
- Keep Kind pinned to the right edge during column divider drags (#349, @felixzsh).
- Bind Ctrl+Plus to larger text while preserving the existing zoom shortcuts (#302, @root1945).
- Keep BOM-prefixed CSV headers aligned in previews (#356, @AksharP5).

## Known issue

Automatic Glassy backgrounds in GPUI remain opaque at alpha 255, rather than the alpha 192 used by v0.3.12. The transparency repair is deferred to 0.4.1.

Thanks @gjouret, @felixzsh, @root1945 and @AksharP5.

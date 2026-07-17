# App icons

The bundler reads `32x32.png`, `128x128.png`, `128x128@2x.png`, `icon.icns`, and
`icon.ico` (see `../tauri.conf.json`). Generate the full set from the source SVG:

```bash
npm run tauri icon src-tauri/icons/sqz.svg
```

This overwrites the PNG/ICNS/ICO files in this directory. The generated binaries
are committed so `tauri build` works from a clean checkout; regenerate whenever
`sqz.svg` changes. CI also runs this step before building.

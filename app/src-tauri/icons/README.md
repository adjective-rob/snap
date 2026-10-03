`icon.png` is the tray icon. It is embedded into the binary at compile time (`include_bytes!` in `src/main.rs`), so replacing it requires a rebuild.

`icon.ico` is the Windows executable and installer icon; the Windows build fails without it. It was generated from the 32x32 `icon.png`, so it is low resolution.

To generate the full set of bundle icons from a source image:

    npx tauri icon path/to/icon.png

`icon.png` is the tray icon. It is embedded into the binary at compile time (`include_bytes!` in `src/main.rs`), so replacing it requires a rebuild.

To generate the full set of bundle icons from a source image:

    npx tauri icon path/to/icon.png

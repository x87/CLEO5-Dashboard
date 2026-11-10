# CLEO5 Dashboard

An in-game overlay for CLEO 5 (GTA San Andreas 1.0 US).

Press **Insert** (rebindable) to open and close it.

<img width="640" height="480" alt="image" src="https://github.com/user-attachments/assets/ec27f9ce-1c70-41d3-a155-421d1fdfc3a3" /> <img width="640" height="480" alt="image" src="https://github.com/user-attachments/assets/2fe9dec8-0f10-4982-96dc-19751d855c2c" />


## Tabs

- **Scripts** - loaded scripts. From here you can change a script's compatibility mode, turn debug mode on and off, suspend or resume it, dump it to disk, or reload it.
- **Libraries** - loaded memory libraries and their allocations. Needs `SA.MemoryOperations.cleo`.
- **Audio** - active CLEO audio streams with their stats. Needs `SA.Audio.cleo`.
- **Logs** - live read of `cleo.log` or `cleo_script.log`, with auto-scroll.
- **Settings** - theme, UI scale, menu opacity, the hotkey, and other options. Stored in `CLEO/.cleo_config.ini` under `[Plugins]`, so you can also edit them by hand.

The overlay renders with [hudhook](https://github.com/veeenu/hudhook) (D3D9).

## Requirements

- GTA San Andreas 1.0 US (tested with Compact and Hoodlum)
- The [latest preview build](https://github.com/cleolibrary/CLEO5/releases/tag/preview) of CLEO 5

## Build

You need the 32-bit MSVC Rust toolchain (`rustup target add i686-pc-windows-msvc`) and a Visual Studio C++ build environment. Then:

```
cargo build --release
```

## License

MIT

The bundled `SourceCodePro-Regular.ttf` is © 2010, 2012 Adobe Systems Incorporated, licensed under the [SIL Open Font License 1.1](https://fonts.adobe.com/variations/17454/eula).

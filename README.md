# CLEO5 Dashboard
[![Discord](https://img.shields.io/discord/911487285990674473?style=for-the-badge)](https://cleo.li/discord)

An in-game overlay for CLEO 5 (GTA San Andreas 1.0 US). 

Press **`** (backtick, rebindable) to open and close it.

<img width="640" height="480" alt="image" src="https://github.com/user-attachments/assets/ec27f9ce-1c70-41d3-a155-421d1fdfc3a3" /> <img width="640" height="480" alt="image" src="https://github.com/user-attachments/assets/2fe9dec8-0f10-4982-96dc-19751d855c2c" />

## Tabs

- **Scripts** - loaded scripts, including game's native scripts, with filters and extra info. Includes a debugger with live trace and stepping through the code functionality.
- **Libraries** - loaded modules, including `.asi`, `.dll`, `.cleo` and other libraries.
- **Audio** - active CLEO audio streams with their stats. Needs `SA.Audio.cleo`.
- **Logs** - live read of `cleo.log` or `cleo_script.log`, with auto-scroll.
- **Settings** - theme, UI scale, menu opacity, the hotkey, and other options. Stored in `CLEO/.cleo_config.ini` under `[Plugins]`, so you can also edit them by hand.

The overlay renders with [hudhook](https://github.com/veeenu/hudhook) (D3D9).

## Requirements

- GTA San Andreas 1.0 US (tested with Compact and Hoodlum)
- [CLEO 5.5.1](https://github.com/cleolibrary/CLEO5/releases) or later

## License

MIT

The bundled `SourceCodePro-Regular.ttf` is © 2010, 2012 Adobe Systems Incorporated, licensed under the [SIL Open Font License 1.1](https://fonts.adobe.com/variations/17454/eula).

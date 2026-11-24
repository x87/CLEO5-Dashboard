# CLEO5 Dashboard

[![Discord](https://img.shields.io/discord/911487285990674473?style=for-the-badge)](https://cleo.li/discord)

An in-game overlay for CLEO 5 (GTA San Andreas 1.0 US).

Press **`** (backtick, rebindable) to open and close it.

<img width="640" height="360" alt="image" src="https://github.com/user-attachments/assets/cb4586ae-a07c-4f59-a6ed-57d6494bc26d" />

## Trailer

[![Watch trailer](https://github.com/user-attachments/assets/b0c1efbb-b666-4cfd-9f62-f7a24200b3ab)](https://www.youtube.com/watch?v=_EIoOoPcVZs)

## Features

- **Scripts** - loaded scripts, including game's native scripts, with filters and extra info. Includes a debugger with live trace and stepping through the code functionality.
- **Libraries** - loaded modules, including `.asi`, `.dll`, `.cleo` and other libraries.
- **Audio** - active CLEO audio streams with their stats. Needs `SA.Audio.cleo`.
- **Logs** - live read of `cleo.log` or `cleo_script.log`, with auto-scroll.
- **Settings** - theme, UI scale, menu opacity, the hotkey, and other options. Changes are persistent (See [Settings](#Settings)).

The overlay renders with [hudhook](https://github.com/veeenu/hudhook) (D3D9).

## Settings

Values live in `[Plugins]` of `CLEO/.cleo_config.ini`:

| Key                               | Default                                              | Meaning                                                                  |
| --------------------------------- | ---------------------------------------------------- | ------------------------------------------------------------------------ |
| `Dashboard.General.Theme`         | 2                                                    | theme index: 0 = Blue, 1 = Purple, 2 = Orange                            |
| `Dashboard.General.ToggleKey`     | [Backtick](https://en.wikipedia.org/wiki/Backtick) ` | Normally right above the Tab button, may vary depending on your keyboard |
| `Dashboard.General.UiScale`       | 1.0                                                  | UI scale, 0.5-2.0                                                        |
| `Dashboard.General.MenuOpacity`   | 0.9                                                  | menu opacity, 0.15-1.0                                                   |
| `Dashboard.General.FreezeInput`   | 1                                                    | stop game input while the menu is open                                   |
| `Dashboard.General.ReleaseCursor` | 1                                                    | release the OS cursor while the menu is open                             |
| `Dashboard.General.ShowFps`       | 1                                                    | FPS counter in the title bar                                             |
| `Dashboard.General.OpenOnStart`   | 0                                                    | open the menu as soon as the overlay starts                              |
| `Dashboard.General.Highlight`     | 0                                                    | colour log and trace lines by what their parts are                       |

## Logs

Important messages are logged in `cleo.log` (check your CLEO configuration to locate it, the root directory is default), each prefixed `[Dashboard]`. To help with troubleshooting enable additional logs in `.cleo_config.ini`:

```ini
[Plugins]
Dashboard.Debug.Log=1
```

These extra logs will go into `SA.Dashboard.log` in cleo.log directory.

## Requirements

- GTA San Andreas 1.0 US (tested with Compact and Hoodlum)
- [CLEO 5.5.1](https://github.com/cleolibrary/CLEO5/releases) or later

## License

MIT

The bundled `SourceCodePro-Regular.ttf` is © 2010, 2012 Adobe Systems Incorporated, licensed under the [SIL Open Font License 1.1](https://fonts.adobe.com/variations/17454/eula).

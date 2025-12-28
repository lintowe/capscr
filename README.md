# capscr

Cross-platform screen capture with HDR support, GIF recording, and cloud upload.

## Features

- Full screen, window, and region capture
- HDR capture with tone mapping
- GIF recording
- Upload to Imgur or custom endpoints
- Global hotkeys
- Clipboard integration

## Install

Download from [Releases](https://github.com/lintowe/capscr/releases):
- **Windows**: `.exe` installer or portable `.zip`
- **Linux**: `.tar.gz`

## Build

```
cargo build --release
```

## Hotkeys

- `Ctrl+Shift+S` - Capture screen
- `Ctrl+Shift+W` - Capture window
- `Ctrl+Shift+R` - Capture region
- `Ctrl+Shift+G` - Record GIF

## Configuration

Settings are stored in:
- Windows: `%APPDATA%\capscr\config.toml`
- macOS: `~/Library/Application Support/capscr/config.toml`
- Linux: `~/.config/capscr/config.toml`

## License

MIT

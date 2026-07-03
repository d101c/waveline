# waveline

**Listen to Mixcloud & SoundCloud from your terminal**, in one place — a
clickable TUI, vim keybindings, spectrum analyzer, media keys.

```sh
npx waveline
```

That's it. `npx` downloads a tiny launcher that fetches the Rust binary
matching your machine from the [GitHub Releases](https://github.com/d101c/waveline/releases),
caches it, and runs it.

## Requirements

- **Linux** (x86_64 or arm64) — audio output uses **PipeWire** (`pw-play`)
  or **ALSA** (`aplay`), available on most distributions.
- Node ≥ 14 (only for this launcher; the binary itself doesn't depend on it).

## Permanent install

```sh
npm install -g waveline   # then: waveline
```

Or without Node at all:

```sh
cargo install waveline                 # from crates.io
cargo binstall waveline                # pre-built binary
```

## Usage

`c` connect your accounts · `/` search · `:` paste a URL · `Space`
play/pause · `v` cycle the visualizer · `L` switch language (English/French)
· `?` help · `q` quit.

Source code, documentation, and other install methods:
<https://github.com/d101c/waveline>

MIT

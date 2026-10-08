# Changelog

All notable changes to waveline are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/); versions follow
[SemVer](https://semver.org/).

## [0.2.0] — 2026-10-08

### Added
- **Queue**: `a` appends the selection, `x` removes, `X` clears, Enter in the
  Queue view plays and consumes. `n` and auto-advance read the queue first.
  The sidebar shows `Queue (n)`.
- **History**: every track played is recorded (most recent first,
  deduplicated, capped at 200). waveline reopens on History when it is not
  empty. Queue and history persist in `~/.local/share/waveline/state.json`
  (atomic writes).
- **Help window** (`?`): generated from the single key table in
  `keymap.rs`, two balanced columns from 80 columns wide, closes on any key.
- **Mouse**: click the progress bar to seek, scroll over the playbar for
  volume, click the "accounts" row to connect.
- **Pagination** with `Ctrl-d` / `Ctrl-u` (half page) and PageDown / PageUp;
  `Ctrl-u` / `Ctrl-w` clear the line / the last word while typing.
- **Bracketed paste**: pasted text arrives in one piece; pasting a
  SoundCloud/Mixcloud link in normal mode opens the `:` prompt pre-filled.
- **MPRIS seek**: `Seek` / `SetPosition` work; `CanSeek` reflects the stream.
- A busy spinner while a search or a library section is loading; per-section
  empty-list hints.
- Volume and visualizer style are remembered across sessions.
- `waveline --version`.

### Changed
- Each sidebar section keeps its own list: switching between Search and
  Likes no longer reloads anything.
- SoundCloud and Mixcloud are queried **in parallel** (scoped threads); a
  failing platform is reported in the status bar instead of being confused
  with "no results".
- A single HTTP agent (connection keep-alive pool) is shared by the whole
  process; the SoundCloud `client_id` cache is process-wide instead of
  per-thread.
- `p` restarts the current track after 3 seconds (seek, or replay for
  non-seekable HLS) and goes to the previous track before that.
- Preferences are written once at exit (plus on explicit changes) instead
  of on every key press.

### Fixed
- Auto-advance no longer replays the last track of the list forever; it
  advances from the track that was playing, not from the cursor.
- A late search response could overwrite newer results; requests now carry a
  generation id and stale responses are dropped.
- A panic left the terminal in raw mode with mouse capture on; a panic hook
  now restores it before printing the error.
- Mixcloud GraphQL lookups escape the username/slug taken from the URL.
- `Ctrl-c` while typing inserted a `c` instead of quitting.

## [0.1.2] — 2026-07-09

- Seek ±10 s with acceleration on repeated presses; keyboard shortcuts
  reworked (`h`/`l` seek, `Tab` switches panel).

## [0.1.1] — 2026-07-03

- English by default, French switchable from the sidebar.

## [0.1.0] — 2026-07-01

- First public release: unified SoundCloud + Mixcloud TUI, native playback,
  search, account mode by public handle, spectrum analyzer, MPRIS.

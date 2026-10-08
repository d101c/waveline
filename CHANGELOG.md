# Changelog

All notable changes to waveline are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/); versions follow
[SemVer](https://semver.org/).

## [0.2.0] — 2026-10-08

### Added
- **Queue**: `a` appends the selection, `x` removes, `X` clears; playing a
  track from the Queue view (Enter, click, Space, `p`) consumes it. `n` and auto-advance read the queue first.
  The sidebar shows `Queue (n)`.
- **History**: every track played is recorded (most recent first,
  deduplicated, capped at 200). While you listen *from* the History view,
  the list stays in place so "next" walks down to older entries. waveline
  reopens on History when it is not empty. Queue and history persist in `~/.local/share/waveline/state.json`
  (atomic writes).
- **Help window** (`?`): generated from the single key table in
  `keymap.rs`, two balanced columns from 80 columns wide, closes on any key.
- **Mouse**: click the progress bar to seek, scroll over the playbar for
  volume, click the "accounts" row to connect.
- **Pagination** with `Ctrl-d` / `Ctrl-u` (half page) and PageDown / PageUp;
  `Ctrl-u` / `Ctrl-w` clear the line / the last word while typing. The list
  cursor is remembered per section.
- **Bracketed paste**: pasted text arrives in one piece; pasting a
  SoundCloud/Mixcloud link in normal mode opens the `:` prompt pre-filled.
- **MPRIS seek**: `Seek` / `SetPosition` work (metadata now carries the
  mandatory `mpris:trackid`); `CanSeek` reflects the stream.
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
- Preferences (language, accounts, and now volume and visualizer style)
  are written on explicit changes and once at exit, only when something
  changed. Both `config.json` and `state.json` are written atomically.
- Network requests carry a generation id and share one result channel; a
  response from a superseded request of the same section is dropped; the
  response of the most recent request brings its section on screen, an
  older one for another section is stored without stealing the view.
- `config.json` is parsed field by field: a hand-edited value out of range
  or of the wrong type falls back to its default without discarding the
  other fields, and an unparseable file is set aside as `config.json.bak`
  instead of being overwritten.
- SoundCloud searches and library loads now recover from an expired
  `client_id` (re-scrape and retry once), not only playback.

### Fixed
- Auto-advance no longer replays the last track of the list forever; it
  advances from the track that was playing, not from the cursor.
- A panic left the terminal in raw mode with mouse capture on; a panic hook
  now restores it before printing the error.
- Mixcloud GraphQL lookups escape the username/slug taken from the URL.
- `Ctrl-c` while typing inserted a `c` instead of quitting.
- Mixcloud tracks from search or library lists were never recognized as the
  track being played (different id forms), so the ▶ marker never showed on
  them and "next" fell back to the cursor.
- At the end of a list the engine is now stopped explicitly ("Nothing
  playing") instead of keeping the finished track displayed as paused.
- Quitting while a track is still resolving no longer blocks the exit until
  the network timeout.
- A list containing the same Mixcloud mix twice no longer makes auto-advance
  loop on it.
- `state.json` is read entry by entry; a damaged file is set aside as
  `state.json.bak` instead of being emptied.

## [0.1.2] — 2026-07-09

- Seek ±10 s with acceleration on repeated presses; keyboard shortcuts
  reworked (`h`/`l` seek, `Tab` switches panel).

## [0.1.1] — 2026-07-03

- English by default, French switchable from the sidebar.

## [0.1.0] — 2026-06-22

- First public release: unified SoundCloud + Mixcloud TUI, native playback,
  search, account mode by public handle, spectrum analyzer, MPRIS.

# waveline — textes prêts à publier

Copie-colle selon le canal. Versions FR et EN.

## Tagline (une ligne)

- FR : **Mixcloud & SoundCloud, réunis dans ton terminal.**
- EN : **Mixcloud & SoundCloud, together in your terminal.**

Variantes : « Un seul lecteur TUI pour tes mixes Mixcloud et SoundCloud. » ·
« The TUI music player for Mixcloud & SoundCloud. »

## Description courte (annuaires, npm, crates.io — ~140 car.)

- FR : TUI pour écouter Mixcloud & SoundCloud au même endroit : cliquable,
  raccourcis vim, recherche unifiée, analyseur de spectre, touches média.
- EN : A terminal UI to play Mixcloud & SoundCloud in one place: clickable,
  vim keys, unified search, spectrum analyzer, media-key control.

## Description longue (Product Hunt / README / Terminal Trove)

> **waveline** réunit Mixcloud et SoundCloud dans un seul lecteur en mode texte.
> Cherche sur les deux plateformes à la fois, colle une URL, connecte tes
> comptes (juste ton pseudo public — pas d'OAuth, rien de sensible) pour
> retrouver tes likes, playlists et écoutes. Tout est pilotable **à la souris
> comme au clavier** (raccourcis vim), avec un **analyseur de spectre** intégré
> (3 styles : barres, miroir, oscilloscope) et le support des **touches média**
> du clavier via MPRIS (panneau GNOME, écran de verrouillage).
>
> C'est un **binaire Rust autonome** : pas de mpv, pas de yt-dlp, pas de Python,
> pas de tokio — il décode l'audio lui-même. `npx waveline` et c'est parti.

## Show HN

**Titre :** `Show HN: waveline – a TUI to play Mixcloud and SoundCloud in one place`

**Corps :**
> I wanted one place to play both my Mixcloud mixes and my SoundCloud likes from
> the terminal, so I built waveline — a clickable Rust TUI (ratatui) that
> unifies both: cross-platform search, paste-a-URL, your likes/playlists by
> entering just your public handle (no OAuth), a built-in spectrum analyzer
> (bars / mirror / scope, toggle with `v`), and hardware media-key control via
> MPRIS.
>
> It's a single static binary — no mpv, no yt-dlp, no Python: it resolves the
> streams and decodes the audio itself (symphonia), output via PipeWire/ALSA.
> Linux only for now.
>
> Try it: `npx waveline` — or `cargo install waveline`.
> Code: https://github.com/d101c/waveline
>
> Honest limitation: major-label monetized SoundCloud tracks are DRM (encrypted
> HLS) and unplayable by any third-party client; waveline detects and skips
> them. Everything else — mixes, podcasts, independent uploads — plays.

## Product Hunt

- **Name:** waveline
- **Tagline:** Mixcloud & SoundCloud, together in your terminal
- **Icon:** `assets/icon.svg` (exporter en PNG 512×512)
- **Topics:** Music, Developer Tools, Open Source, Linux
- **First comment (maker):**
  > Hi! waveline is a terminal music player that finally puts my Mixcloud mixes
  > and SoundCloud likes in the same place. Clickable + vim keys, a spectrum
  > visualizer, and it responds to the keyboard's media keys. Single Rust binary,
  > no external player. `npx waveline` to try it. Feedback very welcome 🙏

## Reddit

- **r/rust :** `waveline: a unified Mixcloud + SoundCloud TUI player (single Rust binary, ratatui, symphonia, MPRIS)`
- **r/commandline :** `waveline – play Mixcloud & SoundCloud from one TUI (npx waveline)`
- **r/unixporn :** poster un screenshot/GIF avec l'oscilloscope en action.

## Phrase d'install (à mettre partout)

```
npx waveline      # ou : cargo install waveline
```

# waveline

**Écoute Mixcloud & SoundCloud depuis ton terminal**, au même endroit — TUI
cliquable, raccourcis vim, analyseur de spectre, touches média.

```sh
npx waveline
```

C'est tout. `npx` télécharge un petit launcher qui récupère le binaire Rust
adapté à ta machine depuis les [Releases GitHub](https://github.com/d101c/waveline/releases),
le met en cache, et le lance.

## Prérequis

- **Linux** (x86_64 ou arm64) — la sortie audio utilise **PipeWire** (`pw-play`)
  ou **ALSA** (`aplay`), présents sur la plupart des distributions.
- Node ≥ 14 (uniquement pour ce launcher ; le binaire lui-même n'en dépend pas).

## Installation permanente

```sh
npm install -g waveline   # puis : waveline
```

Ou sans Node du tout :

```sh
cargo install waveline                 # depuis crates.io
cargo binstall waveline                # binaire pré-compilé
```

## Utilisation

`c` connecter tes comptes · `/` rechercher · `:` coller une URL · `Espace`
play/pause · `v` changer de visualiseur · `?` aide · `q` quitter.

Code source, documentation et autres modes d'installation :
<https://github.com/d101c/waveline>

MIT

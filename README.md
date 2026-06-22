# waveline

**TUI unifiée pour écouter Mixcloud & SoundCloud depuis la console.** Un seul
endroit, fluide, cliquable, avec des raccourcis vim — et un binaire Rust
autonome qui décode et joue l'audio lui-même, sans `mpv`, sans `yt-dlp`, sans
runtime Python.

```
┌ waveline ───────────────────────────────────[ Tout  SC  MC ]─┐
│ ╭ Sources ──────╮ ╭ Recherche · Tout ─────────────────────╮  │
│ │ ♥  Likes      │ │ ▶ Bonobo — Kerala            3:57  SC  │  │
│ │ ☰  Playlists  │ │   Ben UFO — Rinse FM set  1:02:11  MC  │  │
│ │ ◎  Feed       │ │   Four Tet — Two Thousand…   4:10  SC  │  │
│ │ ⌕  Recherche  │ │   Gilles Peterson — WW Show  2:00  MC  │  │
│ │ ⧗  Historique │ │   …                                    │  │
│ │ ▤  File       │ │                                        │  │
│ ╰───────────────╯ ╰────────────────────────────────────────╯  │
├───────────────────────────────────────────────────────────────┤
│ ▶ Bonobo — Kerala        ███████░░░░░░░░  1:48 / 3:57          │
╰───────────────────────────────────────────────────────────────╯
```

## Pourquoi

Mixcloud et SoundCloud sont deux maisons séparées : deux apps, deux onglets,
deux files d'attente. waveline les réunit dans le terminal — naviguer,
chercher, et écouter les deux au même endroit, à la souris **ou** au clavier.

## Caractéristiques

- **Deux plateformes, une interface** — modèle unifié, recherche entrelacée.
- **Lecture native** — décodage 100 % Rust (`symphonia` : MP3, AAC/MP4), sortie
  via PipeWire (`pw-play`) ou ALSA (`aplay`). Aucun lecteur externe requis.
- **Cliquable ET clavier** — clic sur une piste pour jouer, clic sur les onglets
  de filtre, la barre play/pause ; ou tout au clavier en style vim.
- **Avec et sans compte** — utilisable immédiatement sans login (URLs publiques
  + recherche) ; l'accès à *tes* likes/playlists arrive (voir
  [roadmap](#roadmap)).
- **Autonome, peu de dépendances** — un binaire, HTTP pur-Rust (`ureq`+rustls),
  pas de `tokio`, pas d'OpenSSL système, pas de `yt-dlp`.

## Installation

Prérequis : Rust ≥ 1.96, et **`pw-play`** (PipeWire) ou **`aplay`** (alsa-utils)
pour la sortie son — présents sur la plupart des distributions Linux.

```sh
git clone git@github.com:d101c/waveline.git
cd waveline
cargo build --release
./target/release/waveline
```

## Utilisation

Lance `waveline`, puis :

| Touche | Action | | Touche | Action |
|---|---|---|---|---|
| `j` / `k` ou `↑`/`↓` | naviguer | | `/` | rechercher (SC + MC) |
| `Entrée` / clic | jouer la sélection | | `:` | coller une URL et jouer |
| `Espace` | play / pause | | `1` `2` `3` | filtre Tout / SC / MC |
| `n` / `p` | suivant / précédent | | `Tab` | changer de panneau |
| `+` / `-` | volume | | `q` | quitter |
| `g` / `G` | haut / bas de liste | | `?` | aide |

### Modes ligne de commande (debug)

```sh
waveline resolve <url>        # affiche le flux résolu d'une URL
waveline play <url> [secondes] # joue le flux N secondes (test moteur)
waveline search <requête>      # recherche unifiée SC + MC
```

## Architecture

```
src/
├── main.rs         cycle terminal, boucle d'événements, câblage des effets
├── app.rs          état pur + logique (testable, sans I/O) → émet des Effect
├── ui.rs           rendu ratatui + cartographie des zones cliquables
├── model.rs        Track unifié (SoundCloud ⇄ Mixcloud)
├── providers/      résolution de flux & recherche
│   ├── soundcloud.rs   client_id scrapé, /resolve, transcodings
│   ├── mixcloud.rs     GraphQL cloudcastLookup, déchiffrement XOR
│   └── hls.rs          parsing m3u8
├── audio/          moteur de lecture
│   ├── player.rs   thread worker : résolution → décodage → sink
│   ├── source.rs   sources HTTP progressif / HLS pour symphonia
│   └── sink.rs     sortie PCM via pw-play / aplay
├── http.rs         agent ureq partagé (UA navigateur)
└── b64.rs          décodeur base64 (pour le XOR Mixcloud)
```

L'`App` ne fait aucun I/O : elle muta son état et renvoie des `Effect`
(lecture, recherche) que `main.rs` exécute sur le moteur. Le moteur tourne dans
un thread et publie son état (position, durée, lecture/pause) que l'UI relit à
chaque frame. Ce découpage rend toute la navigation testable sans terminal.

## Limites connues

- **DRM SoundCloud** : certains titres monétisés majors sont servis en HLS
  chiffré (Widevine/PlayReady). Ils sont indéchiffrables par tout client tiers ;
  waveline le signale et passe. La grande majorité du contenu (mixes, podcasts,
  artistes indépendants, uploads libres) reste jouable.
- **Mixcloud Select / exclusifs** : contenu restreint non récupérable.
- Ces APIs sont non officielles et peuvent évoluer ; la résolution est conçue
  pour échouer proprement plutôt que de planter.

## Roadmap

- [ ] Mode **avec compte** : OAuth + accès à tes likes / playlists / abonnements.
- [ ] File d'attente persistante et historique.
- [ ] Palette de commandes (`Ctrl-P`) et thèmes.
- [ ] HLS chiffré AES-128 (non-DRM) et préchargement gapless.

## Licence

MIT — voir [LICENSE](LICENSE).

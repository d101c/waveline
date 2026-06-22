<p align="center">
  <img src="assets/banner.svg" alt="waveline" width="640">
</p>

<p align="center">
  <b>Mixcloud &amp; SoundCloud, réunis dans ton terminal.</b><br>
  TUI cliquable, raccourcis vim, analyseur de spectre, touches média —
  un binaire Rust autonome (ni <code>mpv</code>, ni <code>yt-dlp</code>, ni Python).
</p>

<p align="center">
  <img src="https://github.com/d101c/waveline/actions/workflows/ci.yml/badge.svg" alt="CI">
  <img src="https://img.shields.io/crates/v/waveline.svg" alt="crates.io">
  <img src="https://img.shields.io/npm/v/waveline.svg" alt="npm">
  <img src="https://img.shields.io/badge/license-MIT-blue.svg" alt="MIT">
</p>

## Installer

**Disponible maintenant** (Linux x86_64/aarch64) :

```sh
cargo install --git https://github.com/d101c/waveline   # build depuis les sources
```

Ou télécharge un binaire statique prêt à l'emploi depuis les
[Releases](https://github.com/d101c/waveline/releases).

**Bientôt** (après publication sur les registres) :

```sh
npx waveline            # essai immédiat (Node ≥ 14)
cargo install waveline  # depuis crates.io
cargo binstall waveline # binaire pré-compilé
yay -S waveline-bin     # Arch (AUR)
```

Détails & étapes de publication : [`docs/PUBLISHING.md`](docs/PUBLISHING.md).

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
- **Visualiseurs intégrés, réactifs** — 3 styles cyclés avec `v` : barres de
  spectre, miroir (« waveline »), oscilloscope. FFT maison ~30 Hz, rendu ~30 fps
  en lecture, coût CPU négligeable.
- **Touches média & contrôles du bureau** — via MPRIS (D-Bus) : Play/Pause,
  Suivant, Précédent, Stop depuis les touches média du clavier, le panneau
  GNOME et l'écran de verrouillage ; titre/artiste/durée y sont affichés.
- **Avec et sans compte** — sans login : URLs publiques + recherche. Avec
  compte : saisis ton **pseudo** SoundCloud/Mixcloud (touche `c`) et tes
  **Likes / Playlists / Feed** se remplissent depuis les données publiques —
  aucun OAuth, aucun token, rien de sensible stocké.
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
| `Espace` | play / pause | | `c` | connecter tes comptes |
| `n` / `p` | suivant / précédent | | `v` | changer de visualiseur |
| `s` | stop | | `1` `2` `3` | filtre Tout / SC / MC |
| `+` / `-` | volume | | `Tab` | changer de panneau |
| `g` / `G` | haut / bas de liste | | `q` | quitter |
| | | | `?` | aide |

### Connexion à tes comptes

Appuie sur `c`, entre ton **pseudo SoundCloud** (Entrée), puis ton **pseudo
Mixcloud** (Entrée). Les pseudos sont mémorisés dans
`~/.config/waveline/config.json`. Active ensuite **Likes**, **Playlists** ou
**Feed** dans la barre latérale : tes données publiques des deux plateformes y
sont fusionnées. Aucun mot de passe ni token — uniquement des pseudos publics.

### Modes ligne de commande (debug)

```sh
waveline resolve <url>          # affiche le flux résolu d'une URL
waveline play <url> [secondes]  # joue le flux N secondes (test moteur)
waveline search <requête>       # recherche unifiée SC + MC
waveline lib <likes|playlists|feed> <pseudo_sc|-> <pseudo_mc|->
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
│   ├── sink.rs     sortie PCM via pw-play / aplay
│   └── spectrum.rs FFT radix-2 maison + bandes (analyseur)
├── config.rs       pseudos de compte (~/.config/waveline)
├── mpris.rs        serveur MPRIS (D-Bus) : touches média, contrôles bureau
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

- [x] Mode **avec compte** par pseudo public (Likes / Playlists / Feed).
- [x] Analyseur de spectre intégré.
- [x] Touches média / contrôles bureau via MPRIS (D-Bus).
- [ ] Likes *privés* SoundCloud via `oauth_token` collé (optionnel, hors CGU).
- [ ] File d'attente persistante et historique.
- [ ] Palette de commandes (`Ctrl-P`) et thèmes.
- [ ] HLS chiffré AES-128 (non-DRM) et préchargement gapless.

## Licence

MIT — voir [LICENSE](LICENSE).

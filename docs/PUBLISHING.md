# Publier waveline — guide & recommandations

waveline est publié sous le nom nu `waveline` sur crates.io et npm
(`cargo install waveline`, `npx waveline`). Chaque release = **bump de version**
(Cargo.toml, Cargo.lock, npm/package.json, packaging/aur/PKGBUILD) dans un
commit sur `master`, puis les étapes ci-dessous. Les commandes lisent la
version dans Cargo.toml pour ne jamais se tromper de numéro :

```sh
V=$(cargo metadata --no-deps --format-version 1 | jq -r '.packages[0].version')
echo "release v$V"
```

Tout l'outillage est prêt dans le dépôt. Ce qui suit liste, par ordre de
priorité, **où publier** et **la commande exacte** à lancer. Les étapes
marquées 🔑 nécessitent *tes* comptes/jetons (je ne peux pas les faire à ta
place — ce sont des actions sortantes liées à ton identité).

> ⚠️ Rappel : waveline est **Linux uniquement** pour l'instant (sortie audio
> PipeWire/ALSA). Les canaux ci-dessous ciblent donc Linux.

---

## 0. Fondation : la Release GitHub (à faire en premier)

Tout le reste (npx, binstall, AUR) tire les binaires d'une Release GitHub.

```sh
# depuis la racine du dépôt, sur master à jour
git tag "v$V"
git push origin "v$V"
```

→ Le workflow `.github/workflows/release.yml` compile les binaires Linux
statiques (musl, x86_64 + aarch64) et les attache à la Release `v$V`
(archives `waveline-<target>.tar.gz` + sommes SHA-256). Rien d'autre à faire.

Vérifie ensuite : `gh release view "v$V"` doit lister 2 archives + checksums.

---

## 1. crates.io 🔑 — `cargo install waveline`

Le canal le plus naturel pour un binaire Rust.

```sh
cargo login            # colle ton jeton depuis https://crates.io/settings/tokens
cargo publish          # depuis la racine ; métadonnées déjà prêtes dans Cargo.toml
```

Active aussi **`cargo binstall waveline`** sans effort : la section
`[package.metadata.binstall]` est déjà configurée pour pointer vers les archives
de la Release.

## 2. npm 🔑 — `npx waveline` (le one-liner demandé)

Le dossier `npm/` est un launcher minimal : il télécharge le binaire de la
Release adapté à la machine, le met en cache, puis le lance.

```sh
cd npm
# npm/package.json est déjà bumpé dans le commit de release : on vérifie
# l'alignement avec le crate au lieu de le réécrire.
test "$(node -p "require('./package.json').version")" = "$V" \
  || { echo "npm/package.json ($(node -p "require('./package.json').version")) != Cargo.toml ($V)"; exit 1; }
npm login              # ton compte npm
npm publish --access public
```

Après ça : **`npx waveline`** fonctionne pour tout le monde. (Le launcher exige
juste Node ≥ 14 ; le binaire, lui, n'a aucune dépendance Node.)

## 3. AUR (Arch) 🔑 — `yay -S waveline-bin`

Public idéal pour les TUI, très accueillant. PKGBUILD prêt dans
`packaging/aur/PKGBUILD` (paquet binaire `waveline-bin`).

```sh
# remplace les SKIP par les sha256 de la Release, puis :
makepkg --printsrcinfo > .SRCINFO
# pousse sur ssh://aur@aur.archlinux.org/waveline-bin.git (compte AUR + clé SSH)
```

## 4. Homebrew (Linux) 🔑

Le tap `d101c/homebrew-tap` existe (`brew install d101c/tap/waveline`, cf.
README). À chaque release, mettre à jour la formule : URL de l'archive
`v$V` x86_64 (et aarch64 si fournie) + `sha256` publiés avec la Release.

---

## Annuaires & vitrines (soumissions — accueillent volontiers les projets indés)

Ces endroits **acceptent les apps « vibe-codées »**/indépendantes via une simple
soumission ou PR. À faire une fois la Release + crates.io/npm en ligne.

| Où | Type | Comment | Pourquoi c'est pertinent |
|---|---|---|---|
| **Terminal Trove** (terminaltrove.com) | annuaire d'outils TUI/CLI | formulaire « Submit a tool » | LA vitrine des outils de terminal, très ouverte |
| **awesome-ratatui** (ratatui/awesome-ratatui) | liste GitHub | PR (section Apps) | waveline est *fait avec* ratatui → fit parfait |
| **awesome-tuis** (rothgar/awesome-tuis) | liste GitHub | PR (section Audio/Music) | référence des TUI |
| **awesome-rust** (rust-unofficial) | liste GitHub | PR (Audio / Applications) | grande visibilité (barre de qualité haute) |
| **Product Hunt** | lancement produit | « New product » + icône `assets/icon.svg` + texte ci-dessous | accueille les projets indés/IA |
| **Show HN** (news.ycombinator.com) | post | « Show HN: waveline – … » | public dev, parfait pour un TUI |
| **Lobsters** (lobste.rs) | post | tag `rust`, `audio` | communauté technique |
| **Reddit** | posts | r/rust, r/commandline, r/unixporn (screenshot) | forte traction pour les TUI |

Les **textes prêts à coller** (taglines, descriptions courtes/longues, Show HN,
Product Hunt) sont dans [`docs/launch-copy.md`](launch-copy.md).

---

## Ordre recommandé

1. Tag → Release GitHub (binaires).
2. `cargo publish` + `npm publish` (les deux one-liners deviennent vrais).
3. PR awesome-ratatui + awesome-tuis (rapide, fort signal).
4. Soumission Terminal Trove.
5. Show HN / Reddit avec un GIF de démo (le visualiseur en mouvement fait son
   effet) + lien `npx waveline`.
6. Product Hunt si tu veux un lancement plus formel (icône + copy fournis).

> Astuce démo : `vhs`/`asciinema` pour un GIF, ou la touche `v` qui cycle les
> visualiseurs pendant un mix — c'est l'accroche visuelle.

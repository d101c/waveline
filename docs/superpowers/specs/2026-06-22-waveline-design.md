# waveline — conception

*Statut : 0.2.0 — cœur jouable, comptes publics, file d'attente et historique
persistés, aide intégrée. Document de référence tenu à jour.*

## Objectif

Une application **TUI** unique pour gérer et écouter ses lectures **Mixcloud**
et **SoundCloud** depuis la console : fluide, cliquable, raccourcis sérieux.
Contraintes du commanditaire : **le plus autonome possible, le moins de
dépendances possibles**, utilisable **avec et sans compte**.

## Décisions structurantes

1. **Rust, binaire unique.** Maximise l'autonomie ; pas de runtime tiers.
2. **Décodage audio 100 % Rust** (`symphonia`) plutôt que `mpv`/`ffmpeg`
   externes. Sortie PCM vers `pw-play`/`aplay` déjà présents — évite de lier
   `libasound`/`cpal` à la compilation (headers dev absents sur la cible, pas de
   sudo). Un backend `cpal` reste ajoutable derrière un feature flag.
3. **Résolution de flux maison** (pas de `yt-dlp`) :
   - SoundCloud : `client_id` scrapé + caché → `api-v2/resolve` → choix de
     transcoding (progressive mp3 > hls) → URL signée (`track_authorization`).
   - Mixcloud : GraphQL `cloudcastLookup` → `streamInfo` déchiffré
     (**base64 puis XOR** avec une clé en clair).
4. **HTTP pur-Rust** (`ureq` + rustls), pas de `tokio`, pas d'OpenSSL système.
5. **Sans dépendance superflue** : base64 et parsing m3u8/scraping écrits à la
   main. Dépendances retenues : `ratatui`, `crossterm`, `ureq`, `serde(_json)`,
   `symphonia`, `dirs`.

## Architecture

Séparation stricte état / rendu / I/O :

- **`app`** — état pur, sans I/O, entièrement testable. `apply(Action) ->
  Option<Effect>` est l'**unique** point d'entrée (clavier, souris, MPRIS).
  Les effets (`Play`, `Toggle`, `Stop`, `SetVolume`, `Seek`, `Fetch`,
  `SaveConfig`) sont exécutés à l'extérieur. Le temps est **injecté** une fois
  par frame (`tick(now)`) : accélération du saut et spinner sont déterministes
  en test. Une liste par section ; la file et l'historique en sont deux,
  éditables, dont les mutations lèvent un indicateur « état modifié » relevé
  par la boucle (une mutation peut ainsi accompagner un effet audio).
- **`keymap`** — table unique touches → `Action`, avec la description de
  chaque raccourci. Elle sert au dispatch **et** au rendu de l'aide : l'aide
  ne peut pas diverger du clavier.
- **`ui`** — rend l'état et retourne les **zones cliquables** (`Regions`) pour
  le hit-test souris ; dessine l'aide en surimpression (`Clear` + bloc).
- **`main`** — cycle terminal (avec hook de panique restaurateur), événements
  clavier/souris/collage → `Action`, exécution des `Effect`, resynchronisation
  de l'affichage, persistance.
- **`providers`** — `Track` unifié ; `resolve_url`, `search_all` et `library`
  cachent les différences SC/MC. Les deux plateformes sont interrogées **en
  parallèle** (threads scopés) et le résultat `Fetched { tracks, failures }`
  remonte l'échec d'une plateforme au lieu de le taire.
- **`audio`** — un thread worker : résolution → décodage symphonia → `Sink`.
  État partagé (atomics + mutex) lu par l'UI sans blocage.
- **`config`** / **`state`** — préférences (`~/.config/waveline/config.json`)
  séparées des données d'usage (`~/.local/share/waveline/state.json`), toutes
  deux écrites atomiquement (fichier temporaire + `rename`).

### Requêtes asynchrones sans runtime

Une requête réseau reçoit un **numéro de génération** (`Effect::Fetch { id }`).
Le thread qui l'exécute renvoie `(id, Fetched)` sur un canal ; `App::deliver`
ignore tout `id` qui n'est plus celui attendu. Une réponse en retard ne peut
donc jamais écraser une liste plus récente — sans annulation de thread ni
machinerie async.

### File d'attente et enchaînement

À la fin d'un morceau (ou sur `n`), la file a priorité ; sinon on avance depuis
le **morceau en cours** s'il est dans la liste affichée (pas depuis le curseur,
que l'utilisateur a pu déplacer). Aux bornes, rien n'est joué : le dernier
titre ne boucle pas. `p` au-delà de 3 s redémarre le morceau.

## Modèle de données

`Track { platform, id, title, artist, permalink, duration_ms }` — seul type que
l'UI manipule. Chaque provider traduit ses objets (track SC / cloudcast MC) vers
lui.

## Lecture audio

`StreamSource = { Progressive(url) | HlsSegments(urls), container }`.
Le worker ouvre la source (HTTP progressif streamé, ou segments HLS concaténés),
décode paquet par paquet, convertit en S16LE entrelacé (volume logiciel), écrit
au `Sink`. La contre-pression de `pw-play` cadence la lecture en temps réel.
Pause = coupe le `Sink` en gardant le décodeur ; reprise = ré-ouvre le `Sink`.

## Avec / sans compte

- **Sans compte (livré)** : URLs publiques (`:`) et recherche unifiée (`/`) via
  `client_id` public (SC) et API REST publique (MC).
- **Avec compte public (livré)** : pseudos SoundCloud/Mixcloud (`c`) → Likes,
  Playlists, Feed depuis les données publiques. Aucun secret stocké.
- **Avec compte privé (roadmap)** : OAuth optionnel pour les likes privés,
  derrière la même interface ; tokens chiffrés localement.

## Risques & résilience

- APIs non officielles → la résolution échoue **proprement** (messages clairs),
  invalide et re-scrape le `client_id` sur 401/403.
- **DRM** SoundCloud (HLS chiffré Widevine/PlayReady) et **Mixcloud Select** :
  détectés et signalés comme indisponibles, jamais de crash.
- Tailles de terminal dégénérées gardées (pas de panic d'indexation).

## Tests

- Unitaires purs : navigation, pagination, filtre, volume, saisie (dont
  Ctrl-U/W), file d'attente (priorité, consommation), historique
  (déduplication, plafond), enchaînement aux bornes, redémarrage sur `p`,
  requêtes périmées, bilan d'échec par plateforme, accélération du saut avec
  temps injecté, base64, m3u8, scoring transcoding, XOR, échappement GraphQL,
  parsing d'URL, entrelacement, fan-out, persistance (round-trip, écriture
  atomique), keymap (sans conflit, libellés courts).
- Rendu : `TestBackend` (zones affichées, compteur de file, zones souris,
  fenêtre d'aide en deux et une colonnes) + anti-panic petites tailles.
- Live (modes debug) : `resolve` / `play` / `search` validés sur SC et MC.

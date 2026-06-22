# waveline — conception

*Statut : MVP « cœur jouable » implémenté. Document de référence tenu à jour.*

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
  Option<Effect>`. Les effets (`Play`, `Toggle`, `Stop`, `SetVolume`, `Search`)
  sont exécutés à l'extérieur.
- **`ui`** — rend l'état et retourne les **zones cliquables** (`Regions`) pour
  le hit-test souris.
- **`main`** — cycle terminal, événements clavier/souris → `Action`, exécution
  des `Effect` sur le moteur, resynchronisation de l'affichage.
- **`providers`** — `Track` unifié ; `resolve_url` et `search_all` cachent les
  différences SC/MC derrière une interface commune.
- **`audio`** — un thread worker : résolution → décodage symphonia → `Sink`.
  État partagé (atomics + mutex) lu par l'UI sans blocage.

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
- **Avec compte (roadmap)** : OAuth optionnel pour likes/playlists/abonnements,
  derrière la même interface `Provider` ; tokens chiffrés localement.

## Risques & résilience

- APIs non officielles → la résolution échoue **proprement** (messages clairs),
  invalide et re-scrape le `client_id` sur 401/403.
- **DRM** SoundCloud (HLS chiffré Widevine/PlayReady) et **Mixcloud Select** :
  détectés et signalés comme indisponibles, jamais de crash.
- Tailles de terminal dégénérées gardées (pas de panic d'indexation).

## Tests

- Unitaires purs : navigation, filtre, volume, saisie, base64, m3u8, scoring
  transcoding, XOR, parsing d'URL, entrelacement.
- Rendu : `TestBackend` (vérifie les zones affichées) + anti-panic petites
  tailles.
- Live (modes debug) : `resolve` / `play` / `search` validés sur SC et MC.

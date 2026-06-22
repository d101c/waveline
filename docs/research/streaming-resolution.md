# waveline — Document de référence d'implémentation

TUI Rust autonome Mixcloud + SoundCloud. Réimplémentation maison de la résolution de flux (sans yt-dlp comme dépendance runtime), lecture audio native, UX souris+clavier. Document destiné à servir de base directe au code.

---

## 0. Architecture générale et crates minimales

```
waveline/
├── core/        # résolveurs + auth + modèles (pas de TUI)
│   ├── soundcloud/   (client_id, resolve, transcoding, hls)
│   ├── mixcloud/     (graphql, xor, streaminfo)
│   ├── auth/         (stores token, oauth optionnel)
│   └── model.rs      (Track, StreamUrl, Provider, Playlist…)
├── audio/       # décodage + pipeline HLS + sink
└── tui/         # ratatui + routing souris
```

Crates (garder minimal) :

| Besoin | Crate | Notes |
|---|---|---|
| HTTP async + TLS | `reqwest` (features `rustls-tls`, `json`, `stream`, `gzip`) | un seul client réutilisé |
| Runtime async | `tokio` (features `rt-multi-thread`, `macros`, `sync`, `fs`, `process`) | |
| JSON | `serde`, `serde_json` | dérive sur les modèles |
| Décodage scraping | `regex` | client_id, scripts |
| Base64 (Mixcloud) | `base64` | `STANDARD.decode` |
| Décodage audio | `symphonia` (features par codec : `mp3`, `aac`, `isomp4`, `ogg`/`vorbis`; **pas opus natif** — voir §5) | format probing |
| Resampling/conversion | `rubato` (optionnel, si SR ≠ sink) | sinon laisser au sink |
| TUI | `ratatui` + `crossterm` (feature `event-stream`, mouse capture) | |
| Fuzzy (palette/help) | `nucleo` ou `fuzzy-matcher` | léger |
| Erreurs | `anyhow` (app) + `thiserror` (lib core) | |
| Cache disque | `directories` (paths XDG) + `serde_json` | client_id, tokens |
| Impersonation TLS (Mixcloud) | voir §2.5 — `reqwest` + headers d'abord, sinon `rquest`/`curl_cffi`-like | décision à valider en live |

**Règle d'or transversale** : un seul `reqwest::Client` partagé, User-Agent navigateur réaliste constant, toutes les URLs signées résolues **juste avant lecture** (jamais cachées).

---

## 1. Résolution de flux SoundCloud

### 1.1 Modèle de données

```rust
struct ScTranscoding {
    url: String,            // PAS le flux : endpoint à signer
    preset: String,         // http_mp3_128, hls_opus_64, hls_aac_160…
    protocol: ScProtocol,   // Progressive | Hls | EncryptedHls
    mime: String,           // audio/mpeg | audio/ogg;codecs="opus" | audio/mp4;codecs="mp4a.40.2"
    quality: String,
    snipped: bool,
}
enum ScProtocol { Progressive, Hls, EncryptedHls }
```

### 1.2 Algorithme exact (5 étapes)

**Étape 1 — Obtenir/cacher le `client_id`** (voir §3.1, partagé avec l'auth).

**Étape 2 — Resolve de l'URL** (point d'entrée pour toute URL publique)
```
GET https://api-v2.soundcloud.com/resolve?url=<URL_ENCODÉE>&client_id=<CID>
```
- `URL_ENCODÉE` = percent-encoding de `https://soundcloud.com/<user>/<track>`.
- Réponse = objet track JSON complet : `id`, `title`, `duration`, `user`, `media.transcodings[]`.
- Variante si l'id numérique est connu : `GET /tracks/<id>?client_id=<CID>`.

**Étape 3 — Choisir une transcoding** (ordre de préférence)
1. `protocol == Progressive` && `mime == audio/mpeg` (preset `http_mp3_128`) → **cas le plus simple, MP3 direct**.
2. Sinon `protocol == Hls` mp3 (`hls_mp3_128`).
3. Sinon `Hls` aac (`hls_aac_160`) ou opus (`hls_opus_64`).
4. `EncryptedHls` en dernier recours (AES-128, voir §5.4).
- Filtrer/pénaliser les previews : `snipped == true` ou `/preview/` ou `/playlist/0/30/` dans l'URL → préférence basse (équivalent yt-dlp `-10`). Si seul un snipped est dispo et qu'on a un OAuth token premium, retenter avec `Authorization: OAuth`.

**Étape 4 — Résoudre l'URL signée**
```
GET <transcoding.url>?client_id=<CID>
→ { "url": "https://cf-hls-media.sndcdn.com/.../playlist.m3u8?...signature..." }
```
- Réponse JSON à un seul champ `url`. **Expire en quelques minutes.**
- Sur **401/403** : invalider le `client_id` en cache, re-scraper (§3.1), retenter **une fois**.

**Étape 5 — Lecture selon protocole**
- **Progressive** → l'URL signée est un `.mp3` complet, `Range` supporté → streaming HTTP direct vers symphonia (§5).
- **Hls** → l'URL signée est un `.m3u8` → parser playlist média, télécharger segments dans l'ordre, concaténer (§5.3). Segments souvent `.mp3`/`.ts`/`.m4s` déjà signés/publics.
- **EncryptedHls** → m3u8 avec `#EXT-X-KEY` (AES-128) → déchiffrer chaque segment (§5.4).

### 1.3 Mapping codec / conteneur

| mime | extension/conteneur | décodeur symphonia |
|---|---|---|
| `audio/mpeg` | `.mp3` | `mp3` |
| `audio/ogg; codecs="opus"` | `.opus` | **Opus = problème** (§5.1) |
| `audio/mp4; codecs="mp4a.40.2"` | `.m4a` (m4a_dash) | `isomp4` + `aac` |

Bitrate : regex `(\d+)k$` sur le preset, ou `\.(\d+)\.(?:opus|mp3)` sur l'URL.

---

## 2. Résolution de flux Mixcloud

### 2.1 Parse de l'URL
`https://www.mixcloud.com/{username}/{slug}/` → extraire `username` et `slug`.

### 2.2 Métadonnées (optionnel, public, sans auth)
```
GET https://api.mixcloud.com/{username}/{slug}/
```
→ JSON `{name, url, audio_length, pictures, tags, play_count, user{…}}`. **Ne contient PAS le stream** — sert uniquement à enrichir l'affichage.

### 2.3 Récupérer le stream via GraphQL
```
POST https://app.mixcloud.com/graphql
Content-Type: application/json
```
Body (query inline, pas de variables séparées) :
```json
{"query":"{cloudcast(lookup:{username:\"USERNAME\",slug:\"SLUG\"}){name isExclusive restrictedReason streamInfo{url hlsUrl dashUrl}}}"}
```
- `cloudcast.streamInfo` expose 3 candidats : `url` (HTTP progressif), `hlsUrl` (m3u8), `dashUrl` (mpd). Au moins un présent selon le type.
- Si `restrictedReason != null` (Select/exclusif/DRM) → **non récupérable**, afficher message et passer.

### 2.4 Déchiffrement XOR (constante en clair)

Chaque champ de `streamInfo` est **base64 PUIS XOR** avec clé ASCII cyclique :
```
IFYOUWANTTHEARTISTSTOGETPAIDDONOTDOWNLOADFROMMIXCLOUD   (53 octets)
```
XOR symétrique. Implémentation Rust :
```rust
const MC_KEY: &[u8] = b"IFYOUWANTTHEARTISTSTOGETPAIDDONOTDOWNLOADFROMMIXCLOUD";

fn mc_decrypt(field: &str) -> anyhow::Result<String> {
    use base64::{Engine, engine::general_purpose::STANDARD};
    let raw = STANDARD.decode(field)?;
    let out: Vec<u8> = raw.iter().zip(MC_KEY.iter().cycle())
        .map(|(b, k)| b ^ k).collect();
    Ok(String::from_utf8(out)?)
}
```

### 2.5 Routage et impersonation
- Ordre de préférence : `url` (HTTP progressif → symphonia direct) > `hlsUrl` (§5.3) > `dashUrl` (DASH, plus lourd — ne traiter que si nécessaire).
- **app.mixcloud.com bloque les clients non-navigateur (403/Cloudflare) en 2026.** Stratégie progressive :
  1. D'abord `reqwest` avec headers complets : `User-Agent` Chrome récent, `Origin: https://www.mixcloud.com`, `Referer: https://www.mixcloud.com/`, `Content-Type: application/json`.
  2. Si 403 persistant → empreinte TLS navigateur nécessaire : crate type `rquest` (fork reqwest avec impersonation Chrome) ou binder `curl-impersonate`. **À valider en live avant de figer le choix** ; isoler derrière un trait `HttpBackend` pour pouvoir swap.

---

## 3. Authentification SoundCloud

### 3.1 Mode public (`client_id` scrapé) — défaut

Pipeline de scraping (à recopier de yt-dlp, c'est la référence) :
1. `GET https://soundcloud.com/` → HTML.
2. Extraire les scripts : regex `<script[^>]+src="([^"]+)"` → URLs `https://a-v2.sndcdn.com/assets/<hash>.js`.
3. **Itérer en ordre INVERSE** (client_id dans les derniers bundles), télécharger chaque asset, appliquer :
   ```
   client_id\s*:\s*"([0-9a-zA-Z]{32})"
   ```
4. Premier match → cacher sur disque (`~/.cache/waveline/sc_client_id`).
5. **Invalider + re-scraper sur 401/403** à n'importe quel appel.

Accès avec `client_id` seul : métadonnées publiques, tracks, playlists publiques, followings, streaming de morceaux publics (non-snipped).

### 3.2 Mode utilisateur (OAuth token collé)

Pas d'app dev fiable en 2026 (Artist Pro requis, octroi discrétionnaire — **ne pas en dépendre**). Deux voies :

**(a) Token web collé (pragmatique, recommandé pour waveline)**
- L'utilisateur : login sur soundcloud.com → DevTools (F12) > Network > rafraîchir → repère une requête `api-v2` → copie l'en-tête `Authorization: OAuth <token>` + le `client_id` de l'URL.
- waveline stocke `oauth_token` + `client_id` dans le store (§4.4 format commun).
- Toutes les requêtes /me et likes ajoutent l'en-tête **`Authorization: OAuth <token>`** (PAS `Bearer`).
- Le token de session web **expire** et n'a pas de refresh propre → prévoir re-collage manuel + détection 401 → invite utilisateur.

**(b) OAuth 2.1 officiel (si l'utilisateur a des credentials d'app)** — supporté en option, non requis :
```
GET  https://secure.soundcloud.com/authorize?client_id=...&redirect_uri=...&response_type=code&code_challenge=<S256>&code_challenge_method=S256&state=...
POST https://secure.soundcloud.com/oauth/token   (grant_type=authorization_code + code_verifier + client_secret)
refresh: POST .../oauth/token  grant_type=refresh_token
```
- PKCE **obligatoire** (S256). access_token ~1h, refresh_token single-use (rotation).
- Nécessite un mini serveur loopback pour capter `code` → complexe pour une CLI ; n'implémenter que si demandé.

### 3.3 Endpoints utilisateur (api-v2, token requis sauf mention)

```
resolve user id : GET /resolve?url=https://soundcloud.com/USERNAME&client_id=CID
likes           : GET /users/{id}/track_likes?client_id=CID&limit=50&offset=0&linked_partitioning=1   (Authorization: OAuth)
playlists own+liked : GET /users/{id}/playlists/liked_and_owned?client_id=CID&limit=50                (token)
playlists publiques : GET /users/{id}/playlists_without_albums?client_id=CID&limit=50&linked_partitioning=1  (client_id souvent suffit)
followings      : GET /users/{id}/followings?client_id=CID&limit=50                                   (token)
```
Pagination : suivre `next_href` (avec `linked_partitioning=1`), réajouter `client_id`.

> Note : `client_id` en query est officiellement déprécié (issue #145) mais fonctionne encore. Garder l'en-tête `Authorization` comme chemin futur.

---

## 4. Authentification Mixcloud

### 4.1 Mode public (sans aucune auth) — défaut, très favorable

Tout le listing d'un profil **public** est accessible en GET anonyme (HTTP 200 vérifié live) :
```
GET https://api.mixcloud.com/{user}/favorites/?limit=100
GET https://api.mixcloud.com/{user}/following/?limit=100
GET https://api.mixcloud.com/{user}/followers/?limit=100
GET https://api.mixcloud.com/{user}/listens/?limit=100
GET https://api.mixcloud.com/{user}/playlists/        puis /{user}/playlists/{slug}/cloudcasts/
GET https://api.mixcloud.com/{user}/cloudcasts/?limit=100
GET https://api.mixcloud.com/search/?q=QUERY&type=cloudcast|user|tag
```
- **`waveline` n'a besoin que du nom d'utilisateur Mixcloud** pour lire ses propres favoris/follows/playlists publics. C'est le chemin par défaut.
- `/{user}/` renvoie **301** → suivre les redirections (reqwest le fait par défaut, vérifier `redirect::Policy`).

### 4.2 Mode utilisateur (OAuth2) — uniquement pour /me et écritures

Nécessaire seulement pour : `GET /me/` (400 OAuthException sans token), poser/retirer favori, suivre, upload.
```
authorize : https://www.mixcloud.com/oauth/authorize?client_id=CID&redirect_uri=RU
token     : https://www.mixcloud.com/oauth/access_token?client_id=CID&redirect_uri=RU&client_secret=CS&code=CODE  → access_token
```
- Flux browser-based uniquement, **pas de scopes** (autorisation tout-ou-rien).
- Token en query `?access_token=TOKEN` (ou header `Authorization: Bearer`).
- `GET /me/favorites/?access_token=TOKEN` évite d'avoir à connaître le username.

### 4.3 Pagination Mixcloud
- Défaut = cursor temporel : réponse `{data:[…], paging:{next, previous}}`. `paging.next` contient `&until=...` URL-encodé → **réutiliser tel quel**, ne jamais reconstruire à la main. Boucler tant que `paging.next` présent.
- Alternative offset : `?limit=N&offset=M` (limit max ~100).

### 4.4 Store de credentials commun (les deux providers)

```rust
struct AuthStore {
    sc_client_id: Option<String>,     // scrapé, cacheable
    sc_oauth_token: Option<String>,   // collé par l'user, optionnel
    sc_username: Option<String>,
    mc_username: Option<String>,      // suffit pour le public
    mc_access_token: Option<String>,  // optionnel (/me + écritures)
}
```
Persisté en JSON sous `directories::ProjectDirs` → `~/.config/waveline/auth.json` (chmod 600). Tokens jamais loggés.

---

## 5. Décodage et lecture audio en Rust

### 5.1 Décodeur — symphonia

- Pipeline : source → `MediaSourceStream` → `probe` (format) → `decode` paquets → `AudioBufferRef` → conversion en `i16`/`f32` interleavé → sink.
- Features activées : `mp3`, `aac`, `isomp4`, `ogg`, `vorbis` selon codecs rencontrés.
- **Opus = point dur** : symphonia n'a pas (encore) de décodeur Opus mature/activable simplement. Stratégie : **éviter les transcodings opus** côté SoundCloud (préférer mp3/aac dans le scoring §1.2). Si opus inévitable → fallback décodage via process externe (ffmpeg si présent) ou crate `opus` (binding libopus) en feature optionnelle. Ne pas bloquer le MVP dessus.

### 5.2 Source HTTP streamée

Implémenter `symphonia::core::io::MediaSource` (= `Read + Seek`) par-dessus une réponse `reqwest` :
- **Progressive** : utiliser `Range` HTTP pour `Seek` (SoundCloud et Mixcloud `url` le supportent). Buffer glissant + cache disque temporaire optionnel.
- Alternative simple MVP : télécharger entièrement dans un `Vec<u8>`/fichier temp puis `Cursor` → moins élégant mais robuste pour des mixes longs c'est coûteux en RAM → préférer fichier temp + `File` comme `MediaSource`.

### 5.3 HLS « maison » (m3u8 mp3/aac, non chiffré)

Pas besoin d'une grosse lib HLS pour le cas SoundCloud/Mixcloud (playlist média simple, pas de variantes multi-bitrate côté audio) :
1. `GET <m3u8 signé>` → texte.
2. Parser lignes : ignorer `#`, collecter `#EXTINF:<dur>` + ligne URL suivante (relative → résoudre vs base du m3u8).
3. Télécharger segments **dans l'ordre**, les écrire séquentiellement dans un fichier temp (concat) ou les pousser dans un channel `tokio::mpsc` consommé par symphonia → lecture en streaming.
4. mp3 : concat brute OK (frames indépendantes). aac/ts : concat brute des segments puis laisser symphonia probe le conteneur ; pour `.ts`, démux nécessaire → si symphonia ne gère pas, fallback ffmpeg.
- Parser m3u8 maison suffit ; sinon crate `m3u8-rs` (légère) pour robustesse.

### 5.4 encrypted-hls (AES-128, SoundCloud)

- m3u8 contient `#EXT-X-KEY:METHOD=AES-128,URI="...",IV=...`.
- `GET` la clé (16 octets), IV = champ IV ou index de segment big-endian.
- Déchiffrer chaque segment : AES-128-CBC (`aes` + `cbc` crates) avant de l'écrire dans le flux concaténé.
- Si trop coûteux à maintenir → fallback ffmpeg (`ffmpeg -i playlist.m3u8 -c copy out`) qui gère `#EXT-X-KEY` nativement. Garder ffmpeg comme **fallback optionnel détecté à l'exécution**, jamais comme dépendance dure.

### 5.5 Sink audio — sortie système

Objectif « autonome » sans grosse dépendance audio :
- **MVP recommandé** : piper le PCM décodé vers un process système via `tokio::process` :
  - PipeWire : `pw-play --format s16 --rate 44100 --channels 2 -` (stdin).
  - ALSA : `aplay -f S16_LE -r 44100 -c 2 -` (stdin).
  - Détecter lequel est dispo (`which pw-play` / `aplay`) au démarrage.
- Avantage : zéro lien natif, gestion device/mix déléguée au serveur son.
- **Alternative intégrée** : crate `cpal` (sortie directe, contrôle latence/volume précis) ou `rodio` (plus haut niveau, mais réintroduit son propre décodeur). Pour le contrôle volume/seek fin et le visualiseur, `cpal` + ring buffer est le plus propre à terme. Architecturer derrière un trait `AudioSink { write(&[f32]); pause(); set_volume(); }` pour swap pw-play↔cpal.
- **Seek/volume** : volume = multiplication des échantillons avant écriture sink. Seek progressive = re-`Range` sur la source ; seek HLS = repositionnement au segment couvrant le timestamp.

### 5.6 Thread/async model
- Tâche réseau (fetch/résolution) async tokio.
- Tâche décodage : thread dédié (symphonia est sync/CPU) alimenté par la source, poussant le PCM dans un ring buffer `rtrb`/`ringbuf`.
- Tâche sink : draine le ring buffer vers pw-play/cpal.
- TUI : thread principal, communique via `tokio::sync::mpsc` (commandes) + `watch` (état lecture : position, durée, état).

---

## 6. Patterns UX TUI à reprendre

### 6.1 Layout (modèle gagnant, inspiré spotify-player + jellyfin-tui)
```
┌────────────┬─────────────────────────────┬──────────┐
│  Sidebar   │   Table centrale triable    │  Queue   │
│ (Library)  │  title/artist/album/dur     │  (pane   │
│ Favorites  │  tri cliquable sur header   │  dédié)  │
│ Playlists  │                             │          │
│ Following  │                             │          │
│ Search     │                             │          │
├────────────┴─────────────────────────────┴──────────┤
│ Now-playing : titre + Gauge progress (clic=seek) +   │
│ boutons ⏮ ⏯ ⏭  volume   [SC]/[MC] badge provider     │
└──────────────────────────────────────────────────────┘
```

### 6.2 Patterns à implémenter
- **Queue first-class** (cmus/jellyfin/ncspot) : vue/pane dédié distinct de la playlist. Actions : `e` enqueue, `Shift+Enter` add-to-back, `D`/`Shift-D` remove, `C-k`/`C-j` réordonner, commande `clear`. Modèle double-queue (now + upcoming).
- **Command palette `:`** (ncspot/spotify-player) : prompt type-Vim, `Esc` ferme. Chaque action = `Command` nommée bindable : `focus <screen>`, `search <q>`, `sort <key> [dir]`, `shuffle`, `repeat`, `theme`, `reload`. Fuzzy match via `nucleo`.
- **Aide searchable `?`** (spotify-player + cmus settings éditable) : overlay listant ET filtrant tous les bindings.
- **Nav vim + sauts sémantiques** (ncmpcpp) : `j/k` (+ count préfixe `3j`), `[ ]` morceau/album, `gg/G`, `C-f/C-b` page, `Tab/BackTab` cycle panes.
- **Theming par fichier** (jellyfin/spotify-player) : 20-25+ clés couleur (background, primary/secondary, border active/inactive, title, playing, progress, scrollbar, error), accepter `r,g,b` ET refs couleur terminal, **hot-reload au save** + commande `reload`/popup `T`.
- **Raccourcis = Commands nommées remappables dès le départ** (jamais codés en dur). Keymap dans le fichier config.

### 6.3 SOURIS — le différenciateur (ratatui n'a pas de routing natif)

Aucun concurrent ne traite la souris en citoyen de première classe. waveline le fait → couche de hit-testing maison :
- Maintenir un registre `Vec<ClickZone { rect: Rect, action: Command }>` reconstruit à chaque frame.
- `crossterm::event::MouseEvent` → tester `(x,y)` contre les `Rect` :
  - clic ligne table = select (double-clic = play).
  - clic header colonne = tri.
  - clic+drag sur Gauge progress = seek (le seul clic que spotify-player fait).
  - clic boutons playback bottom-bar.
  - clic onglets sidebar = focus.
  - molette = scroll liste ; drag scrollbar.
- Activer la capture souris crossterm (`EnableMouseCapture`).

---

## 7. Risques majeurs et stratégie de résilience

| Risque | Impact | Mitigation dans waveline |
|---|---|---|
| **`client_id` SoundCloud volatile** (rotation/révocation) | Panne silencieuse 401/403 | Re-scraping auto + invalidation cache sur 401/403, retry 1× ; cache disque avec timestamp. Centraliser dans un `ClientIdProvider` avec lock. |
| **URLs signées éphémères** (minutes) | Lecture échoue si cachée | Résoudre l'URL signée **juste avant lecture**, jamais persistée. Re-résoudre sur erreur de lecture / passage à la piste. |
| **Mixcloud 403/Cloudflare** (non-navigateur) | GraphQL inutilisable | Headers navigateur complets ; si insuffisant, backend TLS-impersonant (`rquest`/curl-impersonate) derrière trait `HttpBackend`. Valider en live. |
| **Endpoint GraphQL Mixcloud déjà déplacé 3×** | Casse à chaque migration | Endpoint en constante config-overridable ; surveiller le source yt-dlp comme référence vivante. |
| **Structure api-v2 SC non versionnée** (champs `transcodings`, `track_likes`) | Casse silencieuse | Parsing tolérant (`serde` avec `Option`, `#[serde(default)]`), logs explicites, tests d'intégration sur URLs réelles. |
| **Opus non décodable par symphonia** | Pistes opus muettes | Scoring qui évite opus ; fallback ffmpeg/`opus` crate optionnel. |
| **encrypted-hls AES-128** | Concat manuelle échoue | Déchiffrement AES-128-CBC maison OU fallback ffmpeg détecté à l'exécution. |
| **Rate limiting / 429** | Throttle/ban IP | Espacer les requêtes, un seul client, backoff exponentiel sur 429, pas de pagination agressive en rafale. |
| **Previews 30s (snipped)** | Lecture tronquée | Détecter `snipped`/`/preview/` → badge UI « extrait » ; tenter complet si OAuth token premium dispo. |
| **Token web SC/MC expiré** | 401 sur /me et likes | Détecter 401 → invite utilisateur à re-coller le token (pas de refresh propre). |
| **Légal / ToS** | Blocage compte/IP | Usage lecteur personnel, pas de ré-hébergement ; ne pas télécharger massivement ; documenter clairement à l'utilisateur. |

### Principes de résilience structurants
1. **Tout résolveur derrière un trait** (`StreamResolver`, `HttpBackend`, `AudioSink`) → composants fragiles isolés et swappables.
2. **yt-dlp reste la référence vivante** pour SoundCloud et Mixcloud : copier la logique (regex client_id, query GraphQL exacte, clé XOR, champs transcodings) et la re-synchroniser à chaque casse plutôt que d'inventer.
3. **Dégradation gracieuse** : progressive > hls > encrypted-hls > ffmpeg fallback ; jamais de crash, toujours un message UI exploitable.
4. **Aucune dépendance dure à ffmpeg ni à une app dev officielle** : les deux sont des bonus détectés à l'exécution. Le chemin par défaut (client_id scrapé SC + username public MC + pw-play/aplay) fonctionne seul.

---

### Récapitulatif des chemins « par défaut » (MVP sans aucune config utilisateur)
- **SoundCloud** : scrape `client_id` → `/resolve` → transcoding progressive mp3 → URL signée → symphonia mp3 → pw-play.
- **Mixcloud** : username public → `/{user}/favorites` etc. → pour lecture, GraphQL `streamInfo.url` → XOR-decrypt → symphonia → pw-play.
- **Auth utilisateur** = optionnelle (token collé), uniquement pour /me et likes privés.
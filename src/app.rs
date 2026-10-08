//! État de l'application et logique de mise à jour (pure, testable).
//!
//! `App` ne fait AUCUN I/O : il mute son état et renvoie d'éventuels
//! [`Effect`] (lecture audio, requête réseau, sauvegarde) que la boucle
//! d'événements exécute. Le temps lui est injecté une fois par frame via
//! [`App::tick`], ce qui garde l'accélération du saut, l'indicateur d'attente
//! et les tests déterministes.
//!
//! Chaque section de la barre latérale possède sa propre liste : revenir de la
//! recherche vers les Likes ne recharge rien. La **file d'attente** et
//! l'**historique** sont deux de ces listes, éditables (`a`, `x`, `X`) et
//! persistées par la boucle principale (cf. [`crate::state`]).

use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::i18n::{self, Lang};
use crate::model::{Platform, Track};
use crate::providers::{Fetched, LibrarySection};

/// Sections de la barre latérale gauche.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Section {
    Likes,
    Playlists,
    Feed,
    Search,
    History,
    Queue,
}

impl Section {
    pub const ALL: [Section; 6] = [
        Section::Likes,
        Section::Playlists,
        Section::Feed,
        Section::Search,
        Section::History,
        Section::Queue,
    ];

    /// Libellé avec pictogramme, pour la barre latérale.
    pub fn label(self, lang: Lang) -> &'static str {
        match (self, lang) {
            (Section::Likes, _) => "♥  Likes",
            (Section::Playlists, _) => "☰  Playlists",
            (Section::Feed, _) => "◎  Feed",
            (Section::Search, Lang::En) => "⌕  Search",
            (Section::Search, Lang::Fr) => "⌕  Recherche",
            (Section::History, Lang::En) => "⧗  History",
            (Section::History, Lang::Fr) => "⧗  Historique",
            (Section::Queue, Lang::En) => "▤  Queue",
            (Section::Queue, Lang::Fr) => "▤  File",
        }
    }

    /// Nom nu (sans pictogramme), pour les titres et messages.
    pub fn name(self, lang: Lang) -> &'static str {
        match (self, lang) {
            (Section::Likes, _) => "Likes",
            (Section::Playlists, _) => "Playlists",
            (Section::Feed, _) => "Feed",
            (Section::Search, Lang::En) => "Search",
            (Section::Search, Lang::Fr) => "Recherche",
            (Section::History, Lang::En) => "History",
            (Section::History, Lang::Fr) => "Historique",
            (Section::Queue, Lang::En) => "Queue",
            (Section::Queue, Lang::Fr) => "File",
        }
    }

    /// Section de bibliothèque associée (chargée depuis les comptes), s'il y en a une.
    fn library(self) -> Option<LibrarySection> {
        match self {
            Section::Likes => Some(LibrarySection::Likes),
            Section::Playlists => Some(LibrarySection::Playlists),
            Section::Feed => Some(LibrarySection::Feed),
            Section::Search | Section::History | Section::Queue => None,
        }
    }

    /// Les listes que l'utilisateur peut éditer à la main (`x`, `X`).
    pub fn is_editable(self) -> bool {
        matches!(self, Section::Queue | Section::History)
    }

    fn index(self) -> usize {
        Self::ALL.iter().position(|s| *s == self).unwrap_or(0)
    }
}

/// Filtre par plateforme appliqué à la liste centrale.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Filter {
    All,
    Only(Platform),
}

impl Filter {
    pub fn label(self, lang: Lang) -> String {
        match self {
            Filter::All => match lang {
                Lang::En => "All".to_string(),
                Lang::Fr => "Tout".to_string(),
            },
            Filter::Only(p) => p.to_string(),
        }
    }

    fn keep(self, t: &Track) -> bool {
        match self {
            Filter::All => true,
            Filter::Only(p) => t.platform == p,
        }
    }
}

/// Quel panneau a le focus clavier.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    Sidebar,
    List,
}

/// Mode de saisie courant.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Input {
    Normal,
    /// Saisie d'une URL à lire (déclenchée par `:`).
    Command(String),
    /// Saisie d'une requête de recherche (déclenchée par `/`).
    Search(String),
    /// Saisie du pseudo SoundCloud (connexion compte).
    ConnectSoundCloud(String),
    /// Saisie du pseudo Mixcloud (connexion compte).
    ConnectMixcloud(String),
}

impl Input {
    /// Tampon de saisie mutable, quel que soit le mode actif.
    fn buffer_mut(&mut self) -> Option<&mut String> {
        match self {
            Input::Command(s)
            | Input::Search(s)
            | Input::ConnectSoundCloud(s)
            | Input::ConnectMixcloud(s) => Some(s),
            Input::Normal => None,
        }
    }
}

/// État de lecture affiché (resynchronisé depuis le moteur audio).
#[derive(Debug, Clone, Default)]
pub struct Playback {
    pub current: Option<Track>,
    pub playing: bool,
    pub loading: bool,
    pub position_ms: u64,
    pub duration_ms: u64,
    pub volume: u8,
    /// Vrai si le flux courant supporte le saut (progressif seekable). Faux pour
    /// le HLS : dans ce cas les raccourcis de saut affichent un message et
    /// n'agissent pas.
    pub seekable: bool,
    /// Amplitudes du spectre par bande (0..1), pour l'analyseur visuel.
    pub spectrum: Vec<f32>,
    /// Échantillons de forme d'onde (~[-1,1]) pour le mode oscilloscope.
    pub waveform: Vec<f32>,
}

/// Pas de saut successifs (ms) : 10s → 30s → 1min → 5min → 10min.
const SEEK_STEPS_MS: [i64; 5] = [10_000, 30_000, 60_000, 300_000, 600_000];
/// Nombre d'appuis consécutifs par palier avant de passer au pas suivant.
const SEEK_TIER_PRESSES: u32 = 3;
/// Au-delà de ce délai entre deux appuis, l'accélération repart du plus petit pas.
const SEEK_RESET_AFTER: Duration = Duration::from_millis(1500);
/// Au-delà de cette position, « précédent » redémarre le morceau au lieu de
/// reculer d'une piste (convention des lecteurs classiques).
const PREV_RESTART_AFTER_MS: u64 = 3_000;
/// Taille maximale de l'historique conservé.
pub const HISTORY_CAP: usize = 200;

/// Accélération du saut par répétition rapide.
///
/// Chaque palier dure [`SEEK_TIER_PRESSES`] appuis ; on monte ensuite d'un cran
/// dans [`SEEK_STEPS_MS`]. Le compteur repart de zéro après une pause
/// (> [`SEEK_RESET_AFTER`]) ou un changement de sens. Le temps est injecté
/// (`now`) pour garder la logique testable et hors de la couche I/O.
#[derive(Debug, Default)]
pub struct SeekAccel {
    last: Option<Instant>,
    /// Sens du dernier appui (+1 avant, -1 arrière, 0 au repos).
    dir: i8,
    /// Appuis consécutifs dans le sens courant.
    count: u32,
}

impl SeekAccel {
    pub fn new() -> Self {
        Self::default()
    }

    /// Enregistre un appui de saut dans le sens `dir` (+1 avant, -1 arrière) à
    /// l'instant `now` et renvoie le delta signé en millisecondes à appliquer.
    pub fn step(&mut self, now: Instant, dir: i8) -> i64 {
        let restart = match self.last {
            Some(prev) => now.duration_since(prev) > SEEK_RESET_AFTER || dir != self.dir,
            None => true,
        };
        if restart {
            self.count = 0;
            self.dir = dir;
        }
        self.count += 1;
        self.last = Some(now);
        let tier = ((self.count - 1) / SEEK_TIER_PRESSES).min(SEEK_STEPS_MS.len() as u32 - 1);
        SEEK_STEPS_MS[tier as usize] * dir as i64
    }
}

/// Style d'analyseur visuel (cyclé avec `v`), mémorisé dans la config.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum VizMode {
    /// Barres de spectre ancrées en bas.
    #[default]
    Bars,
    /// Spectre symétrique autour d'une ligne centrale (« waveline »).
    Mirror,
    /// Oscilloscope temporel (forme d'onde brute), très réactif.
    Scope,
}

impl VizMode {
    pub const ALL: [VizMode; 3] = [VizMode::Bars, VizMode::Mirror, VizMode::Scope];

    pub fn label(self, lang: Lang) -> &'static str {
        match (self, lang) {
            (VizMode::Bars, Lang::En) => "bars",
            (VizMode::Bars, Lang::Fr) => "barres",
            (VizMode::Mirror, Lang::En) => "mirror",
            (VizMode::Mirror, Lang::Fr) => "miroir",
            (VizMode::Scope, Lang::En) => "scope",
            (VizMode::Scope, Lang::Fr) => "oscilloscope",
        }
    }

    pub fn next(self) -> VizMode {
        let i = Self::ALL.iter().position(|m| *m == self).unwrap_or(0);
        Self::ALL[(i + 1) % Self::ALL.len()]
    }
}

/// Requête réseau demandée par l'app, exécutée hors de l'état par la boucle.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FetchRequest {
    /// Recherche unifiée SoundCloud + Mixcloud.
    Search(String),
    /// Section de bibliothèque des comptes configurés.
    Library(LibrarySection),
}

/// Effets de bord exécutés par la boucle principale (audio, réseau, disque).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Effect {
    Play(String),
    Toggle,
    Stop,
    SetVolume(u8),
    /// Saut relatif dans le morceau courant (delta signé en millisecondes).
    Seek(i64),
    /// Lance une requête réseau. `id` identifie la génération : un résultat
    /// livré avec un `id` périmé est ignoré par [`App::deliver`].
    Fetch {
        id: u64,
        request: FetchRequest,
    },
    /// Persiste les préférences (comptes, langue, volume, visualiseur).
    SaveConfig,
}

/// Intentions de haut niveau, indépendantes du clavier/souris. `App::apply`
/// est l'unique point d'entrée : clavier, souris et MPRIS passent tous par là.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Up,
    Down,
    PageUp,
    PageDown,
    Top,
    Bottom,
    Activate,
    ToggleFocus,
    PlayPause,
    Stop,
    Next,
    Prev,
    /// Saut avant/arrière avec accélération sur répétition rapide.
    SeekForward,
    SeekBack,
    VolumeUp,
    VolumeDown,
    FilterAll,
    FilterSoundCloud,
    FilterMixcloud,
    /// Ajoute la sélection à la file d'attente.
    Enqueue,
    /// Retire la sélection (file ou historique).
    RemoveFromList,
    /// Vide la liste courante (file ou historique).
    ClearList,
    BeginCommand,
    BeginSearch,
    BeginConnect,
    CycleViz,
    ToggleLang,
    ToggleHelp,
    Quit,
}

/// Requête réseau en cours : section destinataire et génération.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pending {
    pub id: u64,
    pub section: Section,
    pub started: Instant,
}

/// Listes par section de bibliothèque (une par entrée de la barre latérale,
/// hors file et historique qui vivent à part).
#[derive(Debug, Default)]
struct Lists {
    likes: Vec<Track>,
    playlists: Vec<Track>,
    feed: Vec<Track>,
    search: Vec<Track>,
}

/// État global de l'application.
pub struct App {
    pub should_quit: bool,
    pub focus: Focus,
    pub section: Section,
    pub section_index: usize,
    pub filter: Filter,
    lists: Lists,
    /// File d'attente : lue en priorité à la fin d'un morceau et par `n`.
    pub queue: Vec<Track>,
    /// Historique d'écoute, le plus récent en tête, dédupliqué, plafonné.
    pub history: Vec<Track>,
    pub list_index: usize,
    pub playback: Playback,
    pub status: String,
    pub input: Input,
    /// Pseudo SoundCloud connecté (compte de l'utilisateur).
    pub sc_handle: Option<String>,
    /// Pseudo Mixcloud connecté.
    pub mc_handle: Option<String>,
    /// Style d'analyseur visuel courant.
    pub viz: VizMode,
    /// Langue de l'interface (anglais par défaut).
    pub lang: Lang,
    /// Fenêtre d'aide affichée par-dessus l'interface.
    pub show_help: bool,
    /// Requêtes réseau en cours, au plus une par section (spinner + filtrage
    /// des résultats périmés). Une recherche lancée pendant le chargement des
    /// Likes ne fait pas perdre la réponse des Likes.
    pending: Vec<Pending>,
    next_fetch_id: u64,
    /// Section depuis laquelle le morceau en cours a été lancé. Depuis la vue
    /// Historique, la liste reste stable (pas de remontée en tête), sinon
    /// « suivant » rejouerait indéfiniment les deux premières entrées.
    launched_from: Option<Section>,
    /// Curseur mémorisé par section : traverser la barre latérale ne fait pas
    /// perdre sa position dans les résultats.
    cursors: [usize; Section::ALL.len()],
    /// Instant courant, injecté par la boucle via [`App::tick`].
    now: Instant,
    seek_accel: SeekAccel,
    /// Hauteur (lignes) de la liste visible, pour `PageUp`/`PageDown`.
    viewport_rows: u16,
    /// File ou historique modifiés depuis la dernière sauvegarde. La boucle
    /// principale le relève via [`App::take_state_dirty`] et persiste : une
    /// mutation peut ainsi accompagner un effet audio sans multiplier les
    /// valeurs de retour.
    state_dirty: bool,
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}

impl App {
    pub fn new() -> Self {
        App {
            should_quit: false,
            focus: Focus::List,
            section: Section::Search,
            section_index: Section::Search.index(),
            filter: Filter::All,
            lists: Lists::default(),
            queue: Vec::new(),
            history: Vec::new(),
            list_index: 0,
            playback: Playback {
                volume: crate::config::DEFAULT_VOLUME,
                ..Default::default()
            },
            status: i18n::welcome(Lang::En),
            input: Input::Normal,
            sc_handle: None,
            mc_handle: None,
            viz: VizMode::Bars,
            lang: Lang::En,
            show_help: false,
            pending: Vec::new(),
            next_fetch_id: 0,
            launched_from: None,
            cursors: [0; Section::ALL.len()],
            now: Instant::now(),
            seek_accel: SeekAccel::new(),
            viewport_rows: 20,
            state_dirty: false,
        }
    }

    // --- Cycle de vie ---------------------------------------------------------

    /// Injecte l'instant courant (une fois par frame).
    pub fn tick(&mut self, now: Instant) {
        self.now = now;
    }

    /// Instant courant tel que vu par l'app.
    pub fn now(&self) -> Instant {
        self.now
    }

    /// Informe l'app de la hauteur de la liste affichée (pour la pagination).
    pub fn set_viewport_rows(&mut self, rows: u16) {
        self.viewport_rows = rows.max(1);
    }

    /// Relève (et réarme) l'indicateur « file/historique modifiés ».
    pub fn take_state_dirty(&mut self) -> bool {
        std::mem::take(&mut self.state_dirty)
    }

    /// Applique la langue chargée depuis la config au démarrage (avant la
    /// première frame) et rafraîchit le message d'accueil en conséquence.
    pub fn set_startup_lang(&mut self, lang: Lang) {
        self.lang = lang;
        self.status = i18n::welcome(lang);
    }

    /// Restaure la file et l'historique persistés, puis choisit la vue de
    /// départ : l'historique s'il existe (reprendre où l'on en était), sinon
    /// la recherche garnie de `starter` (premier contact sans compte).
    pub fn restore(&mut self, queue: Vec<Track>, history: Vec<Track>, starter: Vec<Track>) {
        self.queue = queue;
        self.history = history;
        self.history.truncate(HISTORY_CAP);
        self.lists.search = starter;
        let start = if self.history.is_empty() {
            Section::Search
        } else {
            Section::History
        };
        self.select_section(start);
    }

    /// Indique si au moins un compte est connecté.
    pub fn has_account(&self) -> bool {
        self.sc_handle.is_some() || self.mc_handle.is_some()
    }

    // --- Listes -------------------------------------------------------------------

    /// La liste de la section affichée.
    pub fn tracks(&self) -> &[Track] {
        self.tracks_of(self.section)
    }

    /// La liste d'une section donnée.
    pub fn tracks_of(&self, section: Section) -> &[Track] {
        match section {
            Section::Likes => &self.lists.likes,
            Section::Playlists => &self.lists.playlists,
            Section::Feed => &self.lists.feed,
            Section::Search => &self.lists.search,
            Section::History => &self.history,
            Section::Queue => &self.queue,
        }
    }

    fn tracks_of_mut(&mut self, section: Section) -> &mut Vec<Track> {
        match section {
            Section::Likes => &mut self.lists.likes,
            Section::Playlists => &mut self.lists.playlists,
            Section::Feed => &mut self.lists.feed,
            Section::Search => &mut self.lists.search,
            Section::History => &mut self.history,
            Section::Queue => &mut self.queue,
        }
    }

    /// Remplace la liste d'une section (sans changer la vue).
    pub fn set_tracks(&mut self, section: Section, tracks: Vec<Track>) {
        *self.tracks_of_mut(section) = tracks;
        self.clamp_cursor();
    }

    /// Indices des morceaux visibles après application du filtre courant.
    pub fn visible_indices(&self) -> Vec<usize> {
        self.tracks()
            .iter()
            .enumerate()
            .filter(|(_, t)| self.filter.keep(t))
            .map(|(i, _)| i)
            .collect()
    }

    /// Le morceau actuellement surligné dans la liste.
    pub fn selected_track(&self) -> Option<&Track> {
        let vis = self.visible_indices();
        vis.get(self.list_index).and_then(|&i| self.tracks().get(i))
    }

    /// Ramène le curseur dans les bornes de la liste visible (après toute
    /// mutation de liste ou changement de filtre/section).
    fn clamp_cursor(&mut self) {
        let n = self.visible_indices().len();
        if n == 0 {
            self.list_index = 0;
        } else if self.list_index >= n {
            self.list_index = n - 1;
        }
    }

    /// Sélectionne une section dans la barre latérale (sans la charger), en
    /// mémorisant le curseur de la section quittée et en restaurant celui de
    /// la nouvelle (borné : les listes ont pu changer entre-temps).
    pub fn select_section(&mut self, section: Section) {
        self.cursors[self.section.index()] = self.list_index;
        self.section = section;
        self.section_index = section.index();
        self.list_index = self.cursors[section.index()];
        self.clamp_cursor();
    }

    /// Vrai si une requête réseau est en cours.
    pub fn is_busy(&self) -> bool {
        !self.pending.is_empty()
    }

    /// La requête en cours la plus récente (pour le spinner).
    pub fn latest_pending(&self) -> Option<&Pending> {
        self.pending.iter().max_by_key(|p| p.id)
    }

    // --- Dispatch ---------------------------------------------------------------

    /// Referme l'aide si elle est ouverte ; renvoie vrai si c'était le cas.
    /// L'aide est modale pour le clavier et la souris (la touche ou le clic
    /// qui la ferme est consommé), mais pas pour les commandes média externes.
    pub fn dismiss_help(&mut self) -> bool {
        std::mem::take(&mut self.show_help)
    }

    /// Applique une intention et renvoie l'effet éventuel.
    pub fn apply(&mut self, action: Action) -> Option<Effect> {
        match action {
            Action::Quit => {
                self.should_quit = true;
                None
            }
            Action::ToggleHelp => {
                self.show_help = true;
                None
            }
            Action::ToggleFocus => {
                self.focus = match self.focus {
                    Focus::Sidebar => Focus::List,
                    Focus::List => Focus::Sidebar,
                };
                None
            }
            Action::Up => {
                self.move_cursor(-1);
                None
            }
            Action::Down => {
                self.move_cursor(1);
                None
            }
            Action::PageUp => {
                self.move_cursor(-self.page_step());
                None
            }
            Action::PageDown => {
                self.move_cursor(self.page_step());
                None
            }
            Action::Top => {
                self.move_to_edge(true);
                None
            }
            Action::Bottom => {
                self.move_to_edge(false);
                None
            }
            Action::Activate => self.activate(),
            Action::PlayPause => {
                // Rien en cours : joue la sélection. Sinon bascule.
                if self.playback.current.is_none() && !self.playback.loading {
                    self.play_selection()
                } else {
                    Some(Effect::Toggle)
                }
            }
            Action::Stop => {
                self.status = i18n::playback_stopped(self.lang);
                Some(Effect::Stop)
            }
            Action::Next => self.skip(1),
            Action::Prev => self.prev(),
            Action::SeekForward => {
                let delta = self.seek_accel.step(self.now, 1);
                self.seek(delta)
            }
            Action::SeekBack => {
                let delta = self.seek_accel.step(self.now, -1);
                self.seek(delta)
            }
            Action::VolumeUp => Some(self.bump_volume(5)),
            Action::VolumeDown => Some(self.bump_volume(-5)),
            Action::FilterAll => {
                self.set_filter(Filter::All);
                None
            }
            Action::FilterSoundCloud => {
                self.set_filter(Filter::Only(Platform::SoundCloud));
                None
            }
            Action::FilterMixcloud => {
                self.set_filter(Filter::Only(Platform::Mixcloud));
                None
            }
            Action::Enqueue => self.enqueue(),
            Action::RemoveFromList => self.remove_from_list(),
            Action::ClearList => self.clear_list(),
            Action::BeginCommand => {
                self.begin_command();
                None
            }
            Action::BeginSearch => {
                self.begin_search();
                None
            }
            Action::BeginConnect => {
                self.begin_connect();
                None
            }
            Action::CycleViz => {
                self.cycle_viz();
                None
            }
            Action::ToggleLang => {
                self.lang = self.lang.toggle();
                self.status = i18n::lang_changed(self.lang);
                Some(Effect::SaveConfig)
            }
        }
    }

    /// Passe au style de visualiseur suivant.
    pub fn cycle_viz(&mut self) {
        self.viz = self.viz.next();
        self.status = i18n::viz_changed(self.lang, self.viz.label(self.lang));
    }

    fn page_step(&self) -> i32 {
        (self.viewport_rows as i32 / 2).max(1)
    }

    fn move_cursor(&mut self, delta: i32) {
        match self.focus {
            Focus::Sidebar => {
                let n = Section::ALL.len() as i32;
                let i = (self.section_index as i32 + delta).rem_euclid(n) as usize;
                self.select_section(Section::ALL[i]);
            }
            Focus::List => {
                let n = self.visible_indices().len() as i32;
                if n == 0 {
                    return;
                }
                let i = (self.list_index as i32 + delta).clamp(0, n - 1) as usize;
                self.list_index = i;
            }
        }
    }

    fn move_to_edge(&mut self, top: bool) {
        match self.focus {
            Focus::Sidebar => {
                let i = if top { 0 } else { Section::ALL.len() - 1 };
                self.select_section(Section::ALL[i]);
            }
            Focus::List => {
                let n = self.visible_indices().len();
                self.list_index = if top || n == 0 { 0 } else { n - 1 };
            }
        }
    }

    // --- Lecture ------------------------------------------------------------------

    /// Demande la lecture d'un morceau précis, lancé depuis la section affichée.
    fn play_track(&mut self, t: &Track) -> Effect {
        self.launched_from = Some(self.section);
        self.status = i18n::loading(self.lang);
        Effect::Play(t.permalink.clone())
    }

    /// Retire de la file l'entrée d'index `i` (jouer depuis la file la consomme).
    fn consume_queued(&mut self, i: usize) {
        if i < self.queue.len() {
            self.queue.remove(i);
            self.state_dirty = true;
            self.clamp_cursor();
        }
    }

    /// Joue le morceau surligné ; depuis la vue File, le consomme — quel que
    /// soit le geste (Entrée, clic, espace).
    fn play_selection(&mut self) -> Option<Effect> {
        let t = self.selected_track().cloned()?;
        if self.section == Section::Queue {
            if let Some(&i) = self.visible_indices().get(self.list_index) {
                self.consume_queued(i);
            }
        }
        Some(self.play_track(&t))
    }

    fn activate(&mut self) -> Option<Effect> {
        match self.focus {
            Focus::Sidebar => self.open_section(),
            Focus::List => self.play_selection(),
        }
    }

    /// Ouvre la section sélectionnée dans la sidebar.
    fn open_section(&mut self) -> Option<Effect> {
        self.focus = Focus::List;
        match self.section {
            Section::Search => {
                // Ouvrir la section = proposer une recherche ; les résultats
                // précédents restent visibles derrière l'invite (Échap annule).
                self.begin_search();
                None
            }
            Section::History => {
                if self.history.is_empty() {
                    self.status = i18n::history_empty(self.lang);
                }
                None
            }
            Section::Queue => {
                if self.queue.is_empty() {
                    self.status = i18n::queue_empty(self.lang);
                }
                None
            }
            lib => {
                let sec = lib.library()?;
                if !self.has_account() {
                    self.status = i18n::no_account(self.lang);
                    return None;
                }
                self.status = i18n::loading_section(self.lang, lib.name(self.lang));
                Some(self.request(lib, FetchRequest::Library(sec)))
            }
        }
    }

    /// Position (dans la liste visible) du morceau en cours, s'il y figure.
    fn current_visible_pos(&self) -> Option<usize> {
        let cur = self.playback.current.as_ref()?;
        let tracks = self.tracks();
        self.visible_indices()
            .iter()
            .position(|&i| tracks[i].same_as(cur))
    }

    /// Piste suivante (`delta` > 0) ou précédente : la file d'attente a la
    /// priorité pour « suivant » ; sinon on avance depuis le morceau en cours
    /// (s'il est dans la liste affichée) ou depuis le curseur. Aux bornes,
    /// rien n'est joué — en particulier, le dernier morceau ne boucle pas.
    fn skip(&mut self, delta: i32) -> Option<Effect> {
        if delta > 0 && !self.queue.is_empty() {
            let t = self.queue.remove(0);
            self.state_dirty = true;
            self.clamp_cursor();
            self.launched_from = Some(Section::Queue);
            self.status = i18n::playing_from_queue(self.lang, &t.title);
            return Some(Effect::Play(t.permalink.clone()));
        }
        let vis = self.visible_indices();
        if vis.is_empty() {
            return None;
        }
        let anchor = self.current_visible_pos().unwrap_or(self.list_index) as i32;
        let target = anchor + delta;
        if target < 0 {
            self.status = i18n::start_of_list(self.lang);
            return None;
        }
        if target >= vis.len() as i32 {
            self.status = i18n::end_of_list(self.lang);
            return None;
        }
        self.list_index = target as usize;
        let t = self.tracks()[vis[target as usize]].clone();
        if self.section == Section::Queue {
            self.consume_queued(vis[target as usize]);
        }
        Some(self.play_track(&t))
    }

    /// « Précédent » : au-delà de quelques secondes, redémarre le morceau ;
    /// sinon recule d'une piste.
    fn prev(&mut self) -> Option<Effect> {
        if let Some(cur) = self.playback.current.clone() {
            if self.playback.position_ms > PREV_RESTART_AFTER_MS {
                self.status = i18n::restarted(self.lang);
                if self.playback.seekable {
                    let back = self.playback.position_ms as i64;
                    self.playback.position_ms = 0;
                    return Some(Effect::Seek(-back));
                }
                return Some(Effect::Play(cur.permalink));
            }
        }
        self.skip(-1)
    }

    /// Saut relatif dans le morceau courant. `delta_ms` est signé (négatif =
    /// arrière). Renvoie `None` (sans effet) si rien n'est en lecture ou si le
    /// flux n'est pas seekable ; sinon met à jour la position affichée de façon
    /// optimiste (le moteur confirme au tick suivant) et renvoie l'effet.
    pub fn seek(&mut self, delta_ms: i64) -> Option<Effect> {
        if self.playback.current.is_none() || self.playback.duration_ms == 0 {
            return None;
        }
        if !self.playback.seekable {
            self.status = i18n::seek_unavailable(self.lang);
            return None;
        }
        let dur = self.playback.duration_ms as i64;
        let target = (self.playback.position_ms as i64 + delta_ms).clamp(0, dur);
        self.playback.position_ms = target as u64;
        self.status = i18n::seek(self.lang, delta_ms);
        Some(Effect::Seek(delta_ms))
    }

    /// Saut absolu (clic sur la barre de progression) : exprimé comme un delta
    /// pour le moteur.
    pub fn seek_to(&mut self, target_ms: u64) -> Option<Effect> {
        let target = target_ms.min(self.playback.duration_ms) as i64;
        let delta = target - self.playback.position_ms as i64;
        self.seek(delta)
    }

    /// Le moteur a changé de morceau : met à jour l'affichage et, pour un
    /// nouveau morceau, l'inscrit en tête de l'historique (dédupliqué,
    /// plafonné) en marquant l'état à sauvegarder.
    pub fn sync_current(&mut self, now_playing: Option<Track>) {
        let changed = match (&self.playback.current, &now_playing) {
            (Some(a), Some(b)) => !a.same_as(b),
            (None, Some(_)) => true,
            _ => false,
        };
        self.playback.current = now_playing;
        if !changed {
            return;
        }
        let Some(t) = self.playback.current.clone() else {
            return;
        };
        // Lancé depuis la vue Historique et déjà présent : on laisse la liste
        // en place (elle ne bouge pas sous le curseur et « suivant » descend
        // vers les entrées plus anciennes au lieu de rejouer les deux premières).
        if self.launched_from == Some(Section::History)
            && self.history.iter().any(|h| h.same_as(&t))
        {
            return;
        }
        self.history.retain(|h| !h.same_as(&t));
        self.history.insert(0, t);
        self.history.truncate(HISTORY_CAP);
        self.state_dirty = true;
        if self.section == Section::History {
            self.clamp_cursor();
        }
    }

    fn bump_volume(&mut self, delta: i32) -> Effect {
        let v = (self.playback.volume as i32 + delta).clamp(0, 100) as u8;
        self.playback.volume = v;
        self.status = i18n::volume(self.lang, v);
        Effect::SetVolume(v)
    }

    fn set_filter(&mut self, f: Filter) {
        self.filter = f;
        self.list_index = 0;
        self.status = i18n::filter_changed(self.lang, &f.label(self.lang));
    }

    // --- File d'attente & historique ------------------------------------------

    fn enqueue(&mut self) -> Option<Effect> {
        if self.section == Section::Queue {
            self.status = i18n::already_queue(self.lang);
            return None;
        }
        let t = self.selected_track().cloned()?;
        self.queue.push(t.clone());
        self.state_dirty = true;
        self.status = i18n::queued(self.lang, &t.title, self.queue.len());
        None
    }

    fn remove_from_list(&mut self) -> Option<Effect> {
        if !self.section.is_editable() {
            self.status = i18n::edit_hint(self.lang);
            return None;
        }
        let vis = self.visible_indices();
        let &i = vis.get(self.list_index)?;
        let section = self.section;
        let removed = self.tracks_of_mut(section).remove(i);
        self.state_dirty = true;
        self.clamp_cursor();
        self.status = i18n::removed(self.lang, &removed.title);
        None
    }

    fn clear_list(&mut self) -> Option<Effect> {
        if !self.section.is_editable() {
            self.status = i18n::edit_hint(self.lang);
            return None;
        }
        let section = self.section;
        if self.tracks_of(section).is_empty() {
            return None;
        }
        self.tracks_of_mut(section).clear();
        self.state_dirty = true;
        self.list_index = 0;
        self.status = i18n::cleared(self.lang, section.name(self.lang));
        None
    }

    // --- Requêtes réseau ----------------------------------------------------------

    /// Enregistre une requête en attente pour `section` (remplaçant celle de
    /// la même section, dont la réponse devient périmée) et renvoie l'effet.
    fn request(&mut self, section: Section, request: FetchRequest) -> Effect {
        self.next_fetch_id += 1;
        let id = self.next_fetch_id;
        self.pending.retain(|p| p.section != section);
        self.pending.push(Pending {
            id,
            section,
            started: self.now,
        });
        Effect::Fetch { id, request }
    }

    /// Livre le résultat d'une requête. Un `id` qui n'est plus attendu
    /// (requête plus récente pour la même section) est ignoré : la liste ne
    /// peut pas être écrasée par une réponse en retard. La réponse de la
    /// requête la plus récente amène sa section à l'écran ; celle d'une
    /// section que l'utilisateur a quittée entre-temps est rangée sans voler
    /// la vue ni le statut.
    pub fn deliver(&mut self, id: u64, fetched: Fetched) {
        let Some(pos) = self.pending.iter().position(|p| p.id == id) else {
            return;
        };
        let p = self.pending.remove(pos);
        let n = fetched.tracks.len();
        self.set_tracks(p.section, fetched.tracks);
        let latest = p.id == self.next_fetch_id;
        if latest || p.section == self.section {
            self.select_section(p.section);
            self.focus = Focus::List;
            self.list_index = 0;
        }
        if latest {
            self.status = i18n::fetch_summary(self.lang, n, &fetched.failures);
        }
    }

    // --- Mode saisie ----------------------------------------------------------------

    pub fn begin_command(&mut self) {
        self.input = Input::Command(String::new());
    }

    pub fn begin_search(&mut self) {
        self.input = Input::Search(String::new());
    }

    /// Démarre la connexion : pseudo SoundCloud, puis Mixcloud (pré-remplis).
    pub fn begin_connect(&mut self) {
        self.input = Input::ConnectSoundCloud(self.sc_handle.clone().unwrap_or_default());
    }

    pub fn input_push(&mut self, c: char) {
        if let Some(s) = self.input.buffer_mut() {
            s.push(c);
        }
    }

    pub fn input_pop(&mut self) {
        if let Some(s) = self.input.buffer_mut() {
            s.pop();
        }
    }

    /// Efface le dernier mot (Ctrl-W).
    pub fn input_pop_word(&mut self) {
        if let Some(s) = self.input.buffer_mut() {
            while s.ends_with(' ') {
                s.pop();
            }
            while let Some(c) = s.chars().last() {
                if c == ' ' {
                    break;
                }
                s.pop();
            }
        }
    }

    /// Vide la ligne (Ctrl-U).
    pub fn input_clear(&mut self) {
        if let Some(s) = self.input.buffer_mut() {
            s.clear();
        }
    }

    pub fn input_cancel(&mut self) {
        self.input = Input::Normal;
    }

    /// Valide la saisie courante ; renvoie l'effet correspondant (lecture d'URL
    /// ou recherche), ou `None` si la saisie est vide/invalide.
    pub fn input_submit(&mut self) -> Option<Effect> {
        // On reprend la saisie et on repasse en Normal par défaut ; le mode de
        // connexion SoundCloud réarme ensuite la saisie Mixcloud.
        match std::mem::replace(&mut self.input, Input::Normal) {
            Input::Command(s) => {
                let url = s.trim().to_string();
                if url.is_empty() {
                    None
                } else if crate::providers::platform_of(&url).is_some() {
                    self.launched_from = None;
                    self.status = i18n::loading(self.lang);
                    Some(Effect::Play(url))
                } else {
                    self.status = i18n::url_unrecognized(self.lang);
                    None
                }
            }
            Input::Search(s) => {
                let q = s.trim().to_string();
                if q.is_empty() {
                    None
                } else {
                    self.status = i18n::searching(self.lang, &q);
                    Some(self.request(Section::Search, FetchRequest::Search(q)))
                }
            }
            Input::ConnectSoundCloud(s) => {
                self.sc_handle = crate::config::normalize_handle(&s);
                // Enchaîne sur le pseudo Mixcloud (pré-rempli).
                self.input = Input::ConnectMixcloud(self.mc_handle.clone().unwrap_or_default());
                None
            }
            Input::ConnectMixcloud(s) => {
                self.mc_handle = crate::config::normalize_handle(&s);
                self.status = i18n::accounts_status(
                    self.lang,
                    self.sc_handle.as_deref().unwrap_or("—"),
                    self.mc_handle.as_deref().unwrap_or("—"),
                );
                Some(Effect::SaveConfig)
            }
            Input::Normal => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn track(p: Platform, title: &str) -> Track {
        Track {
            platform: p,
            id: title.into(),
            title: title.into(),
            artist: "artiste".into(),
            permalink: format!("https://soundcloud.com/x/{title}"),
            duration_ms: Some(200_000),
        }
    }

    fn mix() -> Vec<Track> {
        vec![
            track(Platform::SoundCloud, "sc1"),
            track(Platform::Mixcloud, "mc1"),
            track(Platform::SoundCloud, "sc2"),
        ]
    }

    /// App démarrée sur la recherche, garnie de trois morceaux.
    fn app_with_mix() -> App {
        let mut a = App::new();
        a.restore(Vec::new(), Vec::new(), mix());
        a
    }

    fn fetched(tracks: Vec<Track>) -> Fetched {
        Fetched {
            tracks,
            failures: Vec::new(),
        }
    }

    #[test]
    fn navigation_liste_clampe_aux_bornes() {
        let mut a = app_with_mix();
        assert_eq!(a.apply(Action::Up), None);
        assert_eq!(a.list_index, 0);
        a.apply(Action::Bottom);
        assert_eq!(a.list_index, 2);
        a.apply(Action::Down);
        assert_eq!(a.list_index, 2);
    }

    #[test]
    fn pagination_avance_d_une_demi_fenetre() {
        let mut a = App::new();
        let many: Vec<Track> = (0..50)
            .map(|i| track(Platform::SoundCloud, &format!("t{i}")))
            .collect();
        a.restore(Vec::new(), Vec::new(), many);
        a.set_viewport_rows(20);
        a.apply(Action::PageDown);
        assert_eq!(a.list_index, 10);
        a.apply(Action::PageDown);
        assert_eq!(a.list_index, 20);
        a.apply(Action::PageUp);
        assert_eq!(a.list_index, 10);
    }

    #[test]
    fn filtre_plateforme_reduit_les_visibles() {
        let mut a = app_with_mix();
        a.apply(Action::FilterMixcloud);
        assert_eq!(a.visible_indices().len(), 1);
        assert_eq!(a.selected_track().unwrap().title, "mc1");
        a.apply(Action::FilterAll);
        assert_eq!(a.visible_indices().len(), 3);
    }

    #[test]
    fn activer_un_morceau_emet_play() {
        let mut a = app_with_mix();
        let eff = a.apply(Action::Activate);
        assert_eq!(
            eff,
            Some(Effect::Play("https://soundcloud.com/x/sc1".into()))
        );
    }

    #[test]
    fn play_pause_joue_la_selection_puis_bascule() {
        let mut a = app_with_mix();
        assert_eq!(
            a.apply(Action::PlayPause),
            Some(Effect::Play("https://soundcloud.com/x/sc1".into()))
        );
        a.sync_current(Some(track(Platform::SoundCloud, "sc1")));
        assert_eq!(a.apply(Action::PlayPause), Some(Effect::Toggle));
    }

    #[test]
    fn stop_emet_l_effet_et_le_statut() {
        let mut a = app_with_mix();
        assert_eq!(a.apply(Action::Stop), Some(Effect::Stop));
        assert_eq!(a.status, i18n::playback_stopped(Lang::En));
    }

    #[test]
    fn suivant_avance_depuis_le_morceau_en_cours_pas_depuis_le_curseur() {
        let mut a = app_with_mix();
        a.sync_current(Some(track(Platform::SoundCloud, "sc1")));
        // L'utilisateur a déplacé le curseur sur le dernier morceau…
        a.apply(Action::Bottom);
        // …mais « suivant » enchaîne bien après sc1, c'est-à-dire mc1.
        assert_eq!(
            a.apply(Action::Next),
            Some(Effect::Play("https://soundcloud.com/x/mc1".into()))
        );
        assert_eq!(a.list_index, 1);
    }

    #[test]
    fn fin_de_liste_ne_reboucle_pas_sur_le_dernier() {
        let mut a = app_with_mix();
        a.sync_current(Some(track(Platform::SoundCloud, "sc2")));
        // Fin naturelle du dernier morceau : rien à enchaîner.
        assert_eq!(a.apply(Action::Next), None);
        assert_eq!(a.status, i18n::end_of_list(Lang::En));
        // Et au début, « précédent » ne rejoue pas le premier.
        let mut a = app_with_mix();
        a.sync_current(Some(track(Platform::SoundCloud, "sc1")));
        assert_eq!(a.apply(Action::Prev), None);
    }

    #[test]
    fn precedent_redemarre_le_morceau_apres_trois_secondes() {
        let mut a = app_with_mix();
        a.sync_current(Some(track(Platform::Mixcloud, "mc1")));
        a.playback.duration_ms = 200_000;
        a.playback.position_ms = 45_000;
        a.playback.seekable = true;
        assert_eq!(a.apply(Action::Prev), Some(Effect::Seek(-45_000)));
        assert_eq!(a.playback.position_ms, 0);
        // Flux non seekable (HLS) : on relance la lecture depuis l'URL.
        a.playback.position_ms = 45_000;
        a.playback.seekable = false;
        assert_eq!(
            a.apply(Action::Prev),
            Some(Effect::Play("https://soundcloud.com/x/mc1".into()))
        );
        // Tout début du morceau : piste précédente.
        a.playback.position_ms = 1_000;
        assert_eq!(
            a.apply(Action::Prev),
            Some(Effect::Play("https://soundcloud.com/x/sc1".into()))
        );
    }

    #[test]
    fn la_file_a_priorite_sur_la_liste_pour_suivant() {
        let mut a = app_with_mix();
        a.apply(Action::Down); // mc1
        assert_eq!(a.apply(Action::Enqueue), None);
        assert!(a.take_state_dirty());
        a.apply(Action::Down); // sc2
        a.apply(Action::Enqueue);
        assert_eq!(a.queue.len(), 2);
        a.sync_current(Some(track(Platform::SoundCloud, "sc1")));
        // Suivant consomme la tête de file, pas le voisin dans la liste.
        assert_eq!(
            a.apply(Action::Next),
            Some(Effect::Play("https://soundcloud.com/x/mc1".into()))
        );
        assert_eq!(a.queue.len(), 1);
        assert!(a.take_state_dirty());
        assert_eq!(
            a.apply(Action::Next),
            Some(Effect::Play("https://soundcloud.com/x/sc2".into()))
        );
        assert!(a.queue.is_empty());
        // File vide : on retombe sur la liste (après sc1 → mc1).
        assert_eq!(
            a.apply(Action::Next),
            Some(Effect::Play("https://soundcloud.com/x/mc1".into()))
        );
    }

    #[test]
    fn jouer_depuis_la_file_la_consomme() {
        let mut a = app_with_mix();
        a.apply(Action::Enqueue);
        a.apply(Action::Down);
        a.apply(Action::Enqueue);
        a.take_state_dirty();
        a.focus = Focus::Sidebar;
        a.section_index = Section::Queue.index();
        a.select_section(Section::Queue);
        a.apply(Action::Activate); // ouvre la file
        assert_eq!(a.focus, Focus::List);
        assert_eq!(a.tracks().len(), 2);
        a.apply(Action::Down);
        let eff = a.apply(Action::Activate);
        assert_eq!(
            eff,
            Some(Effect::Play("https://soundcloud.com/x/mc1".into()))
        );
        assert_eq!(a.queue.len(), 1);
        assert_eq!(a.queue[0].title, "sc1");
        assert!(a.take_state_dirty());
        assert_eq!(a.list_index, 0, "curseur ramené dans les bornes");
    }

    #[test]
    fn retirer_et_vider_ne_marchent_que_dans_file_et_historique() {
        let mut a = app_with_mix();
        assert_eq!(a.apply(Action::RemoveFromList), None);
        assert_eq!(a.tracks().len(), 3, "liste de recherche intacte");
        assert_eq!(a.status, i18n::edit_hint(Lang::En));
        a.apply(Action::Enqueue);
        a.select_section(Section::Queue);
        a.take_state_dirty();
        a.apply(Action::RemoveFromList);
        assert!(a.queue.is_empty());
        assert!(a.take_state_dirty());
        // Vider une liste déjà vide ne marque rien.
        a.apply(Action::ClearList);
        assert!(!a.take_state_dirty());
    }

    #[test]
    fn historique_dedoublonne_et_plafonne() {
        let mut a = App::new();
        a.sync_current(Some(track(Platform::SoundCloud, "a")));
        a.sync_current(Some(track(Platform::SoundCloud, "b")));
        a.sync_current(Some(track(Platform::SoundCloud, "a")));
        let titles: Vec<_> = a.history.iter().map(|t| t.title.as_str()).collect();
        assert_eq!(titles, ["a", "b"], "le plus récent en tête, sans doublon");
        assert!(a.take_state_dirty());
        // Resynchroniser le même morceau à chaque frame ne touche à rien.
        a.sync_current(Some(track(Platform::SoundCloud, "a")));
        assert!(!a.take_state_dirty());
        // Plafond.
        for i in 0..(HISTORY_CAP + 20) {
            a.sync_current(Some(track(Platform::Mixcloud, &format!("m{i}"))));
        }
        assert_eq!(a.history.len(), HISTORY_CAP);
    }

    #[test]
    fn demarre_sur_l_historique_s_il_existe_sinon_sur_la_recherche() {
        let mut a = App::new();
        a.restore(Vec::new(), vec![track(Platform::SoundCloud, "h1")], mix());
        assert_eq!(a.section, Section::History);
        assert_eq!(a.tracks().len(), 1);
        let a = app_with_mix();
        assert_eq!(a.section, Section::Search);
        assert_eq!(a.tracks().len(), 3);
    }

    #[test]
    fn les_listes_par_section_survivent_a_la_navigation() {
        let mut a = app_with_mix();
        a.sc_handle = Some("me".into());
        a.focus = Focus::Sidebar;
        a.apply(Action::Top); // Likes
        let eff = a.apply(Action::Activate);
        let Some(Effect::Fetch { id, request }) = eff else {
            panic!("attendu Fetch, eu {eff:?}");
        };
        assert_eq!(request, FetchRequest::Library(LibrarySection::Likes));
        assert!(a.is_busy());
        a.deliver(id, fetched(vec![track(Platform::SoundCloud, "like1")]));
        assert_eq!(a.section, Section::Likes);
        assert_eq!(a.tracks().len(), 1);
        assert!(!a.is_busy());
        // Retour sur la recherche : ses résultats sont toujours là.
        a.focus = Focus::Sidebar;
        a.select_section(Section::Search);
        assert_eq!(a.tracks().len(), 3);
        // Et les Likes aussi.
        assert_eq!(a.tracks_of(Section::Likes).len(), 1);
    }

    #[test]
    fn un_resultat_perime_est_ignore() {
        let mut a = App::new();
        a.begin_search();
        for c in "first".chars() {
            a.input_push(c);
        }
        let Some(Effect::Fetch { id: id1, .. }) = a.input_submit() else {
            panic!("attendu Fetch");
        };
        a.begin_search();
        for c in "second".chars() {
            a.input_push(c);
        }
        let Some(Effect::Fetch { id: id2, .. }) = a.input_submit() else {
            panic!("attendu Fetch");
        };
        assert_ne!(id1, id2);
        // La première réponse arrive en retard : ignorée.
        a.deliver(id1, fetched(vec![track(Platform::SoundCloud, "stale")]));
        assert!(a.tracks_of(Section::Search).is_empty());
        assert!(a.is_busy());
        a.deliver(id2, fetched(vec![track(Platform::SoundCloud, "fresh")]));
        assert_eq!(a.tracks_of(Section::Search)[0].title, "fresh");
        assert!(!a.is_busy());
    }

    #[test]
    fn une_reponse_d_une_autre_section_est_rangee_sans_voler_la_vue() {
        let mut a = App::new();
        a.sc_handle = Some("me".into());
        a.focus = Focus::Sidebar;
        a.apply(Action::Top); // Likes
        let Some(Effect::Fetch { id: likes_id, .. }) = a.apply(Action::Activate) else {
            panic!("attendu Fetch");
        };
        // Pendant le chargement, l'utilisateur lance une recherche.
        a.begin_search();
        a.input_push('q');
        let Some(Effect::Fetch { id: search_id, .. }) = a.input_submit() else {
            panic!("attendu Fetch");
        };
        assert_eq!(a.pending.len(), 2, "deux requêtes en vol, une par section");
        let searching = a.status.clone();
        // Les Likes arrivent en premier : rangés, mais la vue et le statut de
        // la recherche en cours sont préservés.
        a.deliver(
            likes_id,
            fetched(vec![track(Platform::SoundCloud, "like1")]),
        );
        assert_eq!(a.tracks_of(Section::Likes).len(), 1);
        assert_eq!(
            a.section,
            Section::Likes,
            "la vue Likes était affichée : elle le reste"
        );
        assert_eq!(a.status, searching);
        assert!(a.is_busy());
        a.deliver(search_id, fetched(vec![track(Platform::Mixcloud, "hit")]));
        assert_eq!(a.section, Section::Search);
        assert!(!a.is_busy());
        assert_eq!(
            a.tracks_of(Section::Likes).len(),
            1,
            "les Likes n'ont pas été perdus"
        );
    }

    #[test]
    fn ecouter_depuis_l_historique_garde_la_liste_stable_et_enchaine_vers_le_bas() {
        let (ta, tb, tc) = (
            track(Platform::SoundCloud, "A"),
            track(Platform::SoundCloud, "B"),
            track(Platform::SoundCloud, "C"),
        );
        let mut a = App::new();
        a.restore(vec![], vec![ta.clone(), tb.clone(), tc.clone()], vec![]);
        assert_eq!(a.section, Section::History);
        // Entrée sur A (en tête) puis enchaînement naturel.
        assert_eq!(
            a.apply(Action::Activate),
            Some(Effect::Play(ta.permalink.clone()))
        );
        a.sync_current(Some(ta.clone()));
        assert_eq!(
            a.apply(Action::Next),
            Some(Effect::Play(tb.permalink.clone()))
        );
        a.sync_current(Some(tb.clone()));
        let order: Vec<_> = a.history.iter().map(|t| t.title.as_str()).collect();
        assert_eq!(
            order,
            ["A", "B", "C"],
            "la liste ne bouge pas sous le curseur"
        );
        assert_eq!(a.list_index, 1);
        assert_eq!(
            a.apply(Action::Next),
            Some(Effect::Play(tc.permalink.clone()))
        );
        a.sync_current(Some(tc.clone()));
        assert_eq!(a.apply(Action::Next), None, "fin de liste, pas de boucle");
        // « Précédent » (tout début du morceau) remonte bien vers B.
        a.playback.position_ms = 500;
        assert_eq!(
            a.apply(Action::Prev),
            Some(Effect::Play(tb.permalink.clone()))
        );
        // Depuis une autre vue, l'historique reste « le plus récent en tête ».
        let mut a = App::new();
        a.restore(vec![], vec![ta.clone(), tb.clone()], vec![tc.clone()]);
        a.select_section(Section::Search);
        a.focus = Focus::List;
        assert_eq!(
            a.apply(Action::Activate),
            Some(Effect::Play(tc.permalink.clone()))
        );
        a.sync_current(Some(tc.clone()));
        let order: Vec<_> = a.history.iter().map(|t| t.title.as_str()).collect();
        assert_eq!(order, ["C", "A", "B"]);
        // Et rejouer A depuis la recherche le remonte en tête.
        a.set_tracks(Section::Search, vec![ta.clone()]);
        a.apply(Action::Activate);
        a.sync_current(Some(ta.clone()));
        assert_eq!(a.history[0].title, "A");
    }

    #[test]
    fn espace_et_precedent_dans_la_file_consomment_aussi() {
        let mut a = app_with_mix();
        a.apply(Action::Enqueue); // sc1
        a.apply(Action::Down);
        a.apply(Action::Enqueue); // mc1
        a.select_section(Section::Queue);
        a.take_state_dirty();
        // Espace sans lecture en cours : joue et consomme.
        assert_eq!(
            a.apply(Action::PlayPause),
            Some(Effect::Play("https://soundcloud.com/x/sc1".into()))
        );
        assert_eq!(a.queue.len(), 1);
        assert!(a.take_state_dirty());
        // « Précédent » atterrissant sur une ligne de la file : consomme aussi.
        a.sync_current(Some(track(Platform::SoundCloud, "sc1")));
        a.apply(Action::Enqueue); // en vue File : refusé (déjà dans la file)
        a.queue.insert(0, track(Platform::Mixcloud, "first"));
        a.list_index = 1;
        a.playback.position_ms = 0;
        assert_eq!(
            a.apply(Action::Prev),
            Some(Effect::Play("https://soundcloud.com/x/first".into()))
        );
        assert_eq!(a.queue.len(), 1, "l'entrée jouée a été consommée");
    }

    #[test]
    fn le_curseur_est_memorise_par_section() {
        let mut a = app_with_mix();
        a.apply(Action::Bottom);
        assert_eq!(a.list_index, 2);
        // Traverser la barre latérale (sections vides) puis revenir.
        a.focus = Focus::Sidebar;
        a.apply(Action::Down); // History (vide)
        assert_eq!(a.list_index, 0);
        a.apply(Action::Down); // Queue (vide)
        a.apply(Action::Up);
        a.apply(Action::Up); // Search
        assert_eq!(a.list_index, 2, "position dans les résultats retrouvée");
    }

    #[test]
    fn ouvrir_la_section_recherche_propose_toujours_une_saisie() {
        let mut a = app_with_mix(); // liste de recherche non vide
        a.focus = Focus::Sidebar;
        a.select_section(Section::Search);
        assert_eq!(a.apply(Action::Activate), None);
        assert_eq!(a.input, Input::Search(String::new()));
        assert_eq!(
            a.tracks().len(),
            3,
            "les résultats restent affichés derrière"
        );
    }

    #[test]
    fn l_echec_d_une_plateforme_est_signale() {
        let mut a = App::new();
        a.begin_search();
        a.input_push('x');
        let Some(Effect::Fetch { id, .. }) = a.input_submit() else {
            panic!("attendu Fetch");
        };
        a.deliver(
            id,
            Fetched {
                tracks: vec![track(Platform::SoundCloud, "r")],
                failures: vec![(Platform::Mixcloud, "HTTP 503".into())],
            },
        );
        assert!(a.status.contains("Mixcloud"), "{}", a.status);
        assert!(a.status.contains("503"), "{}", a.status);
    }

    #[test]
    fn volume_borne_et_emet_effet() {
        let mut a = App::new();
        let mut last = None;
        for _ in 0..10 {
            last = a.apply(Action::VolumeUp);
        }
        assert_eq!(a.playback.volume, 100);
        assert_eq!(last, Some(Effect::SetVolume(100)));
    }

    #[test]
    fn anglais_par_defaut_et_bascule_vers_le_francais() {
        let a = App::new();
        assert_eq!(a.lang, Lang::En);
        assert_eq!(Section::Search.label(a.lang), "⌕  Search");

        let mut a = App::new();
        let eff = a.apply(Action::ToggleLang);
        assert_eq!(a.lang, Lang::Fr);
        assert_eq!(Section::Search.label(a.lang), "⌕  Recherche");
        assert_eq!(eff, Some(Effect::SaveConfig));

        a.apply(Action::ToggleLang);
        assert_eq!(a.lang, Lang::En);
    }

    #[test]
    fn cycle_visualiseur_boucle_sur_les_trois() {
        let mut a = App::new();
        assert_eq!(a.viz, VizMode::Bars);
        a.apply(Action::CycleViz);
        assert_eq!(a.viz, VizMode::Mirror);
        a.apply(Action::CycleViz);
        assert_eq!(a.viz, VizMode::Scope);
        a.apply(Action::CycleViz);
        assert_eq!(a.viz, VizMode::Bars);
    }

    #[test]
    fn l_aide_se_referme_par_dismiss_sans_bloquer_les_commandes_media() {
        let mut a = app_with_mix();
        assert!(!a.dismiss_help(), "rien à fermer");
        a.apply(Action::ToggleHelp);
        assert!(a.show_help);
        // Une commande externe (MPRIS) agit même aide ouverte.
        assert_eq!(a.apply(Action::VolumeUp), Some(Effect::SetVolume(85)));
        assert!(a.show_help);
        // Le clavier/la souris passent par dismiss_help : la touche est consommée.
        assert!(a.dismiss_help());
        assert!(!a.show_help);
        assert!(!a.dismiss_help());
    }

    #[test]
    fn saisie_url_valide_emet_play() {
        let mut a = App::new();
        a.apply(Action::BeginCommand);
        for c in "https://www.mixcloud.com/a/b/".chars() {
            a.input_push(c);
        }
        let eff = a.input_submit();
        assert_eq!(
            eff,
            Some(Effect::Play("https://www.mixcloud.com/a/b/".into()))
        );
        assert_eq!(a.input, Input::Normal);
    }

    #[test]
    fn saisie_url_invalide_ne_joue_pas() {
        let mut a = App::new();
        a.begin_command();
        for c in "coucou".chars() {
            a.input_push(c);
        }
        assert_eq!(a.input_submit(), None);
    }

    #[test]
    fn edition_de_ligne_mot_et_vidage() {
        let mut a = App::new();
        a.begin_search();
        for c in "four tet live".chars() {
            a.input_push(c);
        }
        // Sémantique readline (Ctrl-W) : efface le mot, garde l'espace qui le
        // précède ; un second appui efface alors « tet ».
        a.input_pop_word();
        assert_eq!(a.input, Input::Search("four tet ".into()));
        a.input_pop_word();
        assert_eq!(a.input, Input::Search("four ".into()));
        a.input_clear();
        assert_eq!(a.input, Input::Search(String::new()));
    }

    #[test]
    fn seek_accel_monte_par_paliers_de_trois_appuis() {
        let mut s = SeekAccel::new();
        let t = Instant::now();
        for _ in 0..3 {
            assert_eq!(s.step(t, 1), 10_000);
        }
        for _ in 0..3 {
            assert_eq!(s.step(t, 1), 30_000);
        }
        for _ in 0..3 {
            assert_eq!(s.step(t, 1), 60_000);
        }
        for _ in 0..3 {
            assert_eq!(s.step(t, 1), 300_000);
        }
        for _ in 0..5 {
            assert_eq!(s.step(t, 1), 600_000);
        }
    }

    #[test]
    fn seek_accel_arriere_est_signe_negatif() {
        let mut s = SeekAccel::new();
        let t = Instant::now();
        assert_eq!(s.step(t, -1), -10_000);
    }

    #[test]
    fn seek_accel_repart_apres_une_pause() {
        let mut s = SeekAccel::new();
        let t = Instant::now();
        for _ in 0..4 {
            s.step(t, 1);
        }
        assert_eq!(s.step(t, 1), 30_000);
        let later = t + Duration::from_millis(1600);
        assert_eq!(s.step(later, 1), 10_000);
    }

    #[test]
    fn seek_accel_repart_au_changement_de_sens() {
        let mut s = SeekAccel::new();
        let t = Instant::now();
        for _ in 0..4 {
            s.step(t, 1);
        }
        assert_eq!(s.step(t, -1), -10_000);
    }

    #[test]
    fn saut_par_action_utilise_le_temps_injecte() {
        let mut a = App::new();
        a.sync_current(Some(track(Platform::SoundCloud, "sc1")));
        a.playback.duration_ms = 3_600_000;
        a.playback.seekable = true;
        let t = Instant::now();
        a.tick(t);
        for _ in 0..3 {
            assert_eq!(a.apply(Action::SeekForward), Some(Effect::Seek(10_000)));
        }
        assert_eq!(a.apply(Action::SeekForward), Some(Effect::Seek(30_000)));
        // Pause : le pas repart à 10 s.
        a.tick(t + Duration::from_secs(2));
        assert_eq!(a.apply(Action::SeekForward), Some(Effect::Seek(10_000)));
        assert_eq!(a.apply(Action::SeekBack), Some(Effect::Seek(-10_000)));
    }

    #[test]
    fn seek_sans_lecture_ou_non_seekable_ne_fait_rien() {
        let mut a = App::new();
        assert_eq!(a.seek(10_000), None);
        a.sync_current(Some(track(Platform::Mixcloud, "mc1")));
        a.playback.duration_ms = 200_000;
        a.playback.position_ms = 50_000;
        a.playback.seekable = false;
        assert_eq!(a.seek(10_000), None);
        assert_eq!(a.playback.position_ms, 50_000);
    }

    #[test]
    fn seek_seekable_borne_la_position_et_emet_leffet() {
        let mut a = App::new();
        a.sync_current(Some(track(Platform::SoundCloud, "sc1")));
        a.playback.duration_ms = 200_000;
        a.playback.seekable = true;
        a.playback.position_ms = 50_000;
        assert_eq!(a.seek(10_000), Some(Effect::Seek(10_000)));
        assert_eq!(a.playback.position_ms, 60_000);
        assert_eq!(a.seek(-999_000), Some(Effect::Seek(-999_000)));
        assert_eq!(a.playback.position_ms, 0);
        assert_eq!(a.seek(999_000), Some(Effect::Seek(999_000)));
        assert_eq!(a.playback.position_ms, 200_000);
    }

    #[test]
    fn seek_absolu_se_traduit_en_delta() {
        let mut a = App::new();
        a.sync_current(Some(track(Platform::SoundCloud, "sc1")));
        a.playback.duration_ms = 200_000;
        a.playback.seekable = true;
        a.playback.position_ms = 50_000;
        assert_eq!(a.seek_to(120_000), Some(Effect::Seek(70_000)));
        assert_eq!(a.playback.position_ms, 120_000);
        // Au-delà de la durée : borné.
        assert_eq!(a.seek_to(999_999), Some(Effect::Seek(80_000)));
        assert_eq!(a.playback.position_ms, 200_000);
    }
}

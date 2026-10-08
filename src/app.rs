//! État de l'application et logique de mise à jour (pure, testable).
//!
//! `App` ne fait AUCUN I/O : il muta son état et renvoie d'éventuels
//! [`Effect`] (lecture audio) que la boucle d'événements exécute sur le
//! `Player`. La position/durée/état de lecture affichés sont resynchronisés
//! depuis le moteur à chaque frame (cf. `main.rs`).

use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::i18n::{self, Lang};
use crate::model::{Platform, Track};

/// Sections de la barre latérale gauche.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

/// Effets de bord exécutés par la boucle principale (audio, réseau, disque).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Effect {
    Play(String),
    Toggle,
    Stop,
    SetVolume(u8),
    /// Saut relatif dans le morceau courant (delta signé en millisecondes).
    Seek(i64),
    Search(String),
    /// Charge une section de bibliothèque depuis les comptes configurés.
    LoadLibrary(crate::providers::LibrarySection),
    /// Persiste les pseudos de compte et la langue sur disque.
    SaveConfig,
}

/// Intentions de haut niveau, indépendantes du clavier/souris.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Up,
    Down,
    Top,
    Bottom,
    Activate,
    ToggleFocus,
    PlayPause,
    Next,
    Prev,
    VolumeUp,
    VolumeDown,
    FilterAll,
    FilterSoundCloud,
    FilterMixcloud,
    ToggleLang,
    Quit,
}

/// État global de l'application.
pub struct App {
    pub should_quit: bool,
    pub focus: Focus,
    pub section: Section,
    pub section_index: usize,
    pub filter: Filter,
    pub tracks: Vec<Track>,
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
}

impl App {
    pub fn new() -> Self {
        App {
            should_quit: false,
            focus: Focus::List,
            section: Section::Likes,
            section_index: 0,
            filter: Filter::All,
            tracks: Vec::new(),
            list_index: 0,
            playback: Playback {
                volume: 80,
                ..Default::default()
            },
            status: i18n::welcome(Lang::En),
            input: Input::Normal,
            sc_handle: None,
            mc_handle: None,
            viz: VizMode::Bars,
            lang: Lang::En,
        }
    }

    /// Applique la langue chargée depuis la config au démarrage (avant la
    /// première frame) et rafraîchit le message d'accueil en conséquence.
    pub fn set_startup_lang(&mut self, lang: Lang) {
        self.lang = lang;
        self.status = i18n::welcome(lang);
    }

    /// Indique si au moins un compte est connecté.
    pub fn has_account(&self) -> bool {
        self.sc_handle.is_some() || self.mc_handle.is_some()
    }

    /// Passe au style de visualiseur suivant.
    pub fn cycle_viz(&mut self) {
        self.viz = self.viz.next();
        self.status = i18n::viz_changed(self.lang, self.viz.label(self.lang));
    }

    /// Indices des morceaux visibles après application du filtre courant.
    pub fn visible_indices(&self) -> Vec<usize> {
        self.tracks
            .iter()
            .enumerate()
            .filter(|(_, t)| self.filter.keep(t))
            .map(|(i, _)| i)
            .collect()
    }

    /// Le morceau actuellement surligné dans la liste.
    pub fn selected_track(&self) -> Option<&Track> {
        let vis = self.visible_indices();
        vis.get(self.list_index).and_then(|&i| self.tracks.get(i))
    }

    fn selected_url(&self) -> Option<String> {
        self.selected_track().map(|t| t.permalink.clone())
    }

    /// Applique une intention et renvoie l'effet audio éventuel.
    pub fn apply(&mut self, action: Action) -> Option<Effect> {
        match action {
            Action::Quit => {
                self.should_quit = true;
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
                    self.selected_url().map(Effect::Play)
                } else {
                    Some(Effect::Toggle)
                }
            }
            Action::Next => self.skip(1),
            Action::Prev => self.skip(-1),
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
            Action::ToggleLang => {
                self.lang = self.lang.toggle();
                self.status = i18n::lang_changed(self.lang);
                Some(Effect::SaveConfig)
            }
        }
    }

    fn move_cursor(&mut self, delta: i32) {
        match self.focus {
            Focus::Sidebar => {
                let n = Section::ALL.len() as i32;
                let i = (self.section_index as i32 + delta).rem_euclid(n) as usize;
                self.section_index = i;
                self.section = Section::ALL[i];
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
                self.section_index = if top { 0 } else { Section::ALL.len() - 1 };
                self.section = Section::ALL[self.section_index];
            }
            Focus::List => {
                let n = self.visible_indices().len();
                self.list_index = if top || n == 0 { 0 } else { n - 1 };
            }
        }
    }

    fn activate(&mut self) -> Option<Effect> {
        match self.focus {
            Focus::Sidebar => self.open_section(),
            Focus::List => match self.selected_url() {
                Some(url) => {
                    self.status = i18n::loading(self.lang);
                    Some(Effect::Play(url))
                }
                None => None,
            },
        }
    }

    /// Ouvre la section sélectionnée dans la sidebar.
    fn open_section(&mut self) -> Option<Effect> {
        use crate::providers::LibrarySection;
        self.focus = Focus::List;
        let lib = match self.section {
            Section::Likes => Some(LibrarySection::Likes),
            Section::Playlists => Some(LibrarySection::Playlists),
            Section::Feed => Some(LibrarySection::Feed),
            Section::Search => {
                self.begin_search();
                return None;
            }
            Section::History | Section::Queue => {
                self.status = i18n::section_soon(self.lang, self.section.label(self.lang).trim());
                return None;
            }
        };
        match lib {
            Some(sec) if self.has_account() => {
                self.status =
                    i18n::loading_section(self.lang, self.section.label(self.lang).trim());
                Some(Effect::LoadLibrary(sec))
            }
            Some(_) => {
                self.status = i18n::no_account(self.lang);
                None
            }
            None => None,
        }
    }

    fn skip(&mut self, delta: i32) -> Option<Effect> {
        let vis = self.visible_indices();
        if vis.is_empty() {
            return None;
        }
        let i = (self.list_index as i32 + delta).clamp(0, vis.len() as i32 - 1) as usize;
        self.list_index = i;
        self.selected_url().map(Effect::Play)
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

    // --- Mode saisie (`:` URL) ------------------------------------------------

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
                    Some(Effect::Search(q))
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

    /// Remplace la liste par des résultats de recherche.
    pub fn set_results(&mut self, tracks: Vec<Track>) {
        let n = tracks.len();
        self.tracks = tracks;
        self.list_index = 0;
        self.focus = Focus::List;
        self.section = Section::Search;
        self.section_index = Section::ALL
            .iter()
            .position(|s| *s == Section::Search)
            .unwrap_or(0);
        self.status = if n == 0 {
            i18n::no_results(self.lang)
        } else {
            i18n::n_results(self.lang, n)
        };
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

    fn app_with_mix() -> App {
        let mut a = App::new();
        a.tracks = vec![
            track(Platform::SoundCloud, "sc1"),
            track(Platform::Mixcloud, "mc1"),
            track(Platform::SoundCloud, "sc2"),
        ];
        a
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
        // Rien en cours -> Play.
        assert_eq!(
            a.apply(Action::PlayPause),
            Some(Effect::Play("https://soundcloud.com/x/sc1".into()))
        );
        // Simule un morceau en cours -> Toggle.
        a.playback.current = Some(track(Platform::SoundCloud, "sc1"));
        assert_eq!(a.apply(Action::PlayPause), Some(Effect::Toggle));
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
        a.cycle_viz();
        assert_eq!(a.viz, VizMode::Mirror);
        a.cycle_viz();
        assert_eq!(a.viz, VizMode::Scope);
        a.cycle_viz();
        assert_eq!(a.viz, VizMode::Bars);
    }

    #[test]
    fn saisie_url_valide_emet_play() {
        let mut a = App::new();
        a.begin_command();
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
    fn seek_accel_monte_par_paliers_de_trois_appuis() {
        let mut s = SeekAccel::new();
        let t = Instant::now();
        // Appuis 1-3 : 10s.
        for _ in 0..3 {
            assert_eq!(s.step(t, 1), 10_000);
        }
        // Appuis 4-6 : 30s.
        for _ in 0..3 {
            assert_eq!(s.step(t, 1), 30_000);
        }
        // Appuis 7-9 : 1min.
        for _ in 0..3 {
            assert_eq!(s.step(t, 1), 60_000);
        }
        // Appuis 10-12 : 5min.
        for _ in 0..3 {
            assert_eq!(s.step(t, 1), 300_000);
        }
        // Appuis 13+ : 10min, puis plafonne.
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
            s.step(t, 1); // rendu au palier 30s
        }
        assert_eq!(s.step(t, 1), 30_000);
        // Pause > 1,5 s : le pas repart à 10s.
        let later = t + Duration::from_millis(1600);
        assert_eq!(s.step(later, 1), 10_000);
    }

    #[test]
    fn seek_accel_repart_au_changement_de_sens() {
        let mut s = SeekAccel::new();
        let t = Instant::now();
        for _ in 0..4 {
            s.step(t, 1); // 30s vers l'avant
        }
        // Inverser le sens ramène au plus petit pas.
        assert_eq!(s.step(t, -1), -10_000);
    }

    #[test]
    fn seek_sans_lecture_ou_non_seekable_ne_fait_rien() {
        let mut a = App::new();
        // Rien en lecture.
        assert_eq!(a.seek(10_000), None);
        // En lecture mais flux non-seekable (HLS).
        a.playback.current = Some(track(Platform::Mixcloud, "mc1"));
        a.playback.duration_ms = 200_000;
        a.playback.position_ms = 50_000;
        a.playback.seekable = false;
        assert_eq!(a.seek(10_000), None);
        assert_eq!(a.playback.position_ms, 50_000); // inchangé
    }

    #[test]
    fn seek_seekable_borne_la_position_et_emet_leffet() {
        let mut a = App::new();
        a.playback.current = Some(track(Platform::SoundCloud, "sc1"));
        a.playback.duration_ms = 200_000;
        a.playback.seekable = true;
        a.playback.position_ms = 50_000;
        assert_eq!(a.seek(10_000), Some(Effect::Seek(10_000)));
        assert_eq!(a.playback.position_ms, 60_000);
        // Bornage en bas.
        assert_eq!(a.seek(-999_000), Some(Effect::Seek(-999_000)));
        assert_eq!(a.playback.position_ms, 0);
        // Bornage en haut.
        assert_eq!(a.seek(999_000), Some(Effect::Seek(999_000)));
        assert_eq!(a.playback.position_ms, 200_000);
    }
}

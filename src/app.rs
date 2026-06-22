//! État de l'application et logique de mise à jour (pure, testable).
//!
//! `App` ne fait AUCUN I/O : il muta son état et renvoie d'éventuels
//! [`Effect`] (lecture audio) que la boucle d'événements exécute sur le
//! `Player`. La position/durée/état de lecture affichés sont resynchronisés
//! depuis le moteur à chaque frame (cf. `main.rs`).

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

    pub fn label(self) -> &'static str {
        match self {
            Section::Likes => "♥  Likes",
            Section::Playlists => "☰  Playlists",
            Section::Feed => "◎  Feed",
            Section::Search => "⌕  Recherche",
            Section::History => "⧗  Historique",
            Section::Queue => "▤  File",
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
    pub fn label(self) -> String {
        match self {
            Filter::All => "Tout".to_string(),
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
    /// Amplitudes du spectre par bande (0..1), pour l'analyseur visuel.
    pub spectrum: Vec<f32>,
    /// Échantillons de forme d'onde (~[-1,1]) pour le mode oscilloscope.
    pub waveform: Vec<f32>,
}

/// Style d'analyseur visuel (cyclé avec `v`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VizMode {
    /// Barres de spectre ancrées en bas.
    Bars,
    /// Spectre symétrique autour d'une ligne centrale (« waveline »).
    Mirror,
    /// Oscilloscope temporel (forme d'onde brute), très réactif.
    Scope,
}

impl VizMode {
    pub const ALL: [VizMode; 3] = [VizMode::Bars, VizMode::Mirror, VizMode::Scope];

    pub fn label(self) -> &'static str {
        match self {
            VizMode::Bars => "barres",
            VizMode::Mirror => "miroir",
            VizMode::Scope => "oscilloscope",
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
    Search(String),
    /// Charge une section de bibliothèque depuis les comptes configurés.
    LoadLibrary(crate::providers::LibrarySection),
    /// Persiste les pseudos de compte sur disque.
    SaveAccounts,
}

/// Intentions de haut niveau, indépendantes du clavier/souris.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Up,
    Down,
    Top,
    Bottom,
    Activate,
    FocusSidebar,
    FocusList,
    ToggleFocus,
    PlayPause,
    Next,
    Prev,
    VolumeUp,
    VolumeDown,
    FilterAll,
    FilterSoundCloud,
    FilterMixcloud,
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
            status: "Bienvenue — 'c' connecter un compte · ':' URL · '/' recherche · '?' aide"
                .to_string(),
            input: Input::Normal,
            sc_handle: None,
            mc_handle: None,
            viz: VizMode::Bars,
        }
    }

    /// Indique si au moins un compte est connecté.
    pub fn has_account(&self) -> bool {
        self.sc_handle.is_some() || self.mc_handle.is_some()
    }

    /// Passe au style de visualiseur suivant.
    pub fn cycle_viz(&mut self) {
        self.viz = self.viz.next();
        self.status = format!("Visualiseur : {}", self.viz.label());
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
            Action::FocusSidebar => {
                self.focus = Focus::Sidebar;
                None
            }
            Action::FocusList => {
                self.focus = Focus::List;
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
                    self.status = "Chargement…".to_string();
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
                self.status = format!("{} — bientôt", self.section.label().trim());
                return None;
            }
        };
        match lib {
            Some(sec) if self.has_account() => {
                self.status = format!("Chargement de {}…", self.section.label().trim());
                Some(Effect::LoadLibrary(sec))
            }
            Some(_) => {
                self.status =
                    "Aucun compte connecté — appuie sur 'c' pour entrer tes pseudos".to_string();
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

    fn bump_volume(&mut self, delta: i32) -> Effect {
        let v = (self.playback.volume as i32 + delta).clamp(0, 100) as u8;
        self.playback.volume = v;
        self.status = format!("Volume : {v}%");
        Effect::SetVolume(v)
    }

    fn set_filter(&mut self, f: Filter) {
        self.filter = f;
        self.list_index = 0;
        self.status = format!("Filtre : {}", f.label());
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
                    self.status = "Chargement…".to_string();
                    Some(Effect::Play(url))
                } else {
                    self.status = "URL non reconnue (SoundCloud ou Mixcloud)".to_string();
                    None
                }
            }
            Input::Search(s) => {
                let q = s.trim().to_string();
                if q.is_empty() {
                    None
                } else {
                    self.status = format!("Recherche : « {q} »…");
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
                self.status = format!(
                    "Comptes — SoundCloud : {} · Mixcloud : {}  (ouvre Likes/Playlists/Feed)",
                    self.sc_handle.as_deref().unwrap_or("—"),
                    self.mc_handle.as_deref().unwrap_or("—"),
                );
                Some(Effect::SaveAccounts)
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
            "Aucun résultat".to_string()
        } else {
            format!("{n} résultats")
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
}

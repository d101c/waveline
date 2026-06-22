//! État de l'application et logique de mise à jour (pure, testable).
//!
//! `App` ne fait AUCUN I/O et ne connaît ni le terminal ni le réseau : il
//! reçoit des intentions ([`Action`]) et muta son état. Le rendu (`ui.rs`) et
//! la boucle d'événements (`main.rs`) restent séparés — on peut tester toute
//! la navigation sans terminal.

use crate::model::{Platform, Track};

/// Les sections de la barre latérale gauche.
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

/// État de lecture courant (sera piloté par le moteur audio plus tard).
#[derive(Debug, Clone, Default)]
pub struct Playback {
    pub current: Option<Track>,
    pub playing: bool,
    pub position_ms: u64,
    pub volume: u8, // 0..=100
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
            status: "Bienvenue dans waveline — appuie sur ? pour l'aide".to_string(),
        }
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

    /// Le morceau actuellement surligné dans la liste, si la liste a le focus
    /// logique d'une sélection valide.
    pub fn selected_track(&self) -> Option<&Track> {
        let vis = self.visible_indices();
        vis.get(self.list_index).and_then(|&i| self.tracks.get(i))
    }

    /// Applique une intention. Point d'entrée unique de la mutation d'état.
    pub fn apply(&mut self, action: Action) {
        match action {
            Action::Quit => self.should_quit = true,
            Action::ToggleFocus => {
                self.focus = match self.focus {
                    Focus::Sidebar => Focus::List,
                    Focus::List => Focus::Sidebar,
                }
            }
            Action::FocusSidebar => self.focus = Focus::Sidebar,
            Action::FocusList => self.focus = Focus::List,
            Action::Up => self.move_cursor(-1),
            Action::Down => self.move_cursor(1),
            Action::Top => self.move_to_edge(true),
            Action::Bottom => self.move_to_edge(false),
            Action::Activate => self.activate(),
            Action::PlayPause => self.toggle_play(),
            Action::Next => self.skip(1),
            Action::Prev => self.skip(-1),
            Action::VolumeUp => self.set_volume(self.playback.volume.saturating_add(5)),
            Action::VolumeDown => self.set_volume(self.playback.volume.saturating_sub(5)),
            Action::FilterAll => self.set_filter(Filter::All),
            Action::FilterSoundCloud => self.set_filter(Filter::Only(Platform::SoundCloud)),
            Action::FilterMixcloud => self.set_filter(Filter::Only(Platform::Mixcloud)),
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

    fn activate(&mut self) {
        match self.focus {
            Focus::Sidebar => {
                self.focus = Focus::List;
                self.status = format!("Section : {}", self.section.label().trim());
            }
            Focus::List => {
                if let Some(t) = self.selected_track().cloned() {
                    self.play(t);
                }
            }
        }
    }

    /// Démarre la lecture d'un morceau (le câblage audio réel viendra ensuite).
    pub fn play(&mut self, t: Track) {
        self.status = format!("▶ {} — {}", t.artist, t.title);
        self.playback.current = Some(t);
        self.playback.playing = true;
        self.playback.position_ms = 0;
    }

    fn toggle_play(&mut self) {
        if self.playback.current.is_some() {
            self.playback.playing = !self.playback.playing;
        } else {
            // Rien en cours : lance la sélection courante.
            if let Some(t) = self.selected_track().cloned() {
                self.play(t);
            }
        }
    }

    fn skip(&mut self, delta: i32) {
        let vis = self.visible_indices();
        if vis.is_empty() {
            return;
        }
        let i = (self.list_index as i32 + delta).clamp(0, vis.len() as i32 - 1) as usize;
        self.list_index = i;
        if let Some(t) = self.selected_track().cloned() {
            self.play(t);
        }
    }

    fn set_volume(&mut self, v: u8) {
        self.playback.volume = v.min(100);
        self.status = format!("Volume : {}%", self.playback.volume);
    }

    fn set_filter(&mut self, f: Filter) {
        self.filter = f;
        self.list_index = 0;
        self.status = format!("Filtre : {}", f.label());
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
            permalink: "https://x".into(),
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
        a.apply(Action::Up); // déjà en haut
        assert_eq!(a.list_index, 0);
        a.apply(Action::Bottom);
        assert_eq!(a.list_index, 2);
        a.apply(Action::Down); // déjà en bas
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
    fn activer_un_morceau_lance_la_lecture() {
        let mut a = app_with_mix();
        a.apply(Action::Activate);
        assert!(a.playback.playing);
        assert_eq!(a.playback.current.unwrap().title, "sc1");
    }

    #[test]
    fn play_pause_bascule() {
        let mut a = app_with_mix();
        a.apply(Action::PlayPause); // lance sc1
        assert!(a.playback.playing);
        a.apply(Action::PlayPause); // pause
        assert!(!a.playback.playing);
    }

    #[test]
    fn volume_borne_a_100() {
        let mut a = App::new();
        for _ in 0..10 {
            a.apply(Action::VolumeUp);
        }
        assert_eq!(a.playback.volume, 100);
    }
}

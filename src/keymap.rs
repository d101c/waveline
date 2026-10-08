//! Table **unique** des raccourcis clavier.
//!
//! Chaque [`Binding`] relie des touches à une [`Action`] et porte sa propre
//! description : la même table sert au dispatch des événements
//! ([`action_for`]) et au rendu de la fenêtre d'aide. L'aide ne peut donc pas
//! dériver de ce que fait réellement le clavier.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::app::Action;
use crate::i18n::Lang;

/// Une touche : code + modificateur Contrôle (Majuscule est ignorée, le code
/// porte déjà la casse du caractère).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Key {
    pub code: KeyCode,
    pub ctrl: bool,
}

impl Key {
    const fn plain(code: KeyCode) -> Key {
        Key { code, ctrl: false }
    }

    const fn ch(c: char) -> Key {
        Key::plain(KeyCode::Char(c))
    }

    const fn ctrl(c: char) -> Key {
        Key {
            code: KeyCode::Char(c),
            ctrl: true,
        }
    }

    /// Vrai si l'événement clavier correspond à cette touche.
    pub fn matches(&self, ev: &KeyEvent) -> bool {
        ev.modifiers.contains(KeyModifiers::CONTROL) == self.ctrl && ev.code == self.code
    }

    /// Libellé court pour l'aide (`j`, `↓`, `^d`, `space`…).
    pub fn label(&self) -> String {
        let base = match self.code {
            KeyCode::Char(' ') => "space".to_string(),
            KeyCode::Char(c) => c.to_string(),
            KeyCode::Enter => "enter".into(),
            KeyCode::Tab => "tab".into(),
            KeyCode::Esc => "esc".into(),
            KeyCode::Up => "↑".into(),
            KeyCode::Down => "↓".into(),
            KeyCode::Left => "←".into(),
            KeyCode::Right => "→".into(),
            KeyCode::PageUp => "pgup".into(),
            KeyCode::PageDown => "pgdn".into(),
            other => format!("{other:?}").to_lowercase(),
        };
        if self.ctrl {
            format!("^{base}")
        } else {
            base
        }
    }
}

/// Rubrique de l'aide.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Group {
    Navigate,
    Playback,
    Lists,
    App,
}

impl Group {
    pub fn title(self, lang: Lang) -> &'static str {
        match (self, lang) {
            (Group::Navigate, Lang::En) => "Navigate",
            (Group::Navigate, Lang::Fr) => "Navigation",
            (Group::Playback, Lang::En) => "Playback",
            (Group::Playback, Lang::Fr) => "Lecture",
            (Group::Lists, Lang::En) => "Queue & history",
            (Group::Lists, Lang::Fr) => "File & historique",
            (Group::App, Lang::En) => "App",
            (Group::App, Lang::Fr) => "Application",
        }
    }
}

/// Un raccourci : touches, action, rubrique, description (anglais, français).
#[derive(Debug, Clone, Copy)]
pub struct Binding {
    pub keys: &'static [Key],
    pub action: Action,
    pub group: Group,
    help: (&'static str, &'static str),
}

impl Binding {
    pub fn help(&self, lang: Lang) -> &'static str {
        match lang {
            Lang::En => self.help.0,
            Lang::Fr => self.help.1,
        }
    }

    /// Toutes les touches du raccourci, séparées par des espaces.
    pub fn keys_label(&self) -> String {
        self.keys
            .iter()
            .map(Key::label)
            .collect::<Vec<_>>()
            .join(" ")
    }
}

const fn b(
    keys: &'static [Key],
    action: Action,
    group: Group,
    en: &'static str,
    fr: &'static str,
) -> Binding {
    Binding {
        keys,
        action,
        group,
        help: (en, fr),
    }
}

/// La table des raccourcis, dans l'ordre d'affichage de l'aide.
pub const BINDINGS: &[Binding] = &[
    // --- Navigation ---
    b(
        &[Key::ch('j'), Key::plain(KeyCode::Down)],
        Action::Down,
        Group::Navigate,
        "move down",
        "descendre",
    ),
    b(
        &[Key::ch('k'), Key::plain(KeyCode::Up)],
        Action::Up,
        Group::Navigate,
        "move up",
        "monter",
    ),
    b(
        &[Key::ctrl('d'), Key::plain(KeyCode::PageDown)],
        Action::PageDown,
        Group::Navigate,
        "half page down",
        "demi-page vers le bas",
    ),
    b(
        &[Key::ctrl('u'), Key::plain(KeyCode::PageUp)],
        Action::PageUp,
        Group::Navigate,
        "half page up",
        "demi-page vers le haut",
    ),
    b(
        &[Key::ch('g')],
        Action::Top,
        Group::Navigate,
        "top of list",
        "début de liste",
    ),
    b(
        &[Key::ch('G')],
        Action::Bottom,
        Group::Navigate,
        "bottom of list",
        "fin de liste",
    ),
    b(
        &[Key::plain(KeyCode::Tab)],
        Action::ToggleFocus,
        Group::Navigate,
        "switch panel",
        "changer de panneau",
    ),
    b(
        &[Key::ch('1')],
        Action::FilterAll,
        Group::Navigate,
        "show all platforms",
        "toutes les plateformes",
    ),
    b(
        &[Key::ch('2')],
        Action::FilterSoundCloud,
        Group::Navigate,
        "SoundCloud only",
        "SoundCloud seulement",
    ),
    b(
        &[Key::ch('3')],
        Action::FilterMixcloud,
        Group::Navigate,
        "Mixcloud only",
        "Mixcloud seulement",
    ),
    // --- Lecture ---
    b(
        &[Key::plain(KeyCode::Enter)],
        Action::Activate,
        Group::Playback,
        "play / open section",
        "jouer / ouvrir la section",
    ),
    b(
        &[Key::ch(' ')],
        Action::PlayPause,
        Group::Playback,
        "play / pause",
        "lecture / pause",
    ),
    b(
        &[Key::ch('n')],
        Action::Next,
        Group::Playback,
        "next (queue first)",
        "suivant (la file d'abord)",
    ),
    b(
        &[Key::ch('p')],
        Action::Prev,
        Group::Playback,
        "previous / restart",
        "précédent / reprendre",
    ),
    b(
        &[Key::ch('l'), Key::plain(KeyCode::Right)],
        Action::SeekForward,
        Group::Playback,
        "seek fwd (repeat = faster)",
        "avancer (répéter = + vite)",
    ),
    b(
        &[Key::ch('h'), Key::plain(KeyCode::Left)],
        Action::SeekBack,
        Group::Playback,
        "rewind (repeat = faster)",
        "reculer (répéter = + vite)",
    ),
    b(
        &[Key::ch('s')],
        Action::Stop,
        Group::Playback,
        "stop",
        "arrêter",
    ),
    b(
        &[Key::ch('+'), Key::ch('=')],
        Action::VolumeUp,
        Group::Playback,
        "volume up",
        "monter le volume",
    ),
    b(
        &[Key::ch('-')],
        Action::VolumeDown,
        Group::Playback,
        "volume down",
        "baisser le volume",
    ),
    // --- File & historique ---
    b(
        &[Key::ch('a')],
        Action::Enqueue,
        Group::Lists,
        "add selection to queue",
        "ajouter à la file",
    ),
    b(
        &[Key::ch('x')],
        Action::RemoveFromList,
        Group::Lists,
        "remove (queue / history)",
        "retirer (file/historique)",
    ),
    b(
        &[Key::ch('X')],
        Action::ClearList,
        Group::Lists,
        "clear (queue / history)",
        "vider (file/historique)",
    ),
    // --- Application ---
    b(
        &[Key::ch('/')],
        Action::BeginSearch,
        Group::App,
        "search both platforms",
        "chercher sur les deux",
    ),
    b(
        &[Key::ch(':')],
        Action::BeginCommand,
        Group::App,
        "paste a URL to play",
        "coller une URL à lire",
    ),
    b(
        &[Key::ch('c')],
        Action::BeginConnect,
        Group::App,
        "connect accounts",
        "connecter des comptes",
    ),
    b(
        &[Key::ch('v')],
        Action::CycleViz,
        Group::App,
        "cycle visualizer",
        "changer de visualiseur",
    ),
    b(
        &[Key::ch('L')],
        Action::ToggleLang,
        Group::App,
        "language EN / FR",
        "langue EN / FR",
    ),
    b(
        &[Key::ch('?')],
        Action::ToggleHelp,
        Group::App,
        "this help",
        "cette aide",
    ),
    b(
        &[Key::ch('q'), Key::ctrl('c')],
        Action::Quit,
        Group::App,
        "quit",
        "quitter",
    ),
];

/// Action associée à un événement clavier (mode normal), s'il y en a une.
pub fn action_for(ev: &KeyEvent) -> Option<Action> {
    BINDINGS
        .iter()
        .find(|b| b.keys.iter().any(|k| k.matches(ev)))
        .map(|b| b.action)
}

/// Raccourcis d'une rubrique, dans l'ordre de la table.
pub fn in_group(group: Group) -> impl Iterator<Item = &'static Binding> {
    BINDINGS.iter().filter(move |b| b.group == group)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ev(code: KeyCode, mods: KeyModifiers) -> KeyEvent {
        KeyEvent::new(code, mods)
    }

    #[test]
    fn chaque_action_est_atteignable_et_sans_conflit() {
        // Aucune touche n'est liée deux fois.
        let mut seen: Vec<Key> = Vec::new();
        for b in BINDINGS {
            for k in b.keys {
                assert!(!seen.contains(k), "touche liée deux fois : {k:?}");
                seen.push(*k);
            }
        }
    }

    #[test]
    fn dispatch_ignore_majuscule_mais_pas_controle() {
        assert_eq!(
            action_for(&ev(KeyCode::Char('G'), KeyModifiers::SHIFT)),
            Some(Action::Bottom)
        );
        assert_eq!(
            action_for(&ev(KeyCode::Char('d'), KeyModifiers::CONTROL)),
            Some(Action::PageDown)
        );
        // 'd' seul n'est pas lié.
        assert_eq!(
            action_for(&ev(KeyCode::Char('d'), KeyModifiers::NONE)),
            None
        );
        assert_eq!(
            action_for(&ev(KeyCode::Char('c'), KeyModifiers::CONTROL)),
            Some(Action::Quit)
        );
        assert_eq!(
            action_for(&ev(KeyCode::Char('c'), KeyModifiers::NONE)),
            Some(Action::BeginConnect)
        );
    }

    #[test]
    fn libelles_lisibles() {
        let down = BINDINGS.iter().find(|b| b.action == Action::Down).unwrap();
        assert_eq!(down.keys_label(), "j ↓");
        let page = BINDINGS
            .iter()
            .find(|b| b.action == Action::PageDown)
            .unwrap();
        assert_eq!(page.keys_label(), "^d pgdn");
        let space = BINDINGS
            .iter()
            .find(|b| b.action == Action::PlayPause)
            .unwrap();
        assert_eq!(space.keys_label(), "space");
        assert_eq!(space.help(Lang::Fr), "lecture / pause");
    }

    #[test]
    fn descriptions_courtes_pour_tenir_dans_l_aide_a_80_colonnes() {
        for b in BINDINGS {
            for lang in [Lang::En, Lang::Fr] {
                let n = b.help(lang).chars().count();
                assert!(n <= 26, "« {} » fait {n} caractères", b.help(lang));
            }
            assert!(b.keys_label().chars().count() <= 9, "{}", b.keys_label());
        }
    }

    #[test]
    fn chaque_rubrique_a_des_raccourcis() {
        for g in [Group::Navigate, Group::Playback, Group::Lists, Group::App] {
            assert!(in_group(g).next().is_some(), "{g:?} vide");
        }
    }
}

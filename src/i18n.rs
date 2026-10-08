//! Langue de l'interface : anglais par défaut, français au choix (bascule
//! avec `L` ou un clic dans la barre latérale, persisté dans la config).
//!
//! Les libellés propres à un type (`Section`, `Filter`, `VizMode`) restent
//! définis dans `app.rs`, à côté de leur enum, et les descriptions des
//! raccourcis vivent dans `keymap.rs` ; ce module regroupe le reste des
//! chaînes affichées par l'UI.

use serde::{Deserialize, Serialize};

use crate::app::Section;
use crate::model::Platform;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Lang {
    #[default]
    En,
    Fr,
}

impl Lang {
    pub fn toggle(self) -> Lang {
        match self {
            Lang::En => Lang::Fr,
            Lang::Fr => Lang::En,
        }
    }

    /// Code court affiché dans la barre latérale ("EN"/"FR").
    pub fn code(self) -> &'static str {
        match self {
            Lang::En => "EN",
            Lang::Fr => "FR",
        }
    }
}

pub fn resize_hint(lang: Lang) -> &'static str {
    match lang {
        Lang::En => "waveline: enlarge the terminal",
        Lang::Fr => "waveline : agrandis le terminal",
    }
}

pub fn welcome(lang: Lang) -> String {
    match lang {
        Lang::En => "Welcome — 'c' connect an account · ':' URL · '/' search · '?' help".into(),
        Lang::Fr => {
            "Bienvenue — 'c' connecter un compte · ':' URL · '/' recherche · '?' aide".into()
        }
    }
}

pub fn viz_changed(lang: Lang, viz_label: &str) -> String {
    match lang {
        Lang::En => format!("Visualizer: {viz_label}"),
        Lang::Fr => format!("Visualiseur : {viz_label}"),
    }
}

pub fn lang_changed(lang: Lang) -> String {
    match lang {
        Lang::En => "Language: English".into(),
        Lang::Fr => "Langue : Français".into(),
    }
}

pub fn loading(lang: Lang) -> String {
    match lang {
        Lang::En => "Loading…".into(),
        Lang::Fr => "Chargement…".into(),
    }
}

pub fn loading_section(lang: Lang, section_label: &str) -> String {
    match lang {
        Lang::En => format!("Loading {section_label}…"),
        Lang::Fr => format!("Chargement de {section_label}…"),
    }
}

pub fn no_account(lang: Lang) -> String {
    match lang {
        Lang::En => "No account connected — press 'c' to enter your handles".into(),
        Lang::Fr => "Aucun compte connecté — appuie sur 'c' pour entrer tes pseudos".into(),
    }
}

pub fn volume(lang: Lang, v: u8) -> String {
    match lang {
        Lang::En => format!("Volume: {v}%"),
        Lang::Fr => format!("Volume : {v}%"),
    }
}

pub fn filter_changed(lang: Lang, filter_label: &str) -> String {
    match lang {
        Lang::En => format!("Filter: {filter_label}"),
        Lang::Fr => format!("Filtre : {filter_label}"),
    }
}

pub fn url_unrecognized(lang: Lang) -> String {
    match lang {
        Lang::En => "URL not recognized (SoundCloud or Mixcloud)".into(),
        Lang::Fr => "URL non reconnue (SoundCloud ou Mixcloud)".into(),
    }
}

pub fn searching(lang: Lang, q: &str) -> String {
    match lang {
        Lang::En => format!("Search: \"{q}\"…"),
        Lang::Fr => format!("Recherche : « {q} »…"),
    }
}

pub fn accounts_status(lang: Lang, sc: &str, mc: &str) -> String {
    match lang {
        Lang::En => {
            format!("Accounts — SoundCloud: {sc} · Mixcloud: {mc}  (opens Likes/Playlists/Feed)")
        }
        Lang::Fr => {
            format!("Comptes — SoundCloud : {sc} · Mixcloud : {mc}  (ouvre Likes/Playlists/Feed)")
        }
    }
}

pub fn no_results(lang: Lang) -> String {
    match lang {
        Lang::En => "No results".into(),
        Lang::Fr => "Aucun résultat".into(),
    }
}

pub fn n_results(lang: Lang, n: usize) -> String {
    match lang {
        Lang::En => format!("{n} results"),
        Lang::Fr => format!("{n} résultats"),
    }
}

/// Message de saut dans le morceau (`⏪ −30s` / `⏩ +1min`).
pub fn seek(_lang: Lang, delta_ms: i64) -> String {
    let secs = delta_ms.unsigned_abs() / 1000;
    let mag = if secs >= 60 {
        format!("{}min", secs / 60)
    } else {
        format!("{secs}s")
    };
    if delta_ms < 0 {
        format!("⏪ −{mag}")
    } else {
        format!("⏩ +{mag}")
    }
}

pub fn seek_unavailable(lang: Lang) -> String {
    match lang {
        Lang::En => "Seek unavailable on this stream".into(),
        Lang::Fr => "Saut indisponible sur ce flux".into(),
    }
}

pub fn playback_stopped(lang: Lang) -> String {
    match lang {
        Lang::En => "Playback stopped".into(),
        Lang::Fr => "Lecture arrêtée".into(),
    }
}

/// Titre de la fenêtre d'aide.
pub fn help_title(lang: Lang) -> &'static str {
    match lang {
        Lang::En => "Help",
        Lang::Fr => "Aide",
    }
}

/// Rubrique souris de la fenêtre d'aide.
pub fn help_mouse_title(lang: Lang) -> &'static str {
    match lang {
        Lang::En => "Mouse",
        Lang::Fr => "Souris",
    }
}

/// Gestes souris : (geste, effet).
pub fn help_mouse_items(lang: Lang) -> &'static [(&'static str, &'static str)] {
    match lang {
        Lang::En => &[
            ("click", "play / open / tabs"),
            ("bar", "click progress to seek"),
            ("wheel", "scroll · on playbar: volume"),
        ],
        Lang::Fr => &[
            ("clic", "jouer / ouvrir / onglets"),
            ("barre", "cliquer pour sauter"),
            ("molette", "défiler · sur la barre : vol"),
        ],
    }
}

/// Pied de la fenêtre d'aide.
pub fn help_close(lang: Lang) -> &'static str {
    match lang {
        Lang::En => "any key to close",
        Lang::Fr => "une touche pour fermer",
    }
}

pub fn keys_bar(lang: Lang) -> &'static str {
    match lang {
        Lang::En => {
            "[space] play  [h/l] seek  [n/p] track  [a] queue  [/] search  [:] url  [tab] focus  [?] help  [q] quit"
        }
        Lang::Fr => {
            "[space] play  [h/l] saut  [n/p] piste  [a] file  [/] rech  [:] url  [tab] focus  [?] aide  [q] quitter"
        }
    }
}

/// Indication affichée dans une liste vide, selon la section.
pub fn empty_hint(lang: Lang, section: Section, has_account: bool) -> &'static str {
    match (section, lang) {
        (Section::Queue, Lang::En) => "  (queue is empty — press 'a' on a track to add it)",
        (Section::Queue, Lang::Fr) => {
            "  (file vide — appuie sur 'a' sur un morceau pour l'ajouter)"
        }
        (Section::History, Lang::En) => {
            "  (nothing played yet — tracks you play will show up here)"
        }
        (Section::History, Lang::Fr) => {
            "  (rien d'écouté encore — les morceaux joués apparaîtront ici)"
        }
        (Section::Search, Lang::En) => "  (empty — paste a URL with : or start a search with /)",
        (Section::Search, Lang::Fr) => {
            "  (vide — colle une URL avec : ou lance une recherche avec /)"
        }
        (_, Lang::En) if !has_account => "  (no account — press 'c' to enter your public handles)",
        (_, Lang::Fr) if !has_account => {
            "  (aucun compte — appuie sur 'c' pour entrer tes pseudos publics)"
        }
        (_, Lang::En) => "  (empty — press enter on the section to load it)",
        (_, Lang::Fr) => "  (vide — appuie sur entrée sur la section pour la charger)",
    }
}

pub fn queued(lang: Lang, title: &str, n: usize) -> String {
    match lang {
        Lang::En => format!("Queued: {title}  ({n} in queue)"),
        Lang::Fr => format!("Ajouté à la file : {title}  ({n} en attente)"),
    }
}

pub fn already_queue(lang: Lang) -> String {
    match lang {
        Lang::En => "Already in the queue — 'x' removes, enter plays".into(),
        Lang::Fr => "Déjà dans la file — 'x' retire, entrée joue".into(),
    }
}

pub fn queue_empty(lang: Lang) -> String {
    match lang {
        Lang::En => "Queue is empty — press 'a' on a track to add it".into(),
        Lang::Fr => "File vide — appuie sur 'a' sur un morceau pour l'ajouter".into(),
    }
}

pub fn history_empty(lang: Lang) -> String {
    match lang {
        Lang::En => "No listening history yet".into(),
        Lang::Fr => "Pas encore d'historique d'écoute".into(),
    }
}

pub fn removed(lang: Lang, title: &str) -> String {
    match lang {
        Lang::En => format!("Removed: {title}"),
        Lang::Fr => format!("Retiré : {title}"),
    }
}

pub fn cleared(lang: Lang, section_name: &str) -> String {
    match lang {
        Lang::En => format!("{section_name} cleared"),
        Lang::Fr => format!("{section_name} : vidé"),
    }
}

pub fn edit_hint(lang: Lang) -> String {
    match lang {
        Lang::En => "'x' removes and 'X' clears — in Queue or History only".into(),
        Lang::Fr => "'x' retire et 'X' vide — dans File ou Historique seulement".into(),
    }
}

pub fn end_of_list(lang: Lang) -> String {
    match lang {
        Lang::En => "End of list".into(),
        Lang::Fr => "Fin de la liste".into(),
    }
}

pub fn start_of_list(lang: Lang) -> String {
    match lang {
        Lang::En => "Start of list".into(),
        Lang::Fr => "Début de la liste".into(),
    }
}

pub fn playing_from_queue(lang: Lang, title: &str) -> String {
    match lang {
        Lang::En => format!("▶ From queue: {title}"),
        Lang::Fr => format!("▶ Depuis la file : {title}"),
    }
}

pub fn restarted(lang: Lang) -> String {
    match lang {
        Lang::En => "⏮ Restarting track".into(),
        Lang::Fr => "⏮ Reprise au début".into(),
    }
}

/// Bilan d'une requête : nombre de résultats + plateformes en échec.
pub fn fetch_summary(lang: Lang, n: usize, failures: &[(Platform, String)]) -> String {
    let mut s = if n == 0 {
        no_results(lang)
    } else {
        n_results(lang, n)
    };
    for (p, e) in failures {
        match lang {
            Lang::En => s.push_str(&format!(" · {p} failed: {e}")),
            Lang::Fr => s.push_str(&format!(" · {p} en échec : {e}")),
        }
    }
    s
}

pub fn nothing_playing(lang: Lang) -> &'static str {
    match lang {
        Lang::En => " Nothing playing ",
        Lang::Fr => " Rien en lecture ",
    }
}

pub fn loading_playbar(lang: Lang) -> &'static str {
    match lang {
        Lang::En => " ⏳ Loading…",
        Lang::Fr => " ⏳ Chargement…",
    }
}

pub fn accounts_hint(lang: Lang) -> &'static str {
    match lang {
        Lang::En => " accounts  (c)",
        Lang::Fr => " comptes  (c)",
    }
}

/// Ligne cliquable de bascule de langue, en bas de la sidebar.
pub fn lang_hint(lang: Lang) -> String {
    match lang {
        Lang::En => format!(" language {} (L)", lang.code()),
        Lang::Fr => format!(" langue {} (L)", lang.code()),
    }
}

pub fn prompt_command_hint(lang: Lang) -> &'static str {
    match lang {
        Lang::En => "paste a SoundCloud/Mixcloud URL · enter to play · esc to cancel",
        Lang::Fr => "colle une URL SoundCloud/Mixcloud · entrée pour jouer · échap pour annuler",
    }
}

pub fn prompt_search_hint(lang: Lang) -> &'static str {
    match lang {
        Lang::En => "type your search · enter to search · esc to cancel",
        Lang::Fr => "tape ta recherche · entrée pour chercher · échap pour annuler",
    }
}

pub fn prompt_connect_sc_hint(lang: Lang) -> &'static str {
    match lang {
        Lang::En => "your SoundCloud handle (empty = none) · enter → Mixcloud · esc cancels",
        Lang::Fr => "ton pseudo SoundCloud (vide = aucun) · entrée → Mixcloud · échap annule",
    }
}

pub fn prompt_connect_mc_hint(lang: Lang) -> &'static str {
    match lang {
        Lang::En => "your Mixcloud handle (empty = none) · enter to confirm · esc cancels",
        Lang::Fr => "ton pseudo Mixcloud (vide = aucun) · entrée pour valider · échap annule",
    }
}

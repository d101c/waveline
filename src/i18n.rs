//! Langue de l'interface : anglais par défaut, français au choix (bascule
//! avec `L` ou un clic dans la barre latérale, persisté dans la config).
//!
//! Les libellés propres à un type (`Section`, `Filter`, `VizMode`) restent
//! définis dans `app.rs`, à côté de leur enum ; ce module regroupe le reste
//! des chaînes affichées par l'UI.

use serde::{Deserialize, Serialize};

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

pub fn section_soon(lang: Lang, section_label: &str) -> String {
    match lang {
        Lang::En => format!("{section_label} — soon"),
        Lang::Fr => format!("{section_label} — bientôt"),
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

pub fn help_text(lang: Lang) -> String {
    match lang {
        Lang::En => "Help: 'c' accounts · 'v' visualizer · 'L' language · ':' URL · '/' search · j/k navigate · h/l (←/→) seek ±10s · tab focus · enter/click play · space pause · n/p track · s stop · 1/2/3 filter · q quit".into(),
        Lang::Fr => "Aide : 'c' comptes · 'v' visualiseur · 'L' langue · ':' URL · '/' rech · j/k naviguer · h/l (←/→) saut ±10s · tab focus · enter/clic jouer · space pause · n/p piste · s stop · 1/2/3 filtre · q quitter".into(),
    }
}

pub fn keys_bar(lang: Lang) -> &'static str {
    match lang {
        Lang::En => {
            "[space] play  [h/l] seek  [n/p] track  [:] url  [/] search  [tab] focus  [1·2·3] filter  [L] lang  [?] help:q"
        }
        Lang::Fr => {
            "[space] play  [h/l] saut  [n/p] piste  [:] url  [/] rech  [tab] focus  [1·2·3] filtre  [L] langue  [?] aide:q"
        }
    }
}

pub fn empty_list_hint(lang: Lang) -> &'static str {
    match lang {
        Lang::En => "  (empty — paste a URL with : or start a search with /)",
        Lang::Fr => "  (vide — colle une URL avec : ou lance une recherche avec /)",
    }
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

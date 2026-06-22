//! Rendu de l'interface + cartographie des zones cliquables.
//!
//! `draw` retourne un [`Regions`] qui mémorise où chaque élément interactif a
//! été dessiné, afin que la boucle d'événements puisse traduire un clic
//! (colonne, ligne) en [`Action`](crate::app::Action) ou en sélection.

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Block, BorderType, Borders, Cell, LineGauge, Paragraph, Row, Table, TableState,
};
use ratatui::Frame;

use crate::app::{App, Filter, Focus, Input, Section};
use crate::model::{fmt_duration, Platform};
use crate::theme::Theme;

/// Zones interactives mémorisées au dernier rendu, pour le hit-test souris.
#[derive(Default, Clone)]
pub struct Regions {
    pub sidebar_rows: Vec<Rect>,
    /// (rect de l'onglet, filtre associé)
    pub filter_tabs: Vec<(Rect, Filter)>,
    /// Zone interne de la liste (sans bordure) + premier index visible.
    pub list_inner: Rect,
    pub list_first: usize,
    pub list_len: usize,
    /// Bouton play/pause de la barre de lecture.
    pub playpause_btn: Rect,
}

impl Regions {
    /// Retrouve la section cliquée dans la sidebar.
    pub fn section_at(&self, x: u16, y: u16) -> Option<usize> {
        self.sidebar_rows
            .iter()
            .position(|r| contains(r, x, y))
    }

    /// Retrouve le filtre dont l'onglet a été cliqué.
    pub fn filter_at(&self, x: u16, y: u16) -> Option<Filter> {
        self.filter_tabs
            .iter()
            .find(|(r, _)| contains(r, x, y))
            .map(|(_, f)| *f)
    }

    /// Index (dans la liste filtrée) de la ligne cliquée.
    pub fn list_row_at(&self, x: u16, y: u16) -> Option<usize> {
        if !contains(&self.list_inner, x, y) {
            return None;
        }
        let offset = (y - self.list_inner.y) as usize + self.list_first;
        if offset < self.list_len {
            Some(offset)
        } else {
            None
        }
    }

    pub fn playpause_at(&self, x: u16, y: u16) -> bool {
        contains(&self.playpause_btn, x, y)
    }
}

fn contains(r: &Rect, x: u16, y: u16) -> bool {
    x >= r.x && x < r.x + r.width && y >= r.y && y < r.y + r.height
}

pub fn draw(f: &mut Frame, app: &App, theme: &Theme) -> Regions {
    let mut regions = Regions::default();

    // Terminal dégénéré (trop petit / taille nulle) : on évite tout rendu qui
    // indexerait hors du buffer, et on invite à agrandir si la place le permet.
    let area = f.area();
    if area.width < 24 || area.height < 12 {
        if area.width >= 1 && area.height >= 1 {
            f.render_widget(
                Paragraph::new("waveline : agrandis le terminal")
                    .style(Style::default().fg(theme.fg)),
                area,
            );
        }
        return regions;
    }

    let root = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // header
            Constraint::Min(3),    // corps
            Constraint::Length(6), // barre de lecture + analyseur
            Constraint::Length(1), // ligne de statut + raccourcis
        ])
        .split(f.area());

    draw_header(f, root[0], app, theme, &mut regions);

    let body = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(24), Constraint::Min(20)])
        .split(root[1]);

    draw_sidebar(f, body[0], app, theme, &mut regions);
    draw_list(f, body[1], app, theme, &mut regions);
    draw_playbar(f, root[2], app, theme, &mut regions);
    draw_status(f, root[3], app, theme);

    regions
}

fn draw_header(f: &mut Frame, area: Rect, app: &App, theme: &Theme, reg: &mut Regions) {
    // Titre à gauche.
    let title = Span::styled(
        " waveline ",
        Style::default().fg(theme.bg).bg(theme.accent).add_modifier(Modifier::BOLD),
    );
    f.render_widget(Paragraph::new(Line::from(title)), area);

    // Onglets de filtre à droite : [Tout] [SC] [MC].
    let tabs = [
        ("Tout", Filter::All),
        ("SC", Filter::Only(Platform::SoundCloud)),
        ("MC", Filter::Only(Platform::Mixcloud)),
    ];
    // Calcule la largeur totale puis pose les onglets en partant de la droite.
    let labels: Vec<String> = tabs.iter().map(|(t, _)| format!(" {t} ")).collect();
    let total: u16 = labels.iter().map(|s| s.chars().count() as u16).sum::<u16>()
        + (tabs.len() as u16 - 1);
    let mut x = area.x + area.width.saturating_sub(total);
    for (i, (_, filt)) in tabs.iter().enumerate() {
        let w = labels[i].chars().count() as u16;
        let r = Rect::new(x, area.y, w, 1);
        let active = *filt == app.filter;
        let style = if active {
            Style::default().fg(theme.bg).bg(theme.accent).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme.dim)
        };
        f.render_widget(Paragraph::new(Span::styled(labels[i].clone(), style)), r);
        reg.filter_tabs.push((r, *filt));
        x += w + 1;
    }
}

fn draw_sidebar(f: &mut Frame, area: Rect, app: &App, theme: &Theme, reg: &mut Regions) {
    let active = app.focus == Focus::Sidebar;
    let block = panel_block("Sources", active, theme);
    let inner = block.inner(area);
    f.render_widget(block, area);

    for (i, section) in Section::ALL.iter().enumerate() {
        let y = inner.y + i as u16;
        if y >= inner.y + inner.height {
            break;
        }
        let row = Rect::new(inner.x, y, inner.width, 1);
        let selected = i == app.section_index;
        let mut style = Style::default().fg(theme.fg);
        if selected {
            style = style.bg(theme.selection_bg).add_modifier(Modifier::BOLD);
            if active {
                style = style.fg(theme.accent);
            }
        }
        let label = format!(" {}", section.label());
        f.render_widget(Paragraph::new(Span::styled(label, style)), row);
        reg.sidebar_rows.push(row);
    }
}

fn draw_list(f: &mut Frame, area: Rect, app: &App, theme: &Theme, reg: &mut Regions) {
    let active = app.focus == Focus::List;
    let title = format!("{}  ·  {}", app.section.label().trim(), app.filter.label());
    let block = panel_block(&title, active, theme);
    let inner = block.inner(area);
    f.render_widget(&block, area);

    let visible = app.visible_indices();
    reg.list_inner = inner;
    reg.list_len = visible.len();

    if visible.is_empty() {
        let hint = Paragraph::new(Line::from(Span::styled(
            "  (vide — colle une URL avec : ou lance une recherche avec /)",
            Style::default().fg(theme.dim),
        )));
        f.render_widget(hint, inner);
        return;
    }

    // Fenêtre de défilement simple centrée sur la sélection.
    let height = inner.height as usize;
    let first = scroll_first(app.list_index, visible.len(), height);
    reg.list_first = first;

    let rows: Vec<Row> = visible
        .iter()
        .enumerate()
        .skip(first)
        .take(height)
        .map(|(vis_i, &track_i)| {
            let t = &app.tracks[track_i];
            let is_current = app
                .playback
                .current
                .as_ref()
                .map(|c| c.id == t.id && c.platform == t.platform)
                .unwrap_or(false);
            let marker = if is_current {
                if app.playback.playing { "▶ " } else { "⏸ " }
            } else {
                "  "
            };
            let title_style = if is_current {
                Style::default().fg(theme.playing).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme.fg)
            };
            let plat = Span::styled(
                t.platform.tag(),
                Style::default().fg(theme.platform(t.platform)).add_modifier(Modifier::BOLD),
            );
            let row = Row::new(vec![
                Cell::from(Span::styled(format!("{marker}{}", t.title), title_style)),
                Cell::from(Span::styled(t.artist.clone(), Style::default().fg(theme.dim))),
                Cell::from(Span::styled(
                    t.duration_human(),
                    Style::default().fg(theme.dim),
                )),
                Cell::from(plat),
            ]);
            let _ = vis_i;
            row
        })
        .collect();

    let widths = [
        Constraint::Percentage(50),
        Constraint::Percentage(34),
        Constraint::Length(8),
        Constraint::Length(3),
    ];
    let mut state = TableState::default();
    state.select(Some(app.list_index.saturating_sub(first)));
    let table = Table::new(rows, widths)
        .row_highlight_style(
            Style::default()
                .bg(theme.selection_bg)
                .add_modifier(Modifier::BOLD),
        )
        .column_spacing(1);
    f.render_stateful_widget(table, inner, &mut state);
}

fn draw_playbar(f: &mut Frame, area: Rect, app: &App, theme: &Theme, reg: &mut Regions) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.border));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // morceau + volume
            Constraint::Min(1),    // analyseur de spectre
            Constraint::Length(1), // progression
        ])
        .split(inner);

    let pb = &app.playback;
    let (icon, line) = if pb.loading {
        (
            "⏳",
            Line::from(Span::styled(
                " ⏳ Chargement…",
                Style::default().fg(theme.accent).add_modifier(Modifier::BOLD),
            )),
        )
    } else {
        match &pb.current {
        Some(t) => {
            let icon = if pb.playing { "▶" } else { "⏸" };
            let line = Line::from(vec![
                Span::styled(
                    format!(" {icon} "),
                    Style::default().fg(theme.playing).add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!("{} — {}", t.artist, t.title),
                    Style::default().fg(theme.fg).add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!("   vol {}%", pb.volume),
                    Style::default().fg(theme.dim),
                ),
            ]);
            (icon, line)
        }
        None => (
            "·",
            Line::from(Span::styled(
                " Rien en lecture ",
                Style::default().fg(theme.dim),
            )),
        ),
        }
    };
    let _ = icon;
    f.render_widget(Paragraph::new(line), rows[0]);
    // Le bouton play/pause = les 3 premières colonnes de la première ligne.
    reg.playpause_btn = Rect::new(rows[0].x, rows[0].y, 3, 1);

    // Barre de progression.
    let dur = pb.current.as_ref().and_then(|t| t.duration_ms).unwrap_or(0);
    let ratio = if dur > 0 {
        (pb.position_ms as f64 / dur as f64).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let label = format!(
        "{} / {}",
        fmt_duration(pb.position_ms),
        if dur > 0 { fmt_duration(dur) } else { "--:--".into() }
    );
    let gauge = LineGauge::default()
        .filled_style(Style::default().fg(theme.accent))
        .unfilled_style(Style::default().fg(theme.border))
        .ratio(ratio)
        .label(Span::styled(label, Style::default().fg(theme.dim)));

    // Analyseur de spectre au milieu, progression en bas.
    draw_spectrum(f, rows[1], &pb.spectrum);
    f.render_widget(gauge, rows[2]);
}

/// Dessine l'analyseur de spectre : une barre verticale par bande, en blocs.
fn draw_spectrum(f: &mut Frame, area: Rect, bands: &[f32]) {
    if area.width == 0 || area.height == 0 || bands.is_empty() {
        return;
    }
    const BLOCKS: [char; 9] = [' ', '▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];
    const PER_ROW: usize = 8;
    let n = bands.len();
    let bar_w = (area.width as usize / n).max(1);
    let total_levels = area.height as usize * PER_ROW;

    for row in 0..area.height {
        // La ligne du haut correspond aux niveaux les plus élevés.
        let from_bottom = (area.height - 1 - row) as usize;
        let base = from_bottom * PER_ROW;
        let mut spans: Vec<Span> = Vec::with_capacity(n);
        for (i, &v) in bands.iter().enumerate() {
            let filled = (v.clamp(0.0, 1.0) * total_levels as f32).round() as usize;
            let lvl = filled.saturating_sub(base).min(PER_ROW);
            let s: String = std::iter::repeat(BLOCKS[lvl]).take(bar_w).collect();
            spans.push(Span::styled(s, Style::default().fg(band_color(i, n))));
        }
        let y = area.y + row;
        f.render_widget(
            Paragraph::new(Line::from(spans)),
            Rect::new(area.x, y, area.width, 1),
        );
    }
}

/// Dégradé vert (graves) → rouge (aigus) pour colorer les bandes.
fn band_color(i: usize, n: usize) -> ratatui::style::Color {
    let t = if n <= 1 { 0.0 } else { i as f32 / (n - 1) as f32 };
    let r = (40.0 + t * 215.0) as u8;
    let g = (220.0 - t * 150.0) as u8;
    let b = (120.0 - t * 80.0) as u8;
    ratatui::style::Color::Rgb(r, g, b)
}

fn draw_status(f: &mut Frame, area: Rect, app: &App, theme: &Theme) {
    // En mode saisie : invite (« : » URL ou « / » recherche) + tampon + curseur.
    let prompt_hint = match &app.input {
        Input::Command(buf) => Some((
            ":",
            buf,
            "colle une URL SoundCloud/Mixcloud · entrée pour jouer · échap pour annuler",
        )),
        Input::Search(buf) => Some((
            "/",
            buf,
            "tape ta recherche · entrée pour chercher · échap pour annuler",
        )),
        Input::Normal => None,
    };
    if let Some((prompt, buf, hint)) = prompt_hint {
        let line = Line::from(vec![
            Span::styled(
                format!(" {prompt} "),
                Style::default().fg(theme.bg).bg(theme.accent).add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!(" {buf}▏"),
                Style::default().fg(theme.fg).add_modifier(Modifier::BOLD),
            ),
            Span::styled(format!("  ({hint})"), Style::default().fg(theme.dim)),
        ]);
        f.render_widget(Paragraph::new(line), area);
        return;
    }
    let keys = "[space] play  [n/p] suiv/préc  [:] url  [/] rech  [tab] focus  [1·2·3] filtre  [?] quitter:q";
    let line = Line::from(vec![
        Span::styled(format!(" {} ", app.status), Style::default().fg(theme.fg)),
        Span::styled(format!("  {keys}"), Style::default().fg(theme.dim)),
    ]);
    f.render_widget(Paragraph::new(line), area);
}

/// Bloc encadré standard, surligné quand le panneau a le focus.
fn panel_block(title: &str, active: bool, theme: &Theme) -> Block<'static> {
    let border = if active {
        theme.border_active
    } else {
        theme.border
    };
    Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(border))
        .title(Span::styled(
            format!(" {title} "),
            Style::default()
                .fg(if active { theme.accent } else { theme.fg })
                .add_modifier(Modifier::BOLD),
        ))
}

/// Détermine le premier index visible pour garder la sélection à l'écran.
fn scroll_first(selected: usize, total: usize, height: usize) -> usize {
    if total <= height || height == 0 {
        return 0;
    }
    let half = height / 2;
    let max_first = total - height;
    selected.saturating_sub(half).min(max_first)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scroll_garde_la_selection_visible() {
        // En haut : pas de décalage.
        assert_eq!(scroll_first(0, 100, 10), 0);
        // Au milieu : centré.
        assert_eq!(scroll_first(50, 100, 10), 45);
        // En bas : borné à max_first.
        assert_eq!(scroll_first(99, 100, 10), 90);
        // Liste plus courte que la fenêtre.
        assert_eq!(scroll_first(3, 5, 10), 0);
    }

    #[test]
    fn contains_teste_les_bornes() {
        let r = Rect::new(2, 3, 4, 2);
        assert!(contains(&r, 2, 3));
        assert!(contains(&r, 5, 4));
        assert!(!contains(&r, 6, 3));
        assert!(!contains(&r, 2, 5));
    }

    /// Concatène tous les symboles du buffer en une chaîne pour les assertions.
    fn buffer_text(term: &ratatui::Terminal<ratatui::backend::TestBackend>) -> String {
        term.backend()
            .buffer()
            .content()
            .iter()
            .map(|c| c.symbol())
            .collect()
    }

    #[test]
    fn rend_les_zones_principales() {
        use crate::app::App;
        use crate::model::{Platform, Track};
        use crate::theme::Theme;

        let mut app = App::new();
        app.tracks = vec![Track {
            platform: Platform::SoundCloud,
            id: "1".into(),
            title: "Mon Morceau Test".into(),
            artist: "Artiste Test".into(),
            permalink: "https://soundcloud.com/a/b".into(),
            duration_ms: Some(200_000),
        }];
        let theme = Theme::dark();

        let backend = ratatui::backend::TestBackend::new(110, 30);
        let mut term = ratatui::Terminal::new(backend).unwrap();
        term.draw(|f| {
            super::draw(f, &app, &theme);
        })
        .unwrap();

        let text = buffer_text(&term);
        assert!(text.contains("waveline"), "titre absent");
        assert!(text.contains("Sources"), "sidebar absente");
        assert!(text.contains("Likes"), "section Likes absente");
        assert!(text.contains("Mon Morceau Test"), "morceau absent");
        assert!(text.contains("Rien en lecture"), "barre de lecture absente");
    }

    #[test]
    fn ne_panique_pas_sur_terminal_minuscule() {
        use crate::app::App;
        use crate::theme::Theme;
        let app = App::new();
        let theme = Theme::dark();
        for (w, h) in [(0, 0), (1, 1), (5, 3), (19, 7)] {
            let backend = ratatui::backend::TestBackend::new(w.max(1), h.max(1));
            let mut term = ratatui::Terminal::new(backend).unwrap();
            // Ne doit pas paniquer même en taille dégénérée.
            term.draw(|f| {
                super::draw(f, &app, &theme);
            })
            .unwrap();
        }
    }
}

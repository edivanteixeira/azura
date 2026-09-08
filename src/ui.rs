use crate::app::*;
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Cell, Clear, Paragraph, Row, Table, TableState};

pub const ACCENT: Color = Color::Rgb(122, 162, 247);
pub const DIM: Color = Color::Rgb(110, 118, 140);
pub const BORDER: Color = Color::Rgb(60, 66, 90);
pub const OK: Color = Color::Rgb(126, 202, 154);
pub const BAD: Color = Color::Rgb(240, 113, 120);
pub const WARN: Color = Color::Rgb(224, 175, 104);
pub const PEND: Color = Color::Rgb(198, 146, 233);

const SPINNER: [&str; 8] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧"];

fn panel_titled(title: Line<'static>) -> Block<'static> {
    Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(Style::new().fg(BORDER))
        .title(title)
}

pub fn panel(title: &str) -> Block<'static> {
    panel_titled(Line::from(format!(" {title} ")).fg(DIM))
}

fn build_mark(status: &str, result: &str) -> (&'static str, Color) {
    match (status, result) {
        ("inProgress", _) => ("◐", WARN),
        ("notStarted" | "postponed", _) => ("○", ACCENT),
        ("cancelling", _) => ("◐", DIM),
        (_, "succeeded") => ("●", OK),
        (_, "partiallySucceeded") => ("◐", WARN),
        (_, "failed") => ("●", BAD),
        (_, "canceled") => ("○", DIM),
        _ => ("·", DIM),
    }
}

fn env_color(status: &str) -> Color {
    match status {
        "succeeded" => OK,
        "inProgress" | "queued" | "scheduled" => WARN,
        "rejected" | "canceled" => BAD,
        "partiallySucceeded" => WARN,
        _ => DIM,
    }
}

fn trunc(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_string()
    } else {
        let mut out: String = s.chars().take(n.saturating_sub(1)).collect();
        out.push('…');
        out
    }
}

pub fn draw(f: &mut Frame, app: &App) {
    let [head, body, status, keys] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(3),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .areas(f.area());

    header(f, head, app);
    match app.view {
        View::List => match app.tab {
            Tab::Prs => prs(f, body, app),
            Tab::Pipelines => pipelines(f, body, app),
            Tab::Releases => releases(f, body, app),
        },
        View::Diff => diff(f, body, app),
        View::Threads => threads(f, body, app),
        View::Timeline => timeline(f, body, app),
        View::Log => logs(f, body, app),
        View::Help => help(f, body),
    }
    status_bar(f, status, app);
    key_bar(f, keys, app);

    if let Some(m) = &app.modal {
        modal(f, f.area(), m);
    }
}

fn header(f: &mut Frame, area: Rect, app: &App) {
    let mut spans = vec![
        Span::styled(" azura ", Style::new().bg(ACCENT).fg(Color::Black).bold()),
        // a versão fica à vista: saber qual build está rodando não deveria
        // exigir sair do app
        Span::styled(
            format!(" {} ", env!("CARGO_PKG_VERSION")),
            Style::new().fg(DIM),
        ),
        Span::styled(
            format!("{}/{}  ", app.org, app.project),
            Style::new().fg(DIM),
        ),
    ];
    let counts = [
        app.prs.len(),
        if app.show_defs {
            app.defs.len()
        } else {
            app.builds.len()
        },
        app.rel_rows().len(),
    ];
    for (i, name) in ["PRs", "Pipelines", "Releases"].iter().enumerate() {
        // sem número antes de carregar: "(0)" numa aba não visitada é mentira
        let label = if app.loaded[i] {
            format!(" {} {name} ({}) ", i + 1, counts[i])
        } else {
            format!(" {} {name} ", i + 1)
        };
        spans.push(if app.tab.idx() == i {
            Span::styled(label, Style::new().fg(ACCENT).bold().underlined())
        } else {
            Span::styled(label, Style::new().fg(DIM))
        });
    }

    let mut right = vec![];
    if !app.approvals.is_empty() {
        right.push(Span::styled(
            format!(" ⚠ {} pending ", app.approvals.len()),
            Style::new().fg(PEND).bold(),
        ));
    }
    if app.client.dry_run {
        right.push(Span::styled(
            " dry-run ",
            Style::new().bg(WARN).fg(Color::Black),
        ));
    }
    right.push(Span::styled(
        if app.loading > 0 {
            format!(" {} ", SPINNER[app.spin % SPINNER.len()])
        } else {
            format!(" ⟳ {}s ", app.last_refresh.elapsed().as_secs())
        },
        Style::new().fg(DIM),
    ));

    let used: usize = right.iter().map(|s| s.content.chars().count()).sum();
    f.render_widget(Line::from(spans), area);
    let w = used.min(area.width as usize) as u16;
    let r = Rect::new(area.x + area.width.saturating_sub(w), area.y, w, 1);
    f.render_widget(Line::from(right), r);
}

fn table(
    f: &mut Frame,
    area: Rect,
    app: &App,
    title: &str,
    header: Row,
    widths: Vec<Constraint>,
    rows: Vec<Row>,
) {
    let empty = rows.is_empty();
    // filtro ativo precisa aparecer: senão uma lista filtrada parece uma lista vazia
    let heading = if app.filter.is_empty() {
        Line::from(format!(" {title} ")).fg(DIM)
    } else {
        Line::from(vec![
            Span::styled(format!(" {title} "), Style::new().fg(DIM)),
            Span::styled(format!("/{} ", app.filter), Style::new().fg(ACCENT).bold()),
            Span::styled(
                format!("{}/{} ", rows.len(), app.total_rows()),
                Style::new().fg(DIM),
            ),
        ])
    };
    let t = Table::new(rows, widths)
        .header(header.style(Style::new().fg(DIM).add_modifier(Modifier::BOLD)))
        .block(panel_titled(heading))
        .row_highlight_style(Style::new().bg(Color::Rgb(40, 46, 66)).bold())
        .highlight_symbol("▍");
    let mut state = TableState::default().with_selected(Some(app.cursor()));
    f.render_stateful_widget(t, area, &mut state);
    if empty {
        // o motivo real vem antes do "loading…": com refresh a cada 30s uma lista
        // filtrada e vazia ficava dizendo "loading…" para sempre
        let msg = if !app.filter.is_empty() {
            "nothing matches the filter · esc clears"
        } else if app.tab == Tab::Prs && app.scope != PrScope::All {
            app.scope.empty_msg()
        } else if app.loading > 0 {
            "loading…"
        } else {
            "nothing here"
        };
        f.render_widget(
            Paragraph::new(msg).fg(DIM),
            Rect::new(area.x + 2, area.y + 2, area.width.saturating_sub(4), 1),
        );
    }
}

fn prs(f: &mut Frame, area: Rect, app: &App) {
    let rows: Vec<Row> = app
        .visible_prs()
        .iter()
        .map(|&i| {
            let p = &app.prs[i];
            let (up, down) = p.votes();
            let my_vote = p
                .reviewers
                .iter()
                .find(|r| r.id == app.my_id)
                .map(|r| r.vote)
                .unwrap_or(0);
            let mark = if p.merge_status == "conflicts" {
                Span::styled("⚠", Style::new().fg(BAD))
            } else if down > 0 {
                Span::styled("●", Style::new().fg(BAD))
            } else if up > 0 {
                Span::styled("●", Style::new().fg(OK))
            } else {
                Span::styled("○", Style::new().fg(DIM))
            };
            let title = if p.is_draft {
                format!("[draft] {}", p.title)
            } else {
                p.title.clone()
            };
            let votes = format!("↑{up} ↓{down}");
            // o que trava o merge, que os votos sozinhos não dizem
            let (gate, gate_cor) = match app.gates.get(&p.pull_request_id) {
                Some(g) => (
                    g.label.clone(),
                    match g.kind {
                        GateKind::Ready => OK,
                        GateKind::Waiting => WARN,
                        GateKind::Blocked => BAD,
                    },
                ),
                None => ("·".into(), DIM),
            };
            Row::new(vec![
                Cell::from(Line::from(mark)),
                Cell::from(format!("!{}", p.pull_request_id)).fg(ACCENT),
                Cell::from(trunc(&p.repository.name, 14)).fg(DIM),
                Cell::from(trunc(&title, 60)),
                Cell::from(trunc(&p.created_by.display_name, 16)).fg(DIM),
                Cell::from(trunc(&gate, 11)).fg(gate_cor),
                Cell::from(votes).fg(if my_vote == 0 { WARN } else { DIM }),
                Cell::from(ago(&p.creation_date)).fg(DIM),
            ])
        })
        .collect();

    let head = Row::new(vec![
        "",
        "id",
        "repo",
        "title",
        "author",
        "blocked on",
        "votes",
        "age",
    ]);
    let widths = vec![
        Constraint::Length(1),
        Constraint::Length(6),
        Constraint::Length(14),
        Constraint::Min(20),
        Constraint::Length(16),
        Constraint::Length(11),
        Constraint::Length(7),
        Constraint::Length(4),
    ];
    table(f, area, app, app.scope.title(), head, widths, rows);
}

fn pipelines(f: &mut Frame, area: Rect, app: &App) {
    if app.show_defs {
        let rows: Vec<Row> = app
            .visible_defs()
            .iter()
            .map(|&i| {
                let d = &app.defs[i];
                Row::new(vec![
                    Cell::from(d.id.to_string()).fg(ACCENT),
                    Cell::from(d.name.clone()),
                    Cell::from(d.path.clone()).fg(DIM),
                    Cell::from(d.queue_status.clone()).fg(if d.queue_status == "enabled" {
                        DIM
                    } else {
                        WARN
                    }),
                ])
            })
            .collect();
        let head = Row::new(vec!["id", "pipeline", "folder", "state"]);
        let widths = vec![
            Constraint::Length(6),
            Constraint::Min(20),
            Constraint::Length(24),
            Constraint::Length(10),
        ];
        return table(f, area, app, "pipeline definitions", head, widths, rows);
    }

    let rows: Vec<Row> = app
        .visible_builds()
        .iter()
        .map(|&i| {
            let b = &app.builds[i];
            let (sym, color) = build_mark(&b.status, &b.result);
            let outcome = if b.status == "completed" {
                b.result.clone()
            } else {
                b.status.clone()
            };
            Row::new(vec![
                Cell::from(Line::from(Span::styled(sym, Style::new().fg(color)))),
                Cell::from(trunc(&b.definition.name, 24)),
                Cell::from(outcome).fg(color),
                Cell::from(trunc(&short_branch(&b.source_branch), 28)).fg(DIM),
                Cell::from(trunc(&b.requested_for.display_name, 18)).fg(DIM),
                Cell::from(dur(&b.start_time, &b.finish_time)).fg(DIM),
                Cell::from(ago(&b.queue_time)).fg(DIM),
            ])
        })
        .collect();
    let head = Row::new(vec!["", "pipeline", "state", "branch", "who", "dur", "age"]);
    let widths = vec![
        Constraint::Length(1),
        Constraint::Min(18),
        Constraint::Length(12),
        Constraint::Length(28),
        Constraint::Length(18),
        Constraint::Length(6),
        Constraint::Length(5),
    ];
    table(f, area, app, "recent runs", head, widths, rows);
}

fn releases(f: &mut Frame, area: Rect, app: &App) {
    let rows: Vec<Row> = app
        .rel_rows()
        .iter()
        .map(|row| match *row {
            RelRow::Approval(i) => {
                let a = &app.approvals[i];
                Row::new(vec![
                    Cell::from(Line::from(Span::styled("⚠", Style::new().fg(PEND)))),
                    Cell::from(trunc(&a.release_definition.name, 26)).fg(PEND),
                    Cell::from(a.release.name.clone()).fg(DIM),
                    Cell::from(format!("waiting: {}", a.release_environment.name))
                        .fg(PEND)
                        .bold(),
                    Cell::from(ago(&a.created_on)).fg(DIM),
                ])
            }
            RelRow::Release(i) => {
                let r = &app.releases[i];
                let stages: Vec<Span> = r
                    .environments
                    .iter()
                    .flat_map(|e| {
                        [
                            Span::styled(trunc(&e.name, 14), Style::new().fg(env_color(&e.status))),
                            Span::raw(" "),
                        ]
                    })
                    .collect();
                Row::new(vec![
                    Cell::from(Line::from(Span::styled("·", Style::new().fg(DIM)))),
                    Cell::from(trunc(&r.release_definition.name, 26)),
                    Cell::from(r.name.clone()).fg(DIM),
                    Cell::from(Line::from(stages)),
                    Cell::from(ago(&r.created_on)).fg(DIM),
                ])
            }
        })
        .collect();
    let head = Row::new(vec!["", "definition", "release", "stages", "age"]);
    let widths = vec![
        Constraint::Length(1),
        Constraint::Length(26),
        Constraint::Length(14),
        Constraint::Min(20),
        Constraint::Length(5),
    ];
    table(
        f,
        area,
        app,
        "releases · pending approvals pinned on top",
        head,
        widths,
        rows,
    );
}

fn diff(f: &mut Frame, area: Rect, app: &App) {
    let [left, right] =
        Layout::horizontal([Constraint::Length(42), Constraint::Min(20)]).areas(area);

    let title = app
        .pr
        .as_ref()
        .map(|p| {
            format!(
                "!{} {}  {} → {}",
                p.pull_request_id,
                trunc(&p.title, 30),
                short_branch(&p.source_ref_name),
                short_branch(&p.target_ref_name)
            )
        })
        .unwrap_or_else(|| "loading…".into());

    let items: Vec<Line> = app
        .changes
        .iter()
        .enumerate()
        .map(|(i, c)| {
            let (mark, color) = match c.change_type.as_str() {
                t if t.contains("add") => ("+", OK),
                t if t.contains("delete") => ("-", BAD),
                _ => ("~", WARN),
            };
            let name = c.item.path.trim_start_matches('/');
            let style = if i == app.change_sel {
                Style::new().bg(Color::Rgb(40, 46, 66)).bold()
            } else {
                Style::new().fg(DIM)
            };
            Line::from(vec![
                Span::styled(format!("{mark} "), Style::new().fg(color)),
                Span::styled(trunc(name, 36), style),
            ])
        })
        .collect();
    let visible = (left.height.saturating_sub(2)) as usize;
    let start = app.change_sel.saturating_sub(visible.saturating_sub(1));
    f.render_widget(
        Paragraph::new(items.into_iter().skip(start).collect::<Vec<_>>())
            .block(panel(&format!("{} files", app.changes.len()))),
        left,
    );

    let height = right.height.saturating_sub(2) as usize;
    let lines: Vec<Line> = app
        .diff
        .iter()
        .skip(app.diff_scroll)
        .take(height)
        .map(|d| {
            let (color, prefix) = match d.kind {
                '+' => (OK, "+"),
                '-' => (BAD, "-"),
                '@' => (ACCENT, ""),
                'i' => (DIM, ""),
                _ => (Color::Reset, " "),
            };
            Line::from(Span::styled(
                format!("{prefix}{}", d.text),
                Style::new().fg(color),
            ))
        })
        .collect();
    f.render_widget(Paragraph::new(lines).block(panel(&title)), right);
}

fn threads(f: &mut Frame, area: Rect, app: &App) {
    let mut lines: Vec<Line> = vec![];
    for (i, t) in app.threads.iter().enumerate() {
        let sel = i == app.thread_sel;
        let marca = if sel { "▍" } else { " " };
        let arquivo = t.thread_context.file_path.trim_start_matches('/');
        let cabecalho = if arquivo.is_empty() {
            format!("{marca} thread {}", t.id)
        } else {
            format!("{marca} {arquivo}")
        };
        lines.push(Line::from(vec![
            Span::styled(
                cabecalho,
                if sel {
                    Style::new().fg(ACCENT).bold()
                } else {
                    Style::new().fg(DIM)
                },
            ),
            Span::styled(
                format!("  {}", if t.status == "closed" { "resolved" } else { "" }),
                Style::new().fg(OK),
            ),
        ]));
        for c in t.comments.iter().filter(|c| c.comment_type != "system") {
            lines.push(Line::from(vec![
                Span::styled(
                    format!("    {} ", trunc(&c.author.display_name, 20)),
                    Style::new().fg(WARN),
                ),
                Span::styled(ago(&c.published_date), Style::new().fg(DIM)),
            ]));
            for l in c.content.lines() {
                lines.push(Line::from(Span::raw(format!("      {}", trunc(l, 100)))));
            }
        }
        lines.push(Line::from(""));
    }
    if lines.is_empty() {
        lines.push(Line::from(Span::styled(
            if app.loading > 0 {
                "  loading…"
            } else {
                "  no comments on this pull request"
            },
            Style::new().fg(DIM),
        )));
    }
    let altura = area.height.saturating_sub(2) as usize;
    let inicio = lines.len().saturating_sub(altura).min(app.thread_sel * 3);
    f.render_widget(
        Paragraph::new(
            lines
                .into_iter()
                .skip(inicio)
                .take(altura)
                .collect::<Vec<_>>(),
        )
        .block(panel(&format!("{} · comments", app.log_title))),
        area,
    );
}

fn timeline(f: &mut Frame, area: Rect, app: &App) {
    let height = area.height.saturating_sub(2) as usize;
    let start = app.tl_sel.saturating_sub(height.saturating_sub(1));
    let lines: Vec<Line> = app
        .timeline
        .iter()
        .enumerate()
        .skip(start)
        .take(height)
        .map(|(i, r)| {
            let (sym, color) = build_mark(
                if r.state == "completed" {
                    "completed"
                } else {
                    "inProgress"
                },
                &r.result,
            );
            let indent = if r.kind == "Task" { "   " } else { " " };
            let style = if i == app.tl_sel {
                Style::new().bg(Color::Rgb(40, 46, 66)).bold()
            } else {
                Style::new()
            };
            Line::from(vec![
                Span::styled(format!("{indent}{sym} "), Style::new().fg(color)),
                Span::styled(trunc(&r.name, 60), style),
                Span::styled(
                    format!("  {}", dur(&r.start_time, &r.finish_time)),
                    Style::new().fg(DIM),
                ),
            ])
        })
        .collect();
    f.render_widget(
        Paragraph::new(lines).block(panel(&format!("build #{} · steps", app.build_id))),
        area,
    );
}

fn logs(f: &mut Frame, area: Rect, app: &App) {
    let height = area.height.saturating_sub(2) as usize;
    let needle = app.search.to_lowercase();
    let lines: Vec<Line> = app
        .log
        .iter()
        .skip(app.log_scroll)
        .take(height)
        .map(|l| {
            let low = l.to_lowercase();
            let color = if !needle.is_empty() && low.contains(&needle) {
                WARN
            } else if low.contains("error") || low.contains("##[error]") {
                BAD
            } else if low.contains("warning") {
                WARN
            } else {
                Color::Reset
            };
            Line::from(Span::styled(l.clone(), Style::new().fg(color)))
        })
        .collect();
    f.render_widget(
        Paragraph::new(lines).block(panel(&format!(
            "{} · line {}/{}",
            app.log_title,
            app.log_scroll + 1,
            app.log.len()
        ))),
        area,
    );
}

fn help(f: &mut Frame, area: Rect) {
    let g = |k: &str, d: &str| {
        Line::from(vec![
            Span::styled(format!("  {k:<12}"), Style::new().fg(ACCENT)),
            Span::styled(d.to_string(), Style::new().fg(DIM)),
        ])
    };
    let lines = vec![
        Line::from(Span::styled("  navigation", Style::new().bold())),
        g("1 2 3 / tab", "switch tab"),
        g("j k up down", "move · ctrl-d/ctrl-u jumps 10"),
        g("g G", "top / bottom"),
        g("/", "filter the list · esc clears"),
        g("r", "refresh now (auto every 30s)"),
        g("o", "open in browser"),
        g("enter", "detail (PR diff, build steps)"),
        g("q esc", "back / quit"),
        Line::from(""),
        Line::from(Span::styled("  pull requests", Style::new().bold())),
        g("a", "approve (vote 10)"),
        g("w", "waiting for author (vote -5)"),
        g("x", "reject (vote -10)"),
        g("c", "complete merge · confirms"),
        g("D", "abandon · confirms"),
        g("m", "cycle scope: all / mine / to review"),
        g("t", "comment threads · R replies"),
        Line::from(""),
        Line::from(Span::styled("  pipelines", Style::new().bold())),
        g("p", "toggle runs / definitions"),
        g("R", "run on a branch · confirms"),
        g("x", "cancel run · confirms"),
        g("l", "log of the step that failed"),
        Line::from(""),
        Line::from(Span::styled("  releases", Style::new().bold())),
        g("a x", "approve / reject · confirms"),
        g("d", "deploy a stage · confirms"),
        Line::from(""),
        Line::from(Span::styled("  columns", Style::new().bold())),
        g("blocked on", "what the branch policies still want"),
        Line::from(""),
        Line::from(Span::styled("  diff and log", Style::new().bold())),
        g("J K right left", "next / previous file"),
        g("/ n", "search · next match"),
    ];
    f.render_widget(
        Paragraph::new(lines).block(panel("keys · any key closes")),
        area,
    );
}

fn status_bar(f: &mut Frame, area: Rect, app: &App) {
    let (prefix, color) = if app.is_err {
        ("✗ ", BAD)
    } else if app.status.is_empty() {
        ("", DIM)
    } else {
        ("· ", OK)
    };
    let text = if app.typing_filter {
        format!("/{}", app.filter)
    } else if app.typing_search {
        format!("search: {}", app.search)
    } else {
        format!("{prefix}{}", app.status)
    };
    let style = if app.typing_filter || app.typing_search {
        Style::new().fg(ACCENT)
    } else {
        Style::new().fg(color)
    };
    f.render_widget(
        Paragraph::new(Span::styled(format!(" {text}"), style)),
        area,
    );
}

fn key_bar(f: &mut Frame, area: Rect, app: &App) {
    let keys: &[(&str, &str)] = match app.view {
        View::List => match app.tab {
            Tab::Prs => &[
                ("a", "approve"),
                ("c", "complete"),
                ("x", "reject"),
                ("D", "abandon"),
                ("enter", "diff"),
                ("t", "comments"),
                ("m", "scope"),
                ("o", "browser"),
                ("?", "keys"),
            ],
            Tab::Pipelines => &[
                ("enter", "steps"),
                ("l", "log"),
                ("R", "run"),
                ("x", "cancel"),
                ("p", "definitions"),
                ("o", "browser"),
                ("?", "keys"),
            ],
            Tab::Releases => &[
                ("a", "approve"),
                ("x", "reject"),
                ("d", "deploy"),
                ("o", "browser"),
                ("r", "refresh"),
                ("?", "keys"),
            ],
        },
        View::Threads => &[
            ("j/k", "thread"),
            ("R", "reply"),
            ("r", "refresh"),
            ("o", "browser"),
            ("esc", "back"),
        ],
        View::Diff => &[
            ("J/K", "file"),
            ("j/k", "scroll"),
            ("/n", "search"),
            ("o", "browser"),
            ("esc", "back"),
        ],
        View::Timeline => &[("enter", "step log"), ("r", "refresh"), ("esc", "back")],
        View::Log => &[
            ("j/k ctrl-d/u", "scroll"),
            ("/n", "search"),
            ("esc", "back"),
        ],
        View::Help => &[("any key", "close")],
    };
    let mut spans = vec![Span::raw(" ")];
    for (k, d) in keys {
        spans.push(Span::styled(*k, Style::new().fg(ACCENT).bold()));
        spans.push(Span::styled(format!(" {d}   "), Style::new().fg(DIM)));
    }
    f.render_widget(Line::from(spans), area);
}

fn centered(area: Rect, w: u16, h: u16) -> Rect {
    let x = area.x + area.width.saturating_sub(w) / 2;
    let y = area.y + area.height.saturating_sub(h) / 2;
    Rect::new(x, y, w.min(area.width), h.min(area.height))
}

fn modal(f: &mut Frame, area: Rect, m: &Modal) {
    let (title, mut lines, hint) = match m {
        Modal::Confirm { title, lines, .. } => (
            title.clone(),
            lines
                .iter()
                .map(|l| Line::from(Span::raw(format!("  {l}"))))
                .collect::<Vec<_>>(),
            "y confirms · any other key cancels",
        ),
        Modal::Input { title, value, .. } => (
            title.clone(),
            vec![Line::from(Span::styled(
                format!("  {value}▌"),
                Style::new().fg(ACCENT),
            ))],
            "enter confirms · esc cancels",
        ),
        Modal::Select {
            title, labels, sel, ..
        } => (
            title.clone(),
            labels
                .iter()
                .enumerate()
                .map(|(i, l)| {
                    Line::from(Span::styled(
                        format!("  {} {l}", if i == *sel { "▍" } else { " " }),
                        if i == *sel {
                            Style::new().fg(ACCENT).bold()
                        } else {
                            Style::new().fg(DIM)
                        },
                    ))
                })
                .collect(),
            "j/k picks · enter confirms · esc cancels",
        ),
    };
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        format!("  {hint}"),
        Style::new().fg(DIM),
    )));

    let w = lines
        .iter()
        .map(|l| l.width())
        .chain(std::iter::once(title.len() + 4))
        .max()
        .unwrap_or(40)
        .clamp(40, 100) as u16
        + 4;
    let h = lines.len() as u16 + 3;
    let rect = centered(area, w, h);
    f.render_widget(Clear, rect);
    f.render_widget(
        Paragraph::new(lines).block(
            Block::bordered()
                .border_type(BorderType::Rounded)
                .border_style(Style::new().fg(PEND))
                .title(Line::from(format!(" {title} ")).fg(PEND).bold()),
        ),
        rect,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::{
        Approval, Build, Change, Client, Definition, Environment, Item, PullRequest, Record,
        Release, Reviewer,
    };
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use std::sync::Arc;

    fn fake_app() -> App {
        let client = Arc::new(Client::new("org", "proj", "pat", true).unwrap());
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        let mut app = App::new(client, tx, "org".into(), "proj".into());
        app.prs = vec![PullRequest {
            pull_request_id: 101,
            title: "a pull request with a reasonably long title".into(),
            merge_status: "conflicts".into(),
            source_ref_name: "refs/heads/feature/x".into(),
            target_ref_name: "refs/heads/master".into(),
            reviewers: vec![Reviewer {
                vote: 10,
                ..Default::default()
            }],
            ..Default::default()
        }];
        app.builds = vec![Build {
            id: 1,
            status: "completed".into(),
            result: "failed".into(),
            ..Default::default()
        }];
        app.defs = vec![Definition {
            id: 2,
            name: "checkout-api".into(),
            ..Default::default()
        }];
        app.approvals = vec![Approval {
            id: 3,
            ..Default::default()
        }];
        app.releases = vec![Release {
            id: 4,
            environments: vec![Environment {
                id: 5,
                name: "prod".into(),
                status: "succeeded".into(),
            }],
            ..Default::default()
        }];
        app.changes = vec![Change {
            change_type: "add".into(),
            item: Item {
                path: "/a/b.ts".into(),
                ..Default::default()
            },
        }];
        app.diff = diff_lines("a\n", "b\n");
        app.timeline = vec![Record {
            name: "step".into(),
            ..Default::default()
        }];
        app.log = vec!["##[error] boom".into()];
        app
    }

    /// Renderiza todas as telas — inclusive num terminal minúsculo, onde as contas de Rect estouram.
    #[tokio::test]
    async fn nenhuma_tela_entra_em_panico() {
        let mut app = fake_app();
        for (w, h) in [(120u16, 40u16), (80, 24), (24, 6), (8, 3)] {
            let mut term = Terminal::new(TestBackend::new(w, h)).unwrap();
            for view in [
                View::List,
                View::Diff,
                View::Timeline,
                View::Log,
                View::Help,
            ] {
                app.view = view;
                for tab in [Tab::Prs, Tab::Pipelines, Tab::Releases] {
                    app.tab = tab;
                    for defs in [false, true] {
                        app.show_defs = defs;
                        term.draw(|f| draw(f, &app)).unwrap();
                    }
                }
            }
            app.view = View::List;
            for m in [
                Modal::Confirm {
                    title: "t".into(),
                    lines: vec!["line".into()],
                    action: Action::CancelBuild(1),
                },
                Modal::Input {
                    title: "t".into(),
                    value: "master".into(),
                    kind: InputKind::RunBuild(1, "p".into()),
                },
                Modal::Select {
                    title: "t".into(),
                    labels: vec!["a".into()],
                    actions: vec![Action::CancelBuild(1)],
                    sel: 0,
                },
            ] {
                app.modal = Some(m);
                term.draw(|f| draw(f, &app)).unwrap();
            }
            app.modal = None;
        }
    }

    /// Listas vazias com cursor em zero não podem estourar índice.
    #[tokio::test]
    async fn telas_vazias_renderizam() {
        let client = Arc::new(Client::new("org", "proj", "pat", true).unwrap());
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        let app = App::new(client, tx, "org".into(), "proj".into());
        let mut term = Terminal::new(TestBackend::new(80, 24)).unwrap();
        term.draw(|f| draw(f, &app)).unwrap();
    }
}

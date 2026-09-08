use crate::api::Client;
use crate::config::{self, Config};
use crate::ui::{ACCENT, BAD, BORDER, DIM, OK, PEND};
use anyhow::Result;
use crossterm::event::{Event, EventStream, KeyCode, KeyEventKind, KeyModifiers};
use futures::StreamExt;
use ratatui::DefaultTerminal;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Paragraph};

const FIELDS: [&str; 3] = ["organization", "project", "personal access token"];

struct Form {
    values: [String; 3],
    field: usize,
    msg: String,
    is_err: bool,
}

/// Primeiro run: pede org, projeto e PAT, valida contra a API e devolve a config.
/// `None` = a pessoa desistiu.
pub async fn run(terminal: &mut DefaultTerminal, cfg: Config) -> Result<Option<Config>> {
    let (az_org, az_project) = Config::from_az();
    let mut form = Form {
        values: [
            if cfg.org.is_empty() { az_org } else { cfg.org },
            if cfg.project.is_empty() {
                az_project
            } else {
                cfg.project
            },
            cfg.pat,
        ],
        field: 0,
        msg: String::new(),
        is_err: false,
    };
    // já veio algo do az? começa no campo que falta
    form.field = form.values.iter().position(|v| v.is_empty()).unwrap_or(0);

    let mut events = EventStream::new();
    terminal.draw(|f| draw(f.area(), f, &form))?;

    while let Some(Ok(ev)) = events.next().await {
        let Event::Key(k) = ev else {
            terminal.draw(|f| draw(f.area(), f, &form))?;
            continue;
        };
        if k.kind != KeyEventKind::Press {
            continue;
        }
        match k.code {
            KeyCode::Esc => return Ok(None),
            KeyCode::Char('c') if k.modifiers.contains(KeyModifiers::CONTROL) => return Ok(None),
            KeyCode::Char(c) => form.values[form.field].push(c),
            KeyCode::Backspace => {
                form.values[form.field].pop();
            }
            KeyCode::Tab | KeyCode::Down => form.field = (form.field + 1) % 3,
            KeyCode::BackTab | KeyCode::Up => form.field = (form.field + 2) % 3,
            KeyCode::Enter => {
                if let Some(i) = form.values.iter().position(|v| v.trim().is_empty()) {
                    form.field = i;
                    form.msg = format!("still empty: {}", FIELDS[i]);
                    form.is_err = true;
                } else {
                    form.msg = "checking…".into();
                    form.is_err = false;
                    terminal.draw(|f| draw(f.area(), f, &form))?;

                    let cfg = Config {
                        org: form.values[0].trim().to_string(),
                        project: form.values[1].trim().to_string(),
                        pat: form.values[2].trim().to_string(),
                    };
                    match Client::new(&cfg.org, &cfg.project, &cfg.pat, true) {
                        Ok(c) => match c.validate().await {
                            Ok(_) => return Ok(Some(cfg)),
                            Err(e) => {
                                form.msg = friendly(&e.to_string());
                                form.is_err = true;
                            }
                        },
                        Err(e) => {
                            form.msg = e.to_string();
                            form.is_err = true;
                        }
                    }
                }
            }
            _ => {}
        }
        terminal.draw(|f| draw(f.area(), f, &form))?;
    }
    Ok(None)
}

fn friendly(err: &str) -> String {
    if err.contains("401") || err.contains("203") {
        "token refused — check the PAT and its scopes".into()
    } else if err.contains("404") {
        "organization or project not found".into()
    } else {
        err.chars().take(90).collect()
    }
}

fn draw(area: Rect, f: &mut ratatui::Frame, form: &Form) {
    let w = 68u16.min(area.width);
    let h = 20u16.min(area.height);
    let rect = Rect::new(
        area.x + area.width.saturating_sub(w) / 2,
        area.y + area.height.saturating_sub(h) / 2,
        w,
        h,
    );

    let mut lines = vec![
        Line::from(Span::styled(
            "  manage Azure DevOps pull requests, pipelines and releases from the terminal",
            Style::new().fg(DIM),
        )),
        Line::from(""),
    ];

    for (i, name) in FIELDS.iter().enumerate() {
        let active = i == form.field;
        let shown = if i == 2 {
            "•".repeat(form.values[2].chars().count().min(40))
        } else {
            form.values[i].clone()
        };
        lines.push(Line::from(vec![
            Span::styled(
                format!("  {}{name:<24}", if active { "▍" } else { " " }),
                if active {
                    Style::new().fg(ACCENT).bold()
                } else {
                    Style::new().fg(DIM)
                },
            ),
            Span::styled(
                format!("{shown}{}", if active { "▌" } else { "" }),
                Style::new().fg(if active { Color::Reset } else { DIM }),
            ),
        ]));
    }

    lines.extend([
        Line::from(""),
        Line::from(Span::styled(
            "  get a PAT from  Azure DevOps → User settings → Personal access tokens",
            Style::new().fg(DIM),
        )),
        Line::from(Span::styled(
            "  scopes: Code (read & write) · Build (read & execute)",
            Style::new().fg(DIM),
        )),
        Line::from(Span::styled(
            "           Release (read, write, execute & manage)",
            Style::new().fg(DIM),
        )),
        Line::from(""),
        Line::from(Span::styled(
            format!("  saved to {}  (mode 600)", config::path().display()),
            Style::new().fg(DIM),
        )),
        Line::from(""),
        Line::from(vec![
            Span::styled("  tab", Style::new().fg(ACCENT)),
            Span::styled(" next field   ", Style::new().fg(DIM)),
            Span::styled("enter", Style::new().fg(ACCENT)),
            Span::styled(" validate and save   ", Style::new().fg(DIM)),
            Span::styled("esc", Style::new().fg(ACCENT)),
            Span::styled(" quit", Style::new().fg(DIM)),
        ]),
    ]);

    if !form.msg.is_empty() {
        lines.push(Line::from(Span::styled(
            format!("  {}{}", if form.is_err { "✗ " } else { "· " }, form.msg),
            Style::new().fg(if form.is_err { BAD } else { OK }),
        )));
    }

    f.render_widget(
        Paragraph::new(lines).block(
            Block::bordered()
                .border_type(BorderType::Rounded)
                .border_style(Style::new().fg(if form.is_err { BAD } else { BORDER }))
                .title(Line::from(" azura · setup ").fg(PEND).bold()),
        ),
        rect,
    );
}

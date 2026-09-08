//! Renders the README SVGs straight from the real ratatui buffer, with fake data.
//! `cargo test shot -- --ignored`

use crate::api::*;
use crate::app::*;
use crate::ui;
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use ratatui::style::{Color, Modifier};
use std::sync::Arc;

const CW: f32 = 8.4;
const CH: f32 = 19.0;
const PAD: f32 = 18.0;
const BG: &str = "#0e1018";
const FG: &str = "#ccd2e3";

fn hex(c: Color, default: &str) -> String {
    match c {
        Color::Rgb(r, g, b) => format!("#{r:02x}{g:02x}{b:02x}"),
        Color::Black => "#0b0d14".into(),
        Color::Reset => default.into(),
        _ => default.into(),
    }
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn svg(buf: &Buffer) -> String {
    let (w, h) = (buf.area.width, buf.area.height);
    let (pw, ph) = (w as f32 * CW + PAD * 2.0, h as f32 * CH + PAD * 2.0);
    let mut out = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{pw:.0}\" height=\"{ph:.0}\" \
         viewBox=\"0 0 {pw:.0} {ph:.0}\">\n\
         <rect width=\"{pw:.0}\" height=\"{ph:.0}\" rx=\"10\" fill=\"{BG}\"/>\n\
         <g font-family=\"SFMono-Regular,Menlo,Consolas,'DejaVu Sans Mono',monospace\" \
         font-size=\"14\" xml:space=\"preserve\">\n"
    );

    for y in 0..h {
        // 1) fundos: runs de mesma cor de fundo, inclusive nas células vazias
        let mut x = 0u16;
        while x < w {
            let Some(cell) = buf.cell((x, y)) else { break };
            let bg = cell.bg;
            let mut n = 1u16;
            while x + n < w && buf.cell((x + n, y)).map(|c| c.bg) == Some(bg) {
                n += 1;
            }
            if hex(bg, BG) != BG {
                out += &format!(
                    "<rect x=\"{:.1}\" y=\"{:.1}\" width=\"{:.1}\" height=\"{CH}\" fill=\"{}\"/>\n",
                    PAD + x as f32 * CW,
                    PAD + y as f32 * CH,
                    n as f32 * CW,
                    hex(bg, BG)
                );
            }
            x += n;
        }

        // 2) texto: nunca junta célula em branco num run — senão o textLength
        // estica a palavra por cima do espaçamento das colunas
        let mut runs: Vec<(u16, String, Color, Modifier)> = vec![];
        for x in 0..w {
            let Some(cell) = buf.cell((x, y)) else {
                continue;
            };
            let sym = cell.symbol();
            if sym.trim().is_empty() {
                runs.push((x, String::new(), cell.fg, cell.modifier));
                continue;
            }
            let junta = match runs.last() {
                Some((sx, text, fg, m)) => {
                    !text.is_empty()
                        && *fg == cell.fg
                        && *m == cell.modifier
                        && sx + text.chars().count() as u16 == x
                        && sym.is_ascii()
                        && text.is_ascii()
                }
                None => false,
            };
            if junta {
                runs.last_mut().unwrap().1.push_str(sym);
            } else {
                runs.push((x, sym.to_string(), cell.fg, cell.modifier));
            }
        }

        for (x, text, fg, m) in runs {
            if text.is_empty() {
                continue;
            }
            let bold = if m.contains(Modifier::BOLD) {
                " font-weight=\"600\""
            } else {
                ""
            };
            let underline = if m.contains(Modifier::UNDERLINED) {
                " text-decoration=\"underline\""
            } else {
                ""
            };
            // textLength trava o run na largura exata das suas células: a grade
            // não depende da fonte que o visualizador acabar escolhendo
            out += &format!(
                "<text x=\"{:.1}\" y=\"{:.1}\" textLength=\"{:.1}\" lengthAdjust=\"spacingAndGlyphs\" \
                 fill=\"{}\"{bold}{underline}>{}</text>\n",
                PAD + x as f32 * CW,
                PAD + y as f32 * CH + CH - 5.0,
                text.chars().count() as f32 * CW,
                hex(fg, FG),
                esc(&text)
            );
        }
    }

    out + "</g>\n</svg>\n"
}

fn iso(min_atras: i64) -> String {
    (chrono::Utc::now() - chrono::Duration::minutes(min_atras)).to_rfc3339()
}

fn named(name: &str) -> Named {
    Named {
        name: name.into(),
        display_name: name.into(),
        ..Default::default()
    }
}

fn pr(
    id: i64,
    repo: &str,
    titulo: &str,
    autor: &str,
    votos: &[i32],
    min: i64,
    extra: &str,
) -> PullRequest {
    PullRequest {
        pull_request_id: id,
        title: titulo.into(),
        is_draft: extra == "draft",
        merge_status: if extra == "conflicts" {
            "conflicts".into()
        } else {
            "succeeded".into()
        },
        creation_date: iso(min),
        source_ref_name: format!("refs/heads/{}", extra_branch(extra, id)),
        target_ref_name: "refs/heads/main".into(),
        created_by: named(autor),
        repository: Named {
            name: repo.into(),
            ..Default::default()
        },
        reviewers: votos
            .iter()
            .map(|v| Reviewer {
                vote: *v,
                ..Default::default()
            })
            .collect(),
        ..Default::default()
    }
}

fn extra_branch(extra: &str, id: i64) -> String {
    if extra.is_empty() || extra == "draft" || extra == "conflicts" {
        format!("feature/{id}")
    } else {
        extra.into()
    }
}

#[allow(clippy::too_many_arguments)]
fn build(
    id: i64,
    def: &str,
    status: &str,
    result: &str,
    branch: &str,
    quem: &str,
    min: i64,
    seg: i64,
) -> Build {
    Build {
        id,
        status: status.into(),
        result: result.into(),
        source_branch: format!("refs/heads/{branch}"),
        definition: named(def),
        requested_for: named(quem),
        queue_time: iso(min),
        start_time: iso(min),
        finish_time: iso(min - seg / 60),
    }
}

fn mock() -> App {
    let client = Arc::new(Client::new("contoso", "Fabrikam", "pat", false).unwrap());
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
    std::mem::forget(rx);
    let mut app = App::new(client, tx, "contoso".into(), "Fabrikam".into());
    app.loading = 0;
    app.status = "PR !412 approved".into();

    app.prs = vec![
        pr(
            412,
            "web",
            "feat: session cache at the edge",
            "Ana Souza",
            &[10, 10],
            95,
            "",
        ),
        pr(
            411,
            "api",
            "fix: upload timeout on large batches",
            "Bruno Lima",
            &[10],
            260,
            "",
        ),
        pr(
            408,
            "api",
            "migrate authentication to OIDC",
            "Carla Dias",
            &[-10, 10],
            700,
            "",
        ),
        pr(
            405,
            "worker",
            "reprocess the failed webhook queue",
            "Diego Reis",
            &[],
            1500,
            "conflicts",
        ),
        pr(
            402,
            "web",
            "adjust contrast on secondary buttons",
            "Ana Souza",
            &[10],
            2900,
            "",
        ),
        pr(
            399,
            "infra",
            "add a dedicated node pool for batch",
            "Elisa Prado",
            &[],
            4300,
            "draft",
        ),
        pr(
            396,
            "api",
            "expose queue metrics on /healthz",
            "Bruno Lima",
            &[10, 10, 10],
            8800,
            "",
        ),
    ];
    app.builds = vec![
        build(
            9241,
            "checkout-api",
            "inProgress",
            "",
            "main",
            "Ana Souza",
            3,
            0,
        ),
        build(
            9240,
            "storefront",
            "completed",
            "succeeded",
            "main",
            "Bruno Lima",
            22,
            214,
        ),
        build(
            9239,
            "invoicing-worker",
            "completed",
            "failed",
            "feature/405",
            "Diego Reis",
            48,
            96,
        ),
        build(
            9238,
            "checkout-api-staging",
            "completed",
            "succeeded",
            "feature/411",
            "Bruno Lima",
            71,
            188,
        ),
        build(
            9237,
            "storefront-staging",
            "completed",
            "succeeded",
            "feature/412",
            "Ana Souza",
            132,
            176,
        ),
        build(
            9236,
            "e2e-nightly",
            "completed",
            "partiallySucceeded",
            "main",
            "scheduled",
            400,
            1450,
        ),
        build(
            9235,
            "checkout-api",
            "completed",
            "succeeded",
            "main",
            "Carla Dias",
            640,
            205,
        ),
        build(
            9234,
            "terraform-plan",
            "completed",
            "canceled",
            "feature/399",
            "Elisa Prado",
            900,
            31,
        ),
    ];
    app.approvals = vec![
        Approval {
            id: 8801,
            approval_type: "preDeploy".into(),
            created_on: iso(12),
            release: named("Release-214"),
            release_definition: named("checkout-api-prod"),
            release_environment: named("Production"),
        },
        Approval {
            id: 8802,
            approval_type: "preDeploy".into(),
            created_on: iso(46),
            release: named("Release-213"),
            release_definition: named("storefront-prod"),
            release_environment: named("Production"),
        },
    ];
    let env = |id: i64, n: &str, s: &str| Environment {
        id,
        name: n.into(),
        status: s.into(),
    };
    app.releases = vec![
        Release {
            id: 7214,
            name: "Release-214".into(),
            created_on: iso(14),
            release_definition: named("checkout-api-prod"),
            environments: vec![
                env(1, "Staging", "succeeded"),
                env(2, "Production", "queued"),
            ],
        },
        Release {
            id: 7213,
            name: "Release-213".into(),
            created_on: iso(52),
            release_definition: named("storefront-prod"),
            environments: vec![
                env(3, "Staging", "succeeded"),
                env(4, "Production", "queued"),
            ],
        },
        Release {
            id: 7212,
            name: "Release-212".into(),
            created_on: iso(300),
            release_definition: named("invoicing-worker"),
            environments: vec![env(5, "Staging", "succeeded")],
        },
        Release {
            id: 7211,
            name: "Release-211".into(),
            created_on: iso(880),
            release_definition: named("checkout-api-prod"),
            environments: vec![
                env(6, "Staging", "succeeded"),
                env(7, "Production", "succeeded"),
            ],
        },
        Release {
            id: 7210,
            name: "Release-210".into(),
            created_on: iso(1600),
            release_definition: named("storefront-prod"),
            environments: vec![
                env(8, "Staging", "succeeded"),
                env(9, "Production", "rejected"),
            ],
        },
    ];
    app
}

const ANTES: &str = "export function useSession(token: string) {\n  const [user, setUser] = useState<User>()\n\n  useEffect(() => {\n    fetch(`/api/session`, { headers: { Authorization: token } })\n      .then((r) => r.json())\n      .then(setUser)\n  }, [token])\n\n  return { user }\n}\n";
const DEPOIS: &str = "export function useSession(token: string) {\n  const [user, setUser] = useState<User>()\n  const cache = useRef(new Map<string, User>())\n\n  useEffect(() => {\n    const hit = cache.current.get(token)\n    if (hit) return setUser(hit)\n\n    fetch(`/api/session`, { headers: { Authorization: token } })\n      .then((r) => r.json())\n      .then((u) => {\n        cache.current.set(token, u)\n        setUser(u)\n      })\n  }, [token])\n\n  return { user }\n}\n";

fn write(app: &App, nome: &str, w: u16, h: u16) {
    let mut term = Terminal::new(TestBackend::new(w, h)).unwrap();
    term.draw(|f| ui::draw(f, app)).unwrap();
    std::fs::create_dir_all("assets").unwrap();
    let path = format!("assets/{nome}.svg");
    std::fs::write(&path, svg(term.backend().buffer())).unwrap();
    println!("{path}");
}

#[tokio::test]
#[ignore]
async fn readme_svgs() {
    let mut app = mock();

    app.tab = Tab::Prs;
    write(&app, "prs", 108, 13);

    app.tab = Tab::Pipelines;
    app.status = "e2e-nightly queued on refs/heads/main".into();
    write(&app, "pipelines", 108, 14);

    app.tab = Tab::Releases;
    app.status = String::new();
    app.sel = [0, 0, 0];
    app.modal = Some(Modal::Confirm {
        title: "Approve deploy".into(),
        lines: vec![
            "checkout-api-prod → stage Production".into(),
            "Release-214 · preDeploy".into(),
        ],
        action: Action::Approval(8801, true),
    });
    write(&app, "releases", 108, 15);
    app.modal = None;

    app.tab = Tab::Prs;
    app.view = View::Diff;
    app.pr = Some(app.prs[0].clone());
    app.changes = vec![
        Change {
            change_type: "edit".into(),
            item: Item {
                path: "/src/hooks/useSession.ts".into(),
                is_folder: false,
            },
        },
        Change {
            change_type: "add".into(),
            item: Item {
                path: "/src/hooks/useSession.test.ts".into(),
                is_folder: false,
            },
        },
        Change {
            change_type: "edit".into(),
            item: Item {
                path: "/src/lib/api.ts".into(),
                is_folder: false,
            },
        },
        Change {
            change_type: "delete".into(),
            item: Item {
                path: "/src/lib/legacy-session.ts".into(),
                is_folder: false,
            },
        },
    ];
    app.diff = diff_lines(ANTES, DEPOIS);
    write(&app, "diff", 108, 20);
}

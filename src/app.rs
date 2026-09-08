use crate::api::*;
use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use similar::TextDiff;
use std::future::Future;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::mpsc::UnboundedSender;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Prs,
    Pipelines,
    Releases,
}

impl Tab {
    pub fn idx(self) -> usize {
        match self {
            Tab::Prs => 0,
            Tab::Pipelines => 1,
            Tab::Releases => 2,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum View {
    List,
    Diff,
    Timeline,
    Log,
    Help,
}

#[derive(Clone, Debug)]
pub struct DiffLine {
    pub kind: char,
    pub text: String,
}

/// Diff unificado entre duas versões de um arquivo.
pub fn diff_lines(old: &str, new: &str) -> Vec<DiffLine> {
    if old == new {
        return vec![DiffLine {
            kind: 'i',
            text: "(no content changes)".into(),
        }];
    }
    if old.contains('\0') || new.contains('\0') {
        return vec![DiffLine {
            kind: 'i',
            text: "(binary file)".into(),
        }];
    }
    let diff = TextDiff::from_lines(old, new);
    let mut out = Vec::new();
    for hunk in diff.unified_diff().context_radius(3).iter_hunks() {
        out.push(DiffLine {
            kind: '@',
            text: hunk.header().to_string(),
        });
        for change in hunk.iter_changes() {
            let kind = match change.tag() {
                similar::ChangeTag::Delete => '-',
                similar::ChangeTag::Insert => '+',
                similar::ChangeTag::Equal => ' ',
            };
            out.push(DiffLine {
                kind,
                text: change.value().trim_end_matches(['\n', '\r']).to_string(),
            });
        }
    }
    if out.is_empty() {
        out.push(DiffLine {
            kind: 'i',
            text: "(no changes)".into(),
        });
    }
    out
}

/// "3m", "2h", "5d" — vazio se a data não parsear.
pub fn ago(iso: &str) -> String {
    let Ok(t) = chrono::DateTime::parse_from_rfc3339(iso) else {
        return String::new();
    };
    let secs = (chrono::Utc::now() - t.with_timezone(&chrono::Utc)).num_seconds();
    match secs {
        s if s < 60 => format!("{s}s"),
        s if s < 3600 => format!("{}m", s / 60),
        s if s < 86400 => format!("{}h", s / 3600),
        s => format!("{}d", s / 86400),
    }
}

pub fn dur(start: &str, finish: &str) -> String {
    let (Ok(a), Ok(b)) = (
        chrono::DateTime::parse_from_rfc3339(start),
        chrono::DateTime::parse_from_rfc3339(finish),
    ) else {
        return String::new();
    };
    let secs = (b - a).num_seconds().max(0);
    format!("{}:{:02}", secs / 60, secs % 60)
}

pub fn short_branch(r: &str) -> String {
    r.trim_start_matches("refs/heads/")
        .trim_start_matches("refs/tags/")
        .to_string()
}

#[derive(Clone, Debug)]
pub enum Action {
    Vote(String, i64, i32),
    CompletePr(String, i64, String),
    AbandonPr(String, i64),
    RunBuild(i64, String, String),
    CancelBuild(i64),
    Approval(i64, bool),
    Deploy(i64, i64, String),
}

pub enum Modal {
    Confirm {
        title: String,
        lines: Vec<String>,
        action: Action,
    },
    Input {
        title: String,
        value: String,
        def: (i64, String),
    },
    Select {
        title: String,
        labels: Vec<String>,
        actions: Vec<Action>,
        sel: usize,
    },
}

pub enum Msg {
    Me(String),
    Prs(Vec<PullRequest>),
    Builds(Vec<Build>),
    Defs(Vec<Definition>),
    Approvals(Vec<Approval>),
    Releases(Vec<Release>),
    PrDetail(Box<PullRequest>),
    Changes(Vec<Change>),
    Diff(Vec<DiffLine>),
    Timeline(Vec<Record>),
    Log(String),
    Ok(String),
    Err(String),
    Done,
    Tick,
    Spin,
}

#[derive(Clone, Copy)]
pub enum RelRow {
    Approval(usize),
    Release(usize),
}

pub struct App {
    pub client: Arc<Client>,
    pub tx: UnboundedSender<Msg>,
    pub org: String,
    pub project: String,

    pub tab: Tab,
    pub view: View,
    pub sel: [usize; 3],

    pub prs: Vec<PullRequest>,
    pub builds: Vec<Build>,
    pub defs: Vec<Definition>,
    pub show_defs: bool,
    pub approvals: Vec<Approval>,
    pub releases: Vec<Release>,

    pub my_id: String,
    pub mine_only: bool,

    pub filter: String,
    pub typing_filter: bool,

    pub pr: Option<PullRequest>,
    pub changes: Vec<Change>,
    pub change_sel: usize,
    pub diff: Vec<DiffLine>,
    pub diff_scroll: usize,

    pub build_id: i64,
    pub timeline: Vec<Record>,
    pub tl_sel: usize,
    pub log: Vec<String>,
    pub log_title: String,
    pub log_scroll: usize,

    pub search: String,
    pub typing_search: bool,

    pub modal: Option<Modal>,
    pub status: String,
    pub is_err: bool,
    pub loading: usize,
    pub spin: usize,
    pub last_refresh: Instant,
    pub quit: bool,
}

impl App {
    pub fn new(
        client: Arc<Client>,
        tx: UnboundedSender<Msg>,
        org: String,
        project: String,
    ) -> Self {
        let mut app = Self {
            client,
            tx,
            org,
            project,
            tab: Tab::Prs,
            view: View::List,
            sel: [0; 3],
            prs: vec![],
            builds: vec![],
            defs: vec![],
            show_defs: false,
            approvals: vec![],
            releases: vec![],
            my_id: String::new(),
            mine_only: false,
            filter: String::new(),
            typing_filter: false,
            pr: None,
            changes: vec![],
            change_sel: 0,
            diff: vec![],
            diff_scroll: 0,
            build_id: 0,
            timeline: vec![],
            tl_sel: 0,
            log: vec![],
            log_title: String::new(),
            log_scroll: 0,
            search: String::new(),
            typing_search: false,
            modal: None,
            status: String::new(),
            is_err: false,
            loading: 0,
            spin: 0,
            last_refresh: Instant::now(),
            quit: false,
        };
        app.status = if app.client.dry_run {
            "--dry-run: no writes will be sent".into()
        } else {
            "loading…".into()
        };
        app.go(|c| async move { c.my_id().await }, Msg::Me);
        app.refresh_all();
        app
    }

    fn go<F, Fut, T>(&mut self, f: F, wrap: fn(T) -> Msg)
    where
        F: FnOnce(Arc<Client>) -> Fut + Send + 'static,
        Fut: Future<Output = Result<T>> + Send,
        T: Send + 'static,
    {
        let c = self.client.clone();
        let tx = self.tx.clone();
        self.loading += 1;
        tokio::spawn(async move {
            let msg = match f(c).await {
                Ok(v) => wrap(v),
                Err(e) => Msg::Err(e.to_string()),
            };
            let _ = tx.send(Msg::Done);
            let _ = tx.send(msg);
        });
    }

    pub fn refresh_all(&mut self) {
        self.go(|c| async move { c.approvals().await }, Msg::Approvals);
        match self.tab {
            Tab::Prs => self.go(|c| async move { c.pull_requests().await }, Msg::Prs),
            Tab::Pipelines => {
                self.go(|c| async move { c.builds().await }, Msg::Builds);
                if self.defs.is_empty() {
                    self.go(|c| async move { c.definitions().await }, Msg::Defs);
                }
            }
            Tab::Releases => self.go(|c| async move { c.releases().await }, Msg::Releases),
        }
        self.last_refresh = Instant::now();
    }

    // ---- linhas visíveis ----

    fn matches(&self, hay: &str) -> bool {
        self.filter.is_empty() || hay.to_lowercase().contains(&self.filter.to_lowercase())
    }

    pub fn visible_prs(&self) -> Vec<usize> {
        self.prs
            .iter()
            .enumerate()
            .filter(|(_, p)| {
                let mine = p.created_by.id.as_str() == Some(self.my_id.as_str())
                    || p.reviewers.iter().any(|r| r.id == self.my_id);
                (!self.mine_only || mine)
                    && self.matches(&format!(
                        "{} {} {} {} {} {} {}",
                        p.title,
                        p.repository.name,
                        p.created_by.display_name,
                        short_branch(&p.source_ref_name),
                        short_branch(&p.target_ref_name),
                        p.merge_status,
                        if p.is_draft { "draft" } else { "" }
                    ))
            })
            .map(|(i, _)| i)
            .collect()
    }

    pub fn visible_builds(&self) -> Vec<usize> {
        self.builds
            .iter()
            .enumerate()
            .filter(|(_, b)| {
                self.matches(&format!(
                    "{} {} {} {} {}",
                    b.definition.name,
                    short_branch(&b.source_branch),
                    b.status,
                    b.result,
                    b.requested_for.display_name
                ))
            })
            .map(|(i, _)| i)
            .collect()
    }

    pub fn visible_defs(&self) -> Vec<usize> {
        self.defs
            .iter()
            .enumerate()
            .filter(|(_, d)| self.matches(&format!("{} {}", d.name, d.path)))
            .map(|(i, _)| i)
            .collect()
    }

    pub fn rel_rows(&self) -> Vec<RelRow> {
        let mut rows: Vec<RelRow> = self
            .approvals
            .iter()
            .enumerate()
            .filter(|(_, a)| {
                self.matches(&format!(
                    "{} {} {} approval pending",
                    a.release_definition.name, a.release_environment.name, a.release.name
                ))
            })
            .map(|(i, _)| RelRow::Approval(i))
            .collect();
        rows.extend(
            self.releases
                .iter()
                .enumerate()
                .filter(|(_, r)| {
                    let stages: String = r
                        .environments
                        .iter()
                        .map(|e| format!("{} {} ", e.name, e.status))
                        .collect();
                    self.matches(&format!(
                        "{} {} {stages}",
                        r.release_definition.name, r.name
                    ))
                })
                .map(|(i, _)| RelRow::Release(i)),
        );
        rows
    }

    pub fn row_count(&self) -> usize {
        match self.tab {
            Tab::Prs => self.visible_prs().len(),
            Tab::Pipelines => {
                if self.show_defs {
                    self.visible_defs().len()
                } else {
                    self.visible_builds().len()
                }
            }
            Tab::Releases => self.rel_rows().len(),
        }
    }

    /// Quantas linhas existiriam sem o filtro de texto.
    pub fn total_rows(&self) -> usize {
        match self.tab {
            Tab::Prs => self.prs.len(),
            Tab::Pipelines => {
                if self.show_defs {
                    self.defs.len()
                } else {
                    self.builds.len()
                }
            }
            Tab::Releases => self.approvals.len() + self.releases.len(),
        }
    }

    pub fn cursor(&self) -> usize {
        self.sel[self.tab.idx()].min(self.row_count().saturating_sub(1))
    }

    fn sel_pr(&self) -> Option<&PullRequest> {
        self.visible_prs().get(self.cursor()).map(|&i| &self.prs[i])
    }
    fn sel_build(&self) -> Option<&Build> {
        self.visible_builds()
            .get(self.cursor())
            .map(|&i| &self.builds[i])
    }
    fn sel_def(&self) -> Option<&Definition> {
        self.visible_defs()
            .get(self.cursor())
            .map(|&i| &self.defs[i])
    }
    fn sel_rel(&self) -> Option<RelRow> {
        self.rel_rows().get(self.cursor()).copied()
    }

    // ---- mensagens ----

    pub fn on_msg(&mut self, msg: Msg) {
        match msg {
            Msg::Me(id) => self.my_id = id,
            Msg::Prs(v) => self.prs = v,
            Msg::Builds(v) => self.builds = v,
            Msg::Defs(v) => self.defs = v,
            Msg::Approvals(v) => self.approvals = v,
            Msg::Releases(v) => self.releases = v,
            Msg::PrDetail(p) => {
                let (repo, id) = (p.repo_id(), p.pull_request_id);
                self.pr = Some(*p);
                self.changes.clear();
                self.diff.clear();
                self.change_sel = 0;
                self.go(
                    move |c| async move { c.pr_changes(&repo, id).await },
                    Msg::Changes,
                );
            }
            Msg::Changes(v) => {
                self.changes = v;
                self.change_sel = 0;
                self.load_diff();
            }
            Msg::Diff(v) => {
                self.diff = v;
                self.diff_scroll = 0;
            }
            Msg::Timeline(v) => {
                self.timeline = v;
                self.tl_sel = 0;
            }
            Msg::Log(text) => {
                self.log = text.lines().map(str::to_string).collect();
                self.log_scroll = 0;
                self.view = View::Log;
            }
            Msg::Ok(s) => {
                self.status = s;
                self.is_err = false;
                self.refresh_all();
            }
            Msg::Err(e) => {
                self.status = e;
                self.is_err = true;
            }
            Msg::Done => self.loading = self.loading.saturating_sub(1),
            Msg::Tick => self.refresh_all(),
            Msg::Spin => self.spin = self.spin.wrapping_add(1),
        }
    }

    fn load_diff(&mut self) {
        let (Some(pr), Some(ch)) = (self.pr.as_ref(), self.changes.get(self.change_sel)) else {
            return;
        };
        let repo = pr.repo_id();
        let path = ch.item.path.clone();
        let base = pr.last_merge_target_commit.commit_id.clone();
        let head = pr.last_merge_source_commit.commit_id.clone();
        let added = ch.change_type.contains("add");
        let deleted = ch.change_type.contains("delete");
        self.diff = vec![DiffLine {
            kind: 'i',
            text: "loading…".into(),
        }];
        self.go(
            move |c| async move {
                let (old, new) = tokio::try_join!(
                    async {
                        if added {
                            Ok(String::new())
                        } else {
                            c.file_text(&repo, &path, &base).await
                        }
                    },
                    async {
                        if deleted {
                            Ok(String::new())
                        } else {
                            c.file_text(&repo, &path, &head).await
                        }
                    }
                )?;
                Ok(diff_lines(&old, &new))
            },
            Msg::Diff,
        );
    }

    fn run(&mut self, action: Action) {
        match action {
            Action::Vote(repo, pr, v) => {
                let me = self.my_id.clone();
                self.go(
                    move |c| async move { c.vote(&repo, pr, &me, v).await },
                    Msg::Ok,
                )
            }
            Action::CompletePr(repo, pr, sha) => self.go(
                move |c| async move { c.complete_pr(&repo, pr, &sha).await },
                Msg::Ok,
            ),
            Action::AbandonPr(repo, pr) => self.go(
                move |c| async move { c.abandon_pr(&repo, pr).await },
                Msg::Ok,
            ),
            Action::RunBuild(id, name, branch) => self.go(
                move |c| async move { c.queue_build(id, &name, &branch).await },
                Msg::Ok,
            ),
            Action::CancelBuild(id) => {
                self.go(move |c| async move { c.cancel_build(id).await }, Msg::Ok)
            }
            Action::Approval(id, ok) => self.go(
                move |c| async move { c.set_approval(id, ok, "via azura").await },
                Msg::Ok,
            ),
            Action::Deploy(rel, env, label) => self.go(
                move |c| async move { c.deploy(rel, env, &label).await },
                Msg::Ok,
            ),
        }
    }

    fn confirm(&mut self, title: &str, lines: Vec<String>, action: Action) {
        self.modal = Some(Modal::Confirm {
            title: title.into(),
            lines,
            action,
        });
    }

    fn open(&mut self, url: String) {
        let cmd = if cfg!(target_os = "macos") {
            "open"
        } else if cfg!(target_os = "windows") {
            "explorer"
        } else {
            "xdg-open"
        };
        let _ = std::process::Command::new(cmd).arg(&url).spawn();
        self.status = format!("opening {url}");
        self.is_err = false;
    }

    // ---- teclado ----

    pub fn on_key(&mut self, k: KeyEvent) {
        if self.modal.is_some() {
            return self.modal_key(k);
        }
        if self.typing_filter || self.typing_search {
            return self.text_key(k);
        }
        match self.view {
            View::List => self.list_key(k),
            View::Diff => self.diff_key(k),
            View::Timeline => self.timeline_key(k),
            View::Log => self.log_key(k),
            View::Help => {
                self.view = View::List;
            }
        }
    }

    fn text_key(&mut self, k: KeyEvent) {
        let target = if self.typing_filter {
            &mut self.filter
        } else {
            &mut self.search
        };
        match k.code {
            KeyCode::Char(c) => target.push(c),
            KeyCode::Backspace => {
                target.pop();
            }
            KeyCode::Esc => {
                target.clear();
                self.typing_filter = false;
                self.typing_search = false;
            }
            KeyCode::Enter => {
                self.typing_filter = false;
                self.typing_search = false;
                if !self.search.is_empty() {
                    self.find_next();
                }
            }
            _ => {}
        }
        self.sel[self.tab.idx()] = 0;
    }

    fn find_next(&mut self) {
        let needle = self.search.to_lowercase();
        let (lines, scroll): (Vec<String>, &mut usize) = match self.view {
            View::Diff => (
                self.diff.iter().map(|d| d.text.clone()).collect(),
                &mut self.diff_scroll,
            ),
            View::Log => (self.log.clone(), &mut self.log_scroll),
            _ => return,
        };
        let start = *scroll + 1;
        if let Some(i) = (start..lines.len())
            .chain(0..start.min(lines.len()))
            .find(|&i| lines[i].to_lowercase().contains(&needle))
        {
            *scroll = i;
        }
    }

    fn modal_key(&mut self, k: KeyEvent) {
        match self.modal.as_mut() {
            Some(Modal::Confirm { action, .. }) => match k.code {
                KeyCode::Char('y') | KeyCode::Char('Y') | KeyCode::Enter => {
                    let a = action.clone();
                    self.modal = None;
                    self.run(a);
                }
                _ => self.modal = None,
            },
            Some(Modal::Input { value, def, .. }) => match k.code {
                KeyCode::Char(c) => value.push(c),
                KeyCode::Backspace => {
                    value.pop();
                }
                KeyCode::Enter => {
                    let (id, name) = def.clone();
                    let branch = value.clone();
                    self.modal = None;
                    let branch_ref = if branch.starts_with("refs/") {
                        branch.clone()
                    } else {
                        format!("refs/heads/{branch}")
                    };
                    self.confirm(
                        "Run pipeline",
                        vec![name.clone(), format!("branch {branch_ref}")],
                        Action::RunBuild(id, name, branch_ref),
                    );
                }
                KeyCode::Esc => self.modal = None,
                _ => {}
            },
            Some(Modal::Select {
                labels,
                actions,
                sel,
                ..
            }) => match k.code {
                KeyCode::Char('j') | KeyCode::Down => *sel = (*sel + 1).min(labels.len() - 1),
                KeyCode::Char('k') | KeyCode::Up => *sel = sel.saturating_sub(1),
                KeyCode::Enter => {
                    let a = actions[*sel].clone();
                    let label = labels[*sel].clone();
                    self.modal = None;
                    self.confirm(
                        "Stage deploy",
                        vec![label, "This starts a real deploy.".into()],
                        a,
                    );
                }
                _ => self.modal = None,
            },
            None => {}
        }
    }

    fn move_cursor(&mut self, delta: isize, len: usize) -> usize {
        let cur = self.cursor() as isize;

        (cur + delta).clamp(0, len.saturating_sub(1) as isize) as usize
    }

    fn list_key(&mut self, k: KeyEvent) {
        let len = self.row_count();
        let i = self.tab.idx();
        match k.code {
            KeyCode::Char('q') => self.quit = true,
            KeyCode::Char('?') => self.view = View::Help,
            KeyCode::Char('1') => self.switch(Tab::Prs),
            KeyCode::Char('2') => self.switch(Tab::Pipelines),
            KeyCode::Char('3') => self.switch(Tab::Releases),
            KeyCode::Tab => self.switch(match self.tab {
                Tab::Prs => Tab::Pipelines,
                Tab::Pipelines => Tab::Releases,
                Tab::Releases => Tab::Prs,
            }),
            KeyCode::Char('j') | KeyCode::Down => self.sel[i] = self.move_cursor(1, len),
            KeyCode::Char('k') | KeyCode::Up => self.sel[i] = self.move_cursor(-1, len),
            KeyCode::Char('d') if k.modifiers.contains(KeyModifiers::CONTROL) => {
                self.sel[i] = self.move_cursor(10, len)
            }
            KeyCode::Char('u') if k.modifiers.contains(KeyModifiers::CONTROL) => {
                self.sel[i] = self.move_cursor(-10, len)
            }
            KeyCode::Char('g') | KeyCode::Home => self.sel[i] = 0,
            KeyCode::Char('G') | KeyCode::End => self.sel[i] = len.saturating_sub(1),
            KeyCode::Char('/') => {
                self.typing_filter = true;
                self.filter.clear();
            }
            KeyCode::Esc => {
                self.filter.clear();
            }
            KeyCode::Char('r') => self.refresh_all(),
            _ => match self.tab {
                Tab::Prs => self.pr_key(k),
                Tab::Pipelines => self.pipe_key(k),
                Tab::Releases => self.rel_key(k),
            },
        }
    }

    fn switch(&mut self, t: Tab) {
        self.tab = t;
        self.filter.clear();
        self.refresh_all();
    }

    fn pr_key(&mut self, k: KeyEvent) {
        let Some(pr) = self.sel_pr() else { return };
        let (repo, id, title) = (pr.repo_id(), pr.pull_request_id, pr.title.clone());
        let repo_name = pr.repository.name.clone();
        let sha = pr.last_merge_source_commit.commit_id.clone();
        match k.code {
            KeyCode::Char('m') => {
                self.mine_only = !self.mine_only;
                self.sel[0] = 0;
            }
            KeyCode::Char('a') => self.run(Action::Vote(repo, id, 10)),
            KeyCode::Char('x') => self.confirm(
                "Reject PR",
                vec![format!("!{id} {title}")],
                Action::Vote(repo, id, -10),
            ),
            KeyCode::Char('w') => self.run(Action::Vote(repo, id, -5)),
            KeyCode::Char('c') => self.confirm(
                "Complete PR (merge)",
                vec![
                    format!("!{id} {title}"),
                    format!("repo {repo_name} · source branch will be deleted"),
                ],
                Action::CompletePr(repo, id, sha),
            ),
            KeyCode::Char('D') => self.confirm(
                "Abandon PR",
                vec![format!("!{id} {title}")],
                Action::AbandonPr(repo, id),
            ),
            KeyCode::Char('o') => {
                let url = self.client.pr_url(&repo_name, id);
                self.open(url);
            }
            KeyCode::Enter => {
                self.view = View::Diff;
                self.pr = None;
                self.changes.clear();
                self.diff.clear();
                self.go(
                    move |c| async move { c.pull_request(&repo, id).await.map(Box::new) },
                    Msg::PrDetail,
                );
            }
            _ => {}
        }
    }

    fn pipe_key(&mut self, k: KeyEvent) {
        match k.code {
            KeyCode::Char('p') => {
                self.show_defs = !self.show_defs;
                self.sel[1] = 0;
                if self.defs.is_empty() {
                    self.go(|c| async move { c.definitions().await }, Msg::Defs);
                }
            }
            KeyCode::Char('R') => {
                let (id, name) = if self.show_defs {
                    self.sel_def().map(|d| (d.id, d.name.clone()))
                } else {
                    self.sel_build().map(|b| {
                        (
                            b.definition.id.as_i64().unwrap_or(0),
                            b.definition.name.clone(),
                        )
                    })
                }
                .unwrap_or((0, String::new()));
                if id == 0 {
                    return;
                }
                let branch = self
                    .sel_build()
                    .map(|b| short_branch(&b.source_branch))
                    .filter(|_| !self.show_defs)
                    .unwrap_or_else(|| "master".into());
                self.modal = Some(Modal::Input {
                    title: format!("Run {name} on branch:"),
                    value: branch,
                    def: (id, name),
                });
            }
            KeyCode::Char('x') => {
                if let Some(b) = self.sel_build() {
                    let (id, name) = (b.id, b.definition.name.clone());
                    if b.status == "completed" {
                        self.status = "build already finished".into();
                        self.is_err = true;
                        return;
                    }
                    self.confirm(
                        "Cancel run",
                        vec![format!("{name} #{id}")],
                        Action::CancelBuild(id),
                    );
                }
            }
            KeyCode::Char('o') => {
                if let Some(b) = self.sel_build() {
                    let url = self.client.build_url(b.id);
                    self.open(url);
                }
            }
            KeyCode::Char('l') => {
                if let Some(b) = self.sel_build() {
                    let id = b.id;
                    let name = b.definition.name.clone();
                    self.build_id = id;
                    self.log_title = format!("{name} #{id} · log of the step that failed");
                    self.go(
                        move |c| async move {
                            let tl = c.timeline(id).await?;
                            let rec = tl
                                .iter()
                                .find(|r| r.result == "failed" && r.log_id().is_some())
                                .or_else(|| tl.iter().rev().find(|r| r.log_id().is_some()));
                            match rec.and_then(|r| r.log_id()) {
                                Some(log) => c.log(id, log).await,
                                None => Ok("(sem logs disponíveis)".into()),
                            }
                        },
                        Msg::Log,
                    );
                }
            }
            KeyCode::Enter => {
                if let Some(b) = self.sel_build() {
                    let id = b.id;
                    self.build_id = id;
                    self.view = View::Timeline;
                    self.timeline.clear();
                    self.go(move |c| async move { c.timeline(id).await }, Msg::Timeline);
                }
            }
            _ => {}
        }
    }

    fn rel_key(&mut self, k: KeyEvent) {
        let Some(row) = self.sel_rel() else { return };
        match (row, k.code) {
            (RelRow::Approval(i), KeyCode::Char('a')) => {
                let a = &self.approvals[i];
                self.confirm(
                    "Approve deploy",
                    vec![
                        format!(
                            "{} → stage {}",
                            a.release_definition.name, a.release_environment.name
                        ),
                        format!("{} · {}", a.release.name, a.approval_type),
                    ],
                    Action::Approval(a.id, true),
                );
            }
            (RelRow::Approval(i), KeyCode::Char('x')) => {
                let a = &self.approvals[i];
                self.confirm(
                    "Reject deploy",
                    vec![format!(
                        "{} → stage {}",
                        a.release_definition.name, a.release_environment.name
                    )],
                    Action::Approval(a.id, false),
                );
            }
            (RelRow::Approval(i), KeyCode::Char('o') | KeyCode::Enter) => {
                let url = self
                    .client
                    .release_url(self.approvals[i].release.id.as_i64().unwrap_or(0));
                self.open(url);
            }
            (RelRow::Release(i), KeyCode::Char('o') | KeyCode::Enter) => {
                let url = self.client.release_url(self.releases[i].id);
                self.open(url);
            }
            (RelRow::Release(i), KeyCode::Char('d')) => {
                let r = &self.releases[i];
                if r.environments.is_empty() {
                    return;
                }
                let labels: Vec<String> = r
                    .environments
                    .iter()
                    .map(|e| format!("{} · {} ({})", r.release_definition.name, e.name, e.status))
                    .collect();
                let actions: Vec<Action> = r
                    .environments
                    .iter()
                    .zip(labels.iter())
                    .map(|(e, l)| Action::Deploy(r.id, e.id, l.clone()))
                    .collect();
                self.modal = Some(Modal::Select {
                    title: format!("Deploy {} to which stage?", r.name),
                    labels,
                    actions,
                    sel: 0,
                });
            }
            _ => {}
        }
    }

    fn diff_key(&mut self, k: KeyEvent) {
        let files = self.changes.len();
        match k.code {
            KeyCode::Esc | KeyCode::Char('q') => {
                self.view = View::List;
                self.search.clear();
            }
            KeyCode::Char('J') | KeyCode::Right | KeyCode::Tab => {
                if files > 0 {
                    self.change_sel = (self.change_sel + 1) % files;
                    self.load_diff();
                }
            }
            KeyCode::Char('K') | KeyCode::Left => {
                if files > 0 {
                    self.change_sel = (self.change_sel + files - 1) % files;
                    self.load_diff();
                }
            }
            KeyCode::Char('j') | KeyCode::Down => {
                self.diff_scroll = (self.diff_scroll + 1).min(self.diff.len().saturating_sub(1))
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.diff_scroll = self.diff_scroll.saturating_sub(1)
            }
            KeyCode::Char('d') if k.modifiers.contains(KeyModifiers::CONTROL) => {
                self.diff_scroll = (self.diff_scroll + 20).min(self.diff.len().saturating_sub(1))
            }
            KeyCode::Char('u') if k.modifiers.contains(KeyModifiers::CONTROL) => {
                self.diff_scroll = self.diff_scroll.saturating_sub(20)
            }
            KeyCode::Char('g') => self.diff_scroll = 0,
            KeyCode::Char('G') => self.diff_scroll = self.diff.len().saturating_sub(1),
            KeyCode::Char('/') => {
                self.typing_search = true;
                self.search.clear();
            }
            KeyCode::Char('n') => self.find_next(),
            KeyCode::Char('o') => {
                if let Some(pr) = &self.pr {
                    let url = self.client.pr_url(&pr.repository.name, pr.pull_request_id);
                    self.open(url);
                }
            }
            _ => {}
        }
    }

    fn timeline_key(&mut self, k: KeyEvent) {
        let len = self.timeline.len();
        match k.code {
            KeyCode::Esc | KeyCode::Char('q') => self.view = View::List,
            KeyCode::Char('j') | KeyCode::Down => {
                self.tl_sel = (self.tl_sel + 1).min(len.saturating_sub(1))
            }
            KeyCode::Char('k') | KeyCode::Up => self.tl_sel = self.tl_sel.saturating_sub(1),
            KeyCode::Char('r') => {
                let id = self.build_id;
                self.go(move |c| async move { c.timeline(id).await }, Msg::Timeline);
            }
            KeyCode::Char('o') => {
                let url = self.client.build_url(self.build_id);
                self.open(url);
            }
            KeyCode::Enter | KeyCode::Char('l') => {
                let Some(rec) = self.timeline.get(self.tl_sel) else {
                    return;
                };
                let Some(log) = rec.log_id() else {
                    self.status = "this step has no log".into();
                    self.is_err = true;
                    return;
                };
                let id = self.build_id;
                self.log_title = format!("#{id} · {}", rec.name);
                self.go(move |c| async move { c.log(id, log).await }, Msg::Log);
            }
            _ => {}
        }
    }

    fn log_key(&mut self, k: KeyEvent) {
        let len = self.log.len();
        match k.code {
            KeyCode::Esc | KeyCode::Char('q') => {
                self.view = if self.timeline.is_empty() {
                    View::List
                } else {
                    View::Timeline
                };
                self.search.clear();
            }
            KeyCode::Char('j') | KeyCode::Down => {
                self.log_scroll = (self.log_scroll + 1).min(len.saturating_sub(1))
            }
            KeyCode::Char('k') | KeyCode::Up => self.log_scroll = self.log_scroll.saturating_sub(1),
            KeyCode::Char('d') if k.modifiers.contains(KeyModifiers::CONTROL) => {
                self.log_scroll = (self.log_scroll + 20).min(len.saturating_sub(1))
            }
            KeyCode::Char('u') if k.modifiers.contains(KeyModifiers::CONTROL) => {
                self.log_scroll = self.log_scroll.saturating_sub(20)
            }
            KeyCode::Char('g') => self.log_scroll = 0,
            KeyCode::Char('G') => self.log_scroll = len.saturating_sub(1),
            KeyCode::Char('/') => {
                self.typing_search = true;
                self.search.clear();
            }
            KeyCode::Char('n') => self.find_next(),
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diff_marca_adicoes_e_remocoes() {
        let d = diff_lines("a\nb\nc\n", "a\nB\nc\n");
        let kinds: String = d.iter().map(|l| l.kind).collect();
        assert!(kinds.contains('@'), "esperava cabeçalho de hunk: {kinds}");
        assert!(
            kinds.contains('-') && kinds.contains('+'),
            "kinds = {kinds}"
        );
        assert!(d.iter().any(|l| l.kind == '-' && l.text == "b"));
        assert!(d.iter().any(|l| l.kind == '+' && l.text == "B"));
        // arquivo novo: tudo adição
        let novo = diff_lines("", "x\ny\n");
        assert_eq!(novo.iter().filter(|l| l.kind == '+').count(), 2);
        // sem mudança
        assert_eq!(diff_lines("a\n", "a\n")[0].kind, 'i');
        // binário
        assert_eq!(diff_lines("a", "\0b")[0].text, "(binary file)");
    }

    #[test]
    fn ago_formata_intervalos() {
        let now = chrono::Utc::now();
        let mk = |d: chrono::Duration| (now - d).to_rfc3339();
        assert_eq!(ago(&mk(chrono::Duration::seconds(30))), "30s");
        assert_eq!(ago(&mk(chrono::Duration::minutes(5))), "5m");
        assert_eq!(ago(&mk(chrono::Duration::hours(3))), "3h");
        assert_eq!(ago(&mk(chrono::Duration::days(2))), "2d");
        assert_eq!(ago("lixo"), "");
        assert_eq!(dur("2026-01-01T00:00:00Z", "2026-01-01T00:02:05Z"), "2:05");
        assert_eq!(short_branch("refs/heads/master"), "master");
    }
}

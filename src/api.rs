use anyhow::{Result, anyhow};
use base64::Engine;
use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};

const API: &str = "7.1";

#[derive(Deserialize)]
struct List<T> {
    #[serde(default = "Vec::new")]
    value: Vec<T>,
}

#[derive(Debug, Deserialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct Named {
    #[serde(default)]
    pub id: Value,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub display_name: String,
}

#[derive(Debug, Deserialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct Commit {
    #[serde(default)]
    pub commit_id: String,
}

#[derive(Debug, Deserialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct Reviewer {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub vote: i32,
}

#[derive(Debug, Deserialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct PullRequest {
    pub pull_request_id: i64,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub is_draft: bool,
    #[serde(default)]
    pub merge_status: String,
    #[serde(default)]
    pub creation_date: String,
    #[serde(default)]
    pub source_ref_name: String,
    #[serde(default)]
    pub target_ref_name: String,
    #[serde(default)]
    pub created_by: Named,
    #[serde(default)]
    pub repository: Named,
    #[serde(default)]
    pub reviewers: Vec<Reviewer>,
    #[serde(default)]
    pub last_merge_source_commit: Commit,
    #[serde(default)]
    pub last_merge_target_commit: Commit,
}

impl PullRequest {
    pub fn repo_id(&self) -> String {
        self.repository.id.as_str().unwrap_or_default().to_string()
    }
    /// (aprovações, rejeições/aguardando autor)
    pub fn votes(&self) -> (usize, usize) {
        let up = self.reviewers.iter().filter(|r| r.vote > 0).count();
        let down = self.reviewers.iter().filter(|r| r.vote < 0).count();
        (up, down)
    }
}

#[derive(Debug, Deserialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct Item {
    #[serde(default)]
    pub path: String,
    #[serde(default)]
    pub is_folder: bool,
}

#[derive(Debug, Deserialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct Change {
    #[serde(default)]
    pub change_type: String,
    #[serde(default)]
    pub item: Item,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Changes {
    #[serde(default = "Vec::new")]
    change_entries: Vec<Change>,
}

#[derive(Debug, Deserialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct Build {
    pub id: i64,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub result: String,
    #[serde(default)]
    pub source_branch: String,
    #[serde(default)]
    pub definition: Named,
    #[serde(default)]
    pub requested_for: Named,
    #[serde(default)]
    pub queue_time: String,
    #[serde(default)]
    pub start_time: String,
    #[serde(default)]
    pub finish_time: String,
}

#[derive(Debug, Deserialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct Definition {
    pub id: i64,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub path: String,
    #[serde(default)]
    pub queue_status: String,
}

#[derive(Debug, Deserialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct Record {
    #[serde(rename = "type", default)]
    pub kind: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub state: String,
    #[serde(default)]
    pub result: String,
    #[serde(default)]
    pub order: i64,
    #[serde(default)]
    pub start_time: String,
    #[serde(default)]
    pub finish_time: String,
    #[serde(default)]
    pub log: Option<Named>,
}

impl Record {
    pub fn log_id(&self) -> Option<i64> {
        self.log.as_ref()?.id.as_i64()
    }
}

#[derive(Debug, Deserialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct Approval {
    pub id: i64,
    #[serde(default)]
    pub approval_type: String,
    #[serde(default)]
    pub created_on: String,
    #[serde(default)]
    pub release: Named,
    #[serde(default)]
    pub release_definition: Named,
    #[serde(default)]
    pub release_environment: Named,
}

#[derive(Debug, Deserialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct Environment {
    pub id: i64,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub status: String,
}

#[derive(Debug, Deserialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct Release {
    pub id: i64,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub created_on: String,
    #[serde(default)]
    pub release_definition: Named,
    #[serde(default)]
    pub environments: Vec<Environment>,
}

pub struct Client {
    http: reqwest::Client,
    core: String,
    vsrm: String,
    vssps: String,
    pub web: String,
    auth: String,
    pub dry_run: bool,
}

impl Client {
    pub fn new(org: &str, project: &str, pat: &str, dry_run: bool) -> Result<Self> {
        Ok(Self {
            http: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .build()?,
            core: format!("https://dev.azure.com/{org}/{project}/_apis"),
            vsrm: format!("https://vsrm.dev.azure.com/{org}/{project}/_apis"),
            vssps: format!("https://vssps.dev.azure.com/{org}/_apis"),
            web: format!("https://dev.azure.com/{org}/{project}"),
            // PAT vira Basic; um bearer JWT (az account get-access-token) vai como Bearer
            auth: if pat.starts_with("ey") && pat.matches('.').count() == 2 {
                format!("Bearer {pat}")
            } else {
                format!(
                    "Basic {}",
                    base64::engine::general_purpose::STANDARD.encode(format!(":{pat}"))
                )
            },
            dry_run,
        })
    }

    fn req(
        &self,
        method: reqwest::Method,
        url: &str,
        q: &[(&str, &str)],
    ) -> reqwest::RequestBuilder {
        self.http
            .request(method, url)
            .header("Authorization", &self.auth)
            .query(&[("api-version", API)])
            .query(q)
    }

    async fn get<T: DeserializeOwned>(&self, url: &str, q: &[(&str, &str)]) -> Result<T> {
        let r = self.req(reqwest::Method::GET, url, q).send().await?;
        let status = r.status();
        let body = r.text().await?;
        if !status.is_success() {
            return Err(anyhow!("{status} at {url}: {}", snippet(&body)));
        }
        serde_json::from_str(&body).map_err(|e| anyhow!("unexpected response from {url}: {e}"))
    }

    async fn list<T: DeserializeOwned>(&self, url: &str, q: &[(&str, &str)]) -> Result<Vec<T>> {
        Ok(self.get::<List<T>>(url, q).await?.value)
    }

    /// Texto cru; 404 vira string vazia (arquivo criado ou removido no PR).
    async fn text(&self, url: &str, q: &[(&str, &str)]) -> Result<String> {
        let r = self.req(reqwest::Method::GET, url, q).send().await?;
        if r.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(String::new());
        }
        let status = r.status();
        let body = r.text().await?;
        if !status.is_success() {
            return Err(anyhow!("{status} at {url}: {}", snippet(&body)));
        }
        Ok(body)
    }

    /// Toda escrita passa por aqui — em --dry-run só descreve o request.
    async fn write(
        &self,
        method: reqwest::Method,
        url: &str,
        q: &[(&str, &str)],
        body: Value,
        ok_msg: String,
    ) -> Result<String> {
        if self.dry_run {
            return Ok(format!("[dry-run] {method} {url} {body}"));
        }
        let r = self.req(method, url, q).json(&body).send().await?;
        let status = r.status();
        if !status.is_success() {
            let text = r.text().await.unwrap_or_default();
            return Err(anyhow!("{status}: {}", snippet(&text)));
        }
        Ok(ok_msg)
    }

    // ---- perfil ----

    pub async fn my_id(&self) -> Result<String> {
        let v: Value = self
            .get(&format!("{}/profile/profiles/me", self.vssps), &[])
            .await?;
        Ok(v["id"].as_str().unwrap_or_default().to_string())
    }

    // ---- pull requests ----

    pub async fn pull_requests(&self) -> Result<Vec<PullRequest>> {
        self.list(
            &format!("{}/git/pullrequests", self.core),
            &[("searchCriteria.status", "active"), ("$top", "200")],
        )
        .await
    }

    pub async fn pull_request(&self, repo: &str, id: i64) -> Result<PullRequest> {
        self.get(
            &format!("{}/git/repositories/{repo}/pullrequests/{id}", self.core),
            &[],
        )
        .await
    }

    pub async fn pr_changes(&self, repo: &str, id: i64) -> Result<Vec<Change>> {
        let iters: Vec<Named> = self
            .list(
                &format!(
                    "{}/git/repositories/{repo}/pullrequests/{id}/iterations",
                    self.core
                ),
                &[],
            )
            .await?;
        let last = iters
            .iter()
            .filter_map(|i| i.id.as_i64())
            .max()
            .unwrap_or(1);
        let changes: Changes = self
            .get(
                &format!(
                    "{}/git/repositories/{repo}/pullrequests/{id}/iterations/{last}/changes",
                    self.core
                ),
                &[("$top", "500")],
            )
            .await?;
        Ok(changes
            .change_entries
            .into_iter()
            .filter(|c| !c.item.is_folder && !c.item.path.is_empty())
            .collect())
    }

    pub async fn file_text(&self, repo: &str, path: &str, sha: &str) -> Result<String> {
        if sha.is_empty() {
            return Ok(String::new());
        }
        self.text(
            &format!("{}/git/repositories/{repo}/items", self.core),
            &[
                ("path", path),
                ("versionDescriptor.version", sha),
                ("versionDescriptor.versionType", "commit"),
                ("includeContent", "true"),
                ("$format", "text"),
            ],
        )
        .await
    }

    pub async fn vote(&self, repo: &str, pr: i64, reviewer: &str, vote: i32) -> Result<String> {
        let label = match vote {
            10 => "approved",
            -5 => "marked as waiting for author",
            -10 => "rejected",
            _ => "vote cleared",
        };
        self.write(
            reqwest::Method::PUT,
            &format!(
                "{}/git/repositories/{repo}/pullrequests/{pr}/reviewers/{reviewer}",
                self.core
            ),
            &[],
            json!({ "vote": vote }),
            format!("PR !{pr} {label}"),
        )
        .await
    }

    pub async fn complete_pr(&self, repo: &str, pr: i64, source_commit: &str) -> Result<String> {
        self.write(
            reqwest::Method::PATCH,
            &format!("{}/git/repositories/{repo}/pullrequests/{pr}", self.core),
            &[],
            json!({
                "status": "completed",
                "lastMergeSourceCommit": { "commitId": source_commit },
                "completionOptions": { "deleteSourceBranch": true, "transitionWorkItems": true }
            }),
            format!("PR !{pr} completed"),
        )
        .await
    }

    pub async fn abandon_pr(&self, repo: &str, pr: i64) -> Result<String> {
        self.write(
            reqwest::Method::PATCH,
            &format!("{}/git/repositories/{repo}/pullrequests/{pr}", self.core),
            &[],
            json!({ "status": "abandoned" }),
            format!("PR !{pr} abandoned"),
        )
        .await
    }

    // ---- builds ----

    pub async fn builds(&self) -> Result<Vec<Build>> {
        self.list(
            &format!("{}/build/builds", self.core),
            &[("$top", "60"), ("queryOrder", "queueTimeDescending")],
        )
        .await
    }

    pub async fn definitions(&self) -> Result<Vec<Definition>> {
        let mut d: Vec<Definition> = self
            .list(&format!("{}/build/definitions", self.core), &[])
            .await?;
        d.sort_by_key(|a| a.name.to_lowercase());
        Ok(d)
    }

    pub async fn queue_build(&self, def: i64, name: &str, branch: &str) -> Result<String> {
        self.write(
            reqwest::Method::POST,
            &format!("{}/build/builds", self.core),
            &[],
            json!({ "definition": { "id": def }, "sourceBranch": branch }),
            format!("{name} queued on {branch}"),
        )
        .await
    }

    pub async fn cancel_build(&self, id: i64) -> Result<String> {
        self.write(
            reqwest::Method::PATCH,
            &format!("{}/build/builds/{id}", self.core),
            &[],
            json!({ "status": "cancelling" }),
            format!("build {id} cancelled"),
        )
        .await
    }

    pub async fn timeline(&self, build: i64) -> Result<Vec<Record>> {
        let v: Value = self
            .get(&format!("{}/build/builds/{build}/timeline", self.core), &[])
            .await?;
        let mut records: Vec<Record> = serde_json::from_value(v["records"].clone())?;
        records.sort_by_key(|r| r.order);
        Ok(records)
    }

    pub async fn log(&self, build: i64, log: i64) -> Result<String> {
        self.text(
            &format!("{}/build/builds/{build}/logs/{log}", self.core),
            &[],
        )
        .await
    }

    // ---- releases ----

    pub async fn approvals(&self) -> Result<Vec<Approval>> {
        self.list(
            &format!("{}/release/approvals", self.vsrm),
            &[("statusFilter", "pending")],
        )
        .await
    }

    pub async fn set_approval(&self, id: i64, approved: bool, comment: &str) -> Result<String> {
        let status = if approved { "approved" } else { "rejected" };
        self.write(
            reqwest::Method::PATCH,
            &format!("{}/release/approvals/{id}", self.vsrm),
            &[],
            json!({ "status": status, "comments": comment }),
            format!("approval {id} → {status}"),
        )
        .await
    }

    pub async fn releases(&self) -> Result<Vec<Release>> {
        self.list(
            &format!("{}/release/releases", self.vsrm),
            &[("$expand", "environments"), ("$top", "40")],
        )
        .await
    }

    pub async fn deploy(&self, release: i64, env: i64, label: &str) -> Result<String> {
        self.write(
            reqwest::Method::PATCH,
            &format!(
                "{}/release/releases/{release}/environments/{env}",
                self.vsrm
            ),
            &[],
            json!({ "status": "inProgress" }),
            format!("deploy started: {label}"),
        )
        .await
    }

    // ---- urls do browser ----

    pub fn pr_url(&self, repo_name: &str, id: i64) -> String {
        format!("{}/_git/{repo_name}/pullrequest/{id}", self.web)
    }
    pub fn build_url(&self, id: i64) -> String {
        format!("{}/_build/results?buildId={id}&view=results", self.web)
    }
    pub fn release_url(&self, id: i64) -> String {
        format!(
            "{}/_releaseProgress?releaseId={id}&_a=release-pipeline-progress",
            self.web
        )
    }
}

fn snippet(body: &str) -> String {
    let msg = serde_json::from_str::<Value>(body)
        .ok()
        .and_then(|v| v["message"].as_str().map(str::to_string));
    let s = msg.unwrap_or_else(|| body.to_string());
    s.chars().take(200).collect()
}

#[cfg(test)]
fn live_cfg() -> crate::config::Config {
    let cfg = crate::config::Config::load();
    assert!(
        cfg.is_complete(),
        "configure org/project/PAT (run `azura setup` or export AZDO_ORG/AZDO_PROJECT/AZDO_PAT)"
    );
    cfg
}

#[cfg(test)]
fn live_client() -> Client {
    let c = live_cfg();
    Client::new(&c.org, &c.project, &c.pat, true).unwrap()
}

/// Smoke test contra a org real — ignorado por padrão.
/// `AZDO_PAT=$(az account get-access-token --scope 499b84ac-1321-427f-aa17-267ca6975798/.default --query accessToken -o tsv) cargo test -- --ignored --nocapture`
#[cfg(test)]
mod live {
    use super::live_client as client;

    #[tokio::test]
    #[ignore]
    async fn leitura_ponta_a_ponta() {
        let c = client();

        let me = c.my_id().await.expect("my_id");
        assert!(!me.is_empty());
        println!("profile         {me}");

        let prs = c.pull_requests().await.expect("pull_requests");
        println!("active PRs      {}", prs.len());
        let builds = c.builds().await.expect("builds");
        println!("builds          {}", builds.len());
        let defs = c.definitions().await.expect("definitions");
        println!("definitions     {}", defs.len());
        let approvals = c.approvals().await.expect("approvals");
        println!("approvals       {}", approvals.len());
        let releases = c.releases().await.expect("releases");
        println!("releases        {}", releases.len());

        assert!(!builds.is_empty() && !defs.is_empty());
        let b = &builds[0];
        assert!(
            !b.definition.name.is_empty(),
            "definition.name did not deserialize"
        );
        println!(
            "latest build    {} {} {}",
            b.definition.name, b.status, b.result
        );
        let tl = c.timeline(b.id).await.expect("timeline");
        assert!(!tl.is_empty(), "empty timeline");
        println!(
            "steps           {} (with log: {})",
            tl.len(),
            tl.iter().filter(|r| r.log_id().is_some()).count()
        );
        if let Some(r) = tl.iter().find(|r| r.log_id().is_some()) {
            let log = c.log(b.id, r.log_id().unwrap()).await.expect("log");
            println!("log of '{}'   {} lines", r.name, log.lines().count());
        }

        for a in &approvals {
            println!(
                "  ⚠ {} → {} ({})",
                a.release_definition.name, a.release_environment.name, a.approval_type
            );
            assert!(!a.release_definition.name.is_empty());
        }
        if let Some(r) = releases.first() {
            assert!(!r.name.is_empty());
            println!("release[0]      {} stages={}", r.name, r.environments.len());
        }

        // caminho do diff: PR → arquivos → conteúdo dos dois lados
        let Some(pr) = prs.first() else { return };
        let full = c
            .pull_request(&pr.repo_id(), pr.pull_request_id)
            .await
            .expect("pr detail");
        assert!(
            !full.last_merge_source_commit.commit_id.is_empty(),
            "no merge commits"
        );
        let changes = c
            .pr_changes(&full.repo_id(), full.pull_request_id)
            .await
            .expect("changes");
        println!(
            "PR !{}          {} files",
            full.pull_request_id,
            changes.len()
        );
        let Some(ch) = changes.iter().find(|c| !c.change_type.contains("add")) else {
            return;
        };
        let old = c
            .file_text(
                &full.repo_id(),
                &ch.item.path,
                &full.last_merge_target_commit.commit_id,
            )
            .await
            .expect("file_text base");
        let new = c
            .file_text(
                &full.repo_id(),
                &ch.item.path,
                &full.last_merge_source_commit.commit_id,
            )
            .await
            .expect("file_text head");
        println!("{}  {} → {} bytes", ch.item.path, old.len(), new.len());
        assert!(
            !old.is_empty() || !new.is_empty(),
            "neither side returned content"
        );

        // escrita em dry-run não sai da máquina
        let msg = c.set_approval(1, true, "teste").await.unwrap();
        assert!(msg.starts_with("[dry-run]"), "{msg}");
    }
}

#[cfg(test)]
mod live_filtro {
    use super::live_client;
    use crate::app::{App, Tab};
    use std::sync::Arc;

    /// O filtro só serve se os termos que a pessoa digitaria baterem nos dados reais.
    #[tokio::test]
    #[ignore]
    async fn termos_reais_batem() {
        let c = Arc::new(live_client());
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        let cfg = super::live_cfg();
        let mut app = App::new(c.clone(), tx, cfg.org, cfg.project);
        app.prs = c.pull_requests().await.unwrap();
        app.builds = c.builds().await.unwrap();
        app.releases = c.releases().await.unwrap();
        app.approvals = c.approvals().await.unwrap();

        // um termo tirado dos próprios dados tem que casar, em qualquer org
        let nome = app
            .builds
            .first()
            .map(|b| b.definition.name.clone())
            .unwrap_or_default();
        let branch = app
            .prs
            .first()
            .map(|p| crate::app::short_branch(&p.target_ref_name))
            .unwrap_or_default();

        let mut check = |tab: Tab, termo: &str| {
            app.tab = tab;
            app.filter = termo.into();
            let n = app.row_count();
            let total = app.total_rows();
            println!("  /{termo:<14} {n}/{total}");
            (n, total)
        };

        println!("PRs:");
        for t in [branch.as_str(), "draft", "conflicts"] {
            check(Tab::Prs, t);
        }
        if !branch.is_empty() {
            let (n, _) = check(Tab::Prs, &branch);
            assert!(
                n > 0,
                "filter did not find branch '{branch}' that came from the API"
            );
        }

        println!("pipelines:");
        for t in ["succeeded", "failed", "staging"] {
            check(Tab::Pipelines, t);
        }
        if !nome.is_empty() {
            let (n, total) = check(Tab::Pipelines, &nome);
            assert!(n > 0 && n <= total, "filter did not find pipeline '{nome}'");
        }

        println!("releases:");
        for t in ["production", "pendente", "succeeded"] {
            check(Tab::Releases, t);
        }
    }
}

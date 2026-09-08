//! Testes que olham a tela renderizada, contra um servidor HTTP falso.
//!
//! Os testes de render existentes só garantem que nada dá panic. Estes leem o
//! texto que a pessoa veria — que é onde moraram os bugs que chegaram a release:
//! "loading…" preso na barra de status, título não traduzido, aba não carregada
//! mostrando "(0)".

use crate::api::Client;
use crate::app::{App, PrScope, View};
use crate::ui;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

const EU: &str = "11111111-1111-1111-1111-111111111111";
const OUTRO: &str = "22222222-2222-2222-2222-222222222222";

fn corpo(path: &str) -> String {
    // ordem importa: a rota de threads também contém "pullrequests"
    if path.contains("/threads") {
        return serde_json::json!({"value": [{
            "id": 7, "status": "active", "isDeleted": false,
            "threadContext": {"filePath": "/src/app.ts"},
            "comments": [
                {"author": {"displayName": "Dana Scott"}, "content": "this leaks a handle",
                 "publishedDate": "2026-09-08T10:00:00Z", "commentType": "text"},
                {"author": {"displayName": "Kim Lee"}, "content": "good catch, fixed",
                 "publishedDate": "2026-09-08T11:00:00Z", "commentType": "text"}
            ]
        }]})
        .to_string();
    }
    if path.contains("/policy/evaluations") {
        // o artifactId vem url-encoded: vstfs:%2F%2F%2FCodeReview%2F...%2F<pr>
        let id = path
            .split("artifactId=")
            .nth(1)
            .and_then(|q| q.split('&').next())
            .and_then(|a| a.rsplit(['/', 'F']).next())
            .unwrap_or("")
            .to_string();
        // o PR 2 está pronto; o 1 espera reviewers
        let pr1 = id == "1";
        return serde_json::json!({"value": [{
            "status": if pr1 { "queued" } else { "approved" },
            "configuration": {"isBlocking": true, "type": {"displayName": "Minimum number of reviewers"}}
        }]})
        .to_string();
    }
    if path.contains("/policy/configurations") {
        return serde_json::json!({"value": [{
            "isEnabled": true,
            "type": {"displayName": "Require a merge strategy"},
            "settings": {"allowSquash": true}
        }]})
        .to_string();
    }
    if path.contains("/projects/") {
        return serde_json::json!({"id": "proj-guid"}).to_string();
    }
    if path.contains("/git/pullrequests") {
        return serde_json::json!({"value": [
            {"pullRequestId": 1, "title": "add retry to the uploader", "isDraft": false,
             "mergeStatus": "succeeded", "creationDate": "2026-09-08T09:00:00Z",
             "sourceRefName": "refs/heads/feature/1", "targetRefName": "refs/heads/main",
             "createdBy": {"id": EU, "displayName": "Me"},
             "repository": {"id": "repo-1", "name": "web"},
             "reviewers": []},
            {"pullRequestId": 2, "title": "drop the legacy session store", "isDraft": false,
             "mergeStatus": "succeeded", "creationDate": "2026-09-07T09:00:00Z",
             "sourceRefName": "refs/heads/feature/2", "targetRefName": "refs/heads/main",
             "createdBy": {"id": OUTRO, "displayName": "Dana Scott"},
             "repository": {"id": "repo-1", "name": "api"},
             "reviewers": [{"id": EU, "vote": 0}]}
        ]})
        .to_string();
    }
    if path.contains("/build/builds") {
        return serde_json::json!({"value": [
            {"id": 10, "status": "completed", "result": "failed", "sourceBranch": "refs/heads/main",
             "definition": {"id": 1, "name": "checkout-api"}, "requestedFor": {"displayName": "Me"},
             "queueTime": "2026-09-08T09:00:00Z", "startTime": "2026-09-08T09:00:00Z",
             "finishTime": "2026-09-08T09:02:00Z"}
        ]})
        .to_string();
    }
    if path.contains("/release/approvals") {
        return serde_json::json!({"value": [
            {"id": 99, "approvalType": "preDeploy", "createdOn": "2026-09-08T09:00:00Z",
             "release": {"id": 5, "name": "Release-5"},
             "releaseDefinition": {"id": 1, "name": "checkout-api-prod"},
             "releaseEnvironment": {"id": 2, "name": "Production"}}
        ]})
        .to_string();
    }
    if path.contains("/release/releases") {
        return serde_json::json!({"value": [
            {"id": 5, "name": "Release-5", "createdOn": "2026-09-08T09:00:00Z",
             "releaseDefinition": {"id": 1, "name": "checkout-api-prod"},
             "environments": [{"id": 2, "name": "Production", "status": "queued"}]}
        ]})
        .to_string();
    }
    serde_json::json!({"value": []}).to_string()
}

/// Sobe um servidor que responde JSON canned por rota e devolve a URL base.
async fn servidor() -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        while let Ok((mut sock, _)) = listener.accept().await {
            tokio::spawn(async move {
                let mut buf = vec![0u8; 16384];
                let n = sock.read(&mut buf).await.unwrap_or(0);
                let req = String::from_utf8_lossy(&buf[..n]).to_string();
                let path = req
                    .lines()
                    .next()
                    .and_then(|l| l.split(' ').nth(1))
                    .unwrap_or("")
                    .to_string();
                let body = corpo(&path);
                let resp = format!(
                    "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\n\
                     x-vss-userdata: {EU}:me@example.com\r\ncontent-length: {}\r\n\
                     connection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = sock.write_all(resp.as_bytes()).await;
            });
        }
    });
    format!("http://{addr}")
}

/// Sobe o app como o main sobe e drena as mensagens até o fluxo parar.
type Fila = tokio::sync::mpsc::UnboundedReceiver<crate::app::Msg>;

async fn app_pronto() -> (App, Fila) {
    let base = servidor().await;
    let client = Arc::new(Client::for_tests(&base));
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let mut app = App::new(client, tx, "contoso".into(), "Fabrikam".into());
    drena(&mut app, &mut rx).await;
    (app, rx)
}

/// Consome mensagens até o fluxo parar, como o loop de eventos faria.
async fn drena(app: &mut App, rx: &mut Fila) {
    for _ in 0..300 {
        match tokio::time::timeout(Duration::from_millis(120), rx.recv()).await {
            Ok(Some(m)) => app.on_msg(m),
            _ => break,
        }
    }
}

fn tela(app: &App) -> String {
    let mut term = Terminal::new(TestBackend::new(120, 24)).unwrap();
    term.draw(|f| ui::draw(f, app)).unwrap();
    let buf = term.backend().buffer();
    (0..buf.area.height)
        .map(|y| {
            (0..buf.area.width)
                .map(|x| buf.cell((x, y)).map(|c| c.symbol()).unwrap_or(" "))
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn tecla(c: char) -> KeyEvent {
    KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE)
}

#[tokio::test]
async fn barra_de_status_nao_fica_presa_em_loading() {
    let (app, _rx) = app_pronto().await;
    let t = tela(&app);
    assert!(
        !t.contains("loading…"),
        "status preso em loading depois de tudo carregar:\n{t}"
    );
    assert!(t.contains("add retry to the uploader"), "{t}");
}

#[tokio::test]
async fn aba_nao_carregada_nao_mostra_zero() {
    let (app, _rx) = app_pronto().await;
    let t = tela(&app);
    assert!(
        t.contains("1 PRs (2)"),
        "aba ativa deve mostrar a contagem:\n{t}"
    );
    assert!(
        !t.contains("Pipelines (0)"),
        "aba não visitada não pode alegar zero:\n{t}"
    );
}

#[tokio::test]
async fn m_cicla_os_tres_escopos() {
    let (mut app, _rx) = app_pronto().await;
    assert!(tela(&app).contains("active pull requests"));

    app.on_key(tecla('m'));
    assert_eq!(app.scope, PrScope::Mine);
    let t = tela(&app);
    assert!(t.contains("opened by me"), "{t}");
    assert!(t.contains("add retry"), "o PR que eu abri fica");
    assert!(!t.contains("drop the legacy"), "o PR do outro sai");

    app.on_key(tecla('m'));
    let t = tela(&app);
    assert!(t.contains("waiting for my review"), "{t}");
    assert!(t.contains("drop the legacy"), "sou reviewer e não votei");
    assert!(
        !t.contains("add retry"),
        "PR meu não entra na fila de revisão"
    );

    app.on_key(tecla('m'));
    assert_eq!(app.scope, PrScope::All);
}

#[tokio::test]
async fn coluna_de_policy_diz_o_que_falta() {
    let (app, _rx) = app_pronto().await;
    let t = tela(&app);
    assert!(t.contains("blocked on"), "cabeçalho da coluna:\n{t}");
    assert!(t.contains("reviewers"), "PR pendente mostra a policy:\n{t}");
    assert!(t.contains("ready"), "PR liberado mostra ready:\n{t}");
}

#[tokio::test]
async fn estrategia_de_merge_vem_da_policy() {
    let (app, _rx) = app_pronto().await;
    assert_eq!(
        app.merge_strategy.as_deref(),
        Some("squash"),
        "a policy do projeto permite squash"
    );
}

#[tokio::test]
async fn comentarios_do_pr_aparecem() {
    let (mut app, mut rx) = app_pronto().await;
    app.on_key(tecla('t'));
    drena(&mut app, &mut rx).await;
    assert_eq!(app.view, View::Threads);
    let t = tela(&app);
    assert!(
        t.contains("this leaks a handle"),
        "comentário na tela:\n{t}"
    );
    assert!(t.contains("Dana Scott"), "autor na tela:\n{t}");
    assert!(t.contains("src/app.ts"), "arquivo da thread:\n{t}");
}

/// Guarda de idioma: a varredura por palavras soltas já deixou passar
/// "só os meus". Aqui a tela renderizada é que decide.
#[tokio::test]
async fn nenhuma_tela_fala_portugues() {
    let (mut app, _rx) = app_pronto().await;
    let marcadores = [
        "ção",
        "ções",
        "ões",
        "não",
        "meus",
        "arquivo",
        "voltar",
        "tecla",
        "carregando",
        "passo",
        "nenhum",
        "aprova",
        "atualiz",
    ];
    for _ in 0..3 {
        for view in [
            View::List,
            View::Diff,
            View::Timeline,
            View::Log,
            View::Help,
        ] {
            app.view = view;
            let t = tela(&app);
            for m in marcadores {
                assert!(!t.contains(m), "'{m}' na tela:\n{t}");
            }
        }
        app.view = View::List;
        app.on_key(tecla('m'));
    }
}

#[tokio::test]
async fn responder_thread_envia_o_texto() {
    let (mut app, mut rx) = app_pronto().await;
    app.on_key(tecla('t'));
    drena(&mut app, &mut rx).await;
    assert!(!app.threads.is_empty(), "as threads precisam ter carregado");

    // R abre o campo, o texto é digitado tecla a tecla, enter envia
    app.on_key(KeyEvent::new(KeyCode::Char('R'), KeyModifiers::SHIFT));
    let t = tela(&app);
    assert!(t.contains("Reply on thread"), "modal de resposta:\n{t}");
    for c in "lgtm".chars() {
        app.on_key(tecla(c));
    }
    assert!(tela(&app).contains("lgtm"), "o texto digitado aparece");

    app.on_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    drena(&mut app, &mut rx).await;
    let t = tela(&app);
    assert!(
        t.contains("replied on thread 7"),
        "a barra de status confirma o envio:\n{t}"
    );
}

#[tokio::test]
async fn resposta_vazia_nao_e_enviada() {
    let (mut app, mut rx) = app_pronto().await;
    app.on_key(tecla('t'));
    drena(&mut app, &mut rx).await;
    app.on_key(KeyEvent::new(KeyCode::Char('R'), KeyModifiers::SHIFT));
    app.on_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    drena(&mut app, &mut rx).await;
    assert!(
        !tela(&app).contains("replied on thread"),
        "enter sem texto não deve postar comentário vazio"
    );
}

#[tokio::test]
async fn r_escolhe_o_repo() {
    let (mut app, _rx) = app_pronto().await;
    assert_eq!(app.repos(), vec!["api".to_string(), "web".to_string()]);

    let r = KeyEvent::new(KeyCode::Char('R'), KeyModifiers::SHIFT);
    let enter = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);

    app.on_key(r);
    let t = tela(&app);
    assert!(t.contains("Show pull requests from"), "seletor abre:\n{t}");
    assert!(
        t.contains("all repos (2)"),
        "opção de limpar o filtro:\n{t}"
    );
    assert!(
        t.contains("api (1)") && t.contains("web (1)"),
        "repos com contagem:\n{t}"
    );

    // primeira opção é "todos"; desce uma e escolhe o primeiro repo
    app.on_key(tecla('j'));
    app.on_key(enter);
    let t = tela(&app);
    assert!(
        t.contains("· api"),
        "repo escolhido vai para o título:\n{t}"
    );
    assert!(
        t.contains("drop the legacy") && !t.contains("add retry"),
        "{t}"
    );

    // reabrir já vem posicionado no repo atual, não no topo
    app.on_key(r);
    app.on_key(enter);
    assert_eq!(
        app.repo_filter.as_deref(),
        Some("api"),
        "reabrir e confirmar mantém o repo atual"
    );

    // subir até "all repos" limpa o filtro
    app.on_key(r);
    app.on_key(tecla('k'));
    app.on_key(enter);
    assert!(
        app.repo_filter.is_none(),
        "escolher 'all repos' limpa o filtro"
    );
    let t = tela(&app);
    assert!(
        t.contains("add retry") && t.contains("drop the legacy"),
        "{t}"
    );
}

#[tokio::test]
async fn repo_e_escopo_se_combinam() {
    let (mut app, _rx) = app_pronto().await;
    app.on_key(KeyEvent::new(KeyCode::Char('R'), KeyModifiers::SHIFT));
    app.on_key(tecla('j'));
    app.on_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    // repo api + escopo "abertos por mim": o PR do api é de outra pessoa
    app.on_key(tecla('m'));
    assert_eq!(app.scope, PrScope::Mine);
    let t = tela(&app);
    assert!(
        t.contains("no pull requests in this repo"),
        "os dois filtros somam:\n{t}"
    );
}

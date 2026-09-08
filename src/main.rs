mod api;
mod app;
mod config;
#[cfg(test)]
mod e2e;
mod setup;
#[cfg(test)]
mod shot;
mod ui;

use anyhow::Result;
use app::{App, Msg};
use crossterm::event::{Event, EventStream, KeyEventKind};
use futures::StreamExt;
use std::sync::Arc;
use std::time::Duration;

const USAGE: &str = "\
azura — Azure DevOps pull requests, pipelines and releases in your terminal

Usage:
  azura              open the TUI
  azura setup        reconfigure organization, project and token
  azura --dry-run    never send writes, just describe them

Flags:
  -n, --dry-run      describe every write instead of sending it
  -V, --version      print the version and exit
  -h, --help         print this help and exit

Config  ~/.config/azura/config.toml, or AZDO_ORG / AZDO_PROJECT / AZDO_PAT
Keys    press ? inside the app
";

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let dry_run = args.iter().any(|a| a == "--dry-run" || a == "-n");
    let force_setup = args.iter().any(|a| a == "setup" || a == "--setup");

    if args.iter().any(|a| a == "--version" || a == "-V") {
        println!("azura {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }
    if args.iter().any(|a| a == "--help" || a == "-h") {
        print!("{USAGE}");
        return Ok(());
    }

    let mut cfg = config::Config::load();
    let mut terminal = ratatui::init();

    if force_setup || !cfg.is_complete() {
        match setup::run(&mut terminal, cfg.clone()).await {
            Ok(Some(novo)) => {
                cfg = novo;
                if let Err(e) = cfg.save() {
                    ratatui::restore();
                    eprintln!("config validated but could not be saved: {e}");
                    std::process::exit(1);
                }
            }
            Ok(None) => {
                ratatui::restore();
                return Ok(());
            }
            Err(e) => {
                ratatui::restore();
                return Err(e);
            }
        }
    }

    let client = Arc::new(api::Client::new(&cfg.org, &cfg.project, &cfg.pat, dry_run)?);
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<Msg>();
    let mut app = App::new(client, tx.clone(), cfg.org.clone(), cfg.project.clone());

    // auto-refresh e spinner
    let t = tx.clone();
    tokio::spawn(async move {
        let mut refresh = tokio::time::interval(Duration::from_secs(30));
        let mut spin = tokio::time::interval(Duration::from_millis(120));
        refresh.tick().await;
        loop {
            tokio::select! {
                _ = refresh.tick() => { if t.send(Msg::Tick).is_err() { break } }
                _ = spin.tick() => { if t.send(Msg::Spin).is_err() { break } }
            }
        }
    });

    let mut events = EventStream::new();
    terminal.draw(|f| ui::draw(f, &app))?;

    while !app.quit {
        let mut dirty = false;
        tokio::select! {
            Some(msg) = rx.recv() => {
                app.on_msg(msg);
                dirty = true;
            }
            Some(Ok(ev)) = events.next() => {
                match ev {
                    Event::Key(k) if k.kind == KeyEventKind::Press => {
                        app.on_key(k);
                        dirty = true;
                    }
                    Event::Resize(_, _) => dirty = true,
                    _ => {}
                }
            }
        }
        if dirty {
            terminal.draw(|f| ui::draw(f, &app))?;
        }
    }

    ratatui::restore();
    Ok(())
}

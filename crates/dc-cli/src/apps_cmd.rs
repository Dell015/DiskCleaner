//! `dc apps`: list installed programs (read-only).

use std::fmt::Write as _;
use std::fs::OpenOptions;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::{bail, Context, Result};
use dc_winmeta::{AppSnapshot, InstalledApp, SNAPSHOT_SCHEMA_VERSION};

pub struct AppsArgs {
    pub out: Option<PathBuf>,
    pub from: Option<PathBuf>,
    pub include_hidden: bool,
}

#[cfg(windows)]
fn read_system() -> Result<AppSnapshot> {
    Ok(dc_winmeta::build_snapshot(&dc_winmeta::WindowsSource))
}

#[cfg(not(windows))]
fn read_system() -> Result<AppSnapshot> {
    bail!("`dc apps` reads the Windows registry and only works on Windows; use --from FILE to show a saved snapshot")
}

fn load(path: &Path) -> Result<AppSnapshot> {
    let text =
        std::fs::read_to_string(path).with_context(|| format!("cannot read {}", path.display()))?;
    let snapshot: AppSnapshot = serde_json::from_str(&text)
        .with_context(|| format!("{} is not a DiskCleaner apps snapshot", path.display()))?;
    if snapshot.schema_version != SNAPSHOT_SCHEMA_VERSION {
        bail!(
            "{} has snapshot version {}, this dc understands version {}",
            path.display(),
            snapshot.schema_version,
            SNAPSHOT_SCHEMA_VERSION
        );
    }
    Ok(snapshot)
}

fn clip(text: &str, width: usize) -> String {
    if text.chars().count() <= width {
        return text.to_owned();
    }
    let mut short: String = text.chars().take(width.saturating_sub(1)).collect();
    short.push('~');
    short
}

fn flag_text(app: &InstalledApp) -> String {
    let f = &app.flags;
    let mut parts = Vec::new();
    if f.stale_registration {
        parts.push("STALE");
    }
    if f.no_install_folder {
        parts.push("no-folder");
    }
    if f.system_component {
        parts.push("system");
    }
    if f.update_or_patch {
        parts.push("update");
    }
    if f.windows_installer {
        parts.push("msi");
    }
    if f.hidden_entry {
        parts.push("hidden");
    }
    parts.join(",")
}

fn shown(app: &InstalledApp, include_hidden: bool) -> bool {
    include_hidden || !(app.flags.hidden_entry || app.flags.update_or_patch)
}

/// The text table printed by `dc apps`.
pub fn render(snapshot: &AppSnapshot, include_hidden: bool) -> String {
    let rows: Vec<&InstalledApp> = snapshot
        .apps
        .iter()
        .filter(|app| shown(app, include_hidden))
        .collect();
    let mut out = String::new();
    let _ = writeln!(
        out,
        "{:<34} {:<22} {:<14} {:<18} FOLDER",
        "NAME", "PUBLISHER", "VERSION", "FLAGS"
    );
    for app in &rows {
        let name = if app.name.is_empty() {
            &app.key
        } else {
            &app.name
        };
        let _ = writeln!(
            out,
            "{:<34} {:<22} {:<14} {:<18} {}",
            clip(name, 34),
            clip(app.publisher.as_deref().unwrap_or("-"), 22),
            clip(app.version.as_deref().unwrap_or("-"), 14),
            clip(&flag_text(app), 18),
            app.install_folder.as_deref().unwrap_or("-"),
        );
    }
    let hidden = snapshot.apps.len() - rows.len();
    let stale = snapshot
        .apps
        .iter()
        .filter(|a| a.flags.stale_registration)
        .count();
    let _ = writeln!(
        out,
        "\n{} apps shown, {} hidden (updates and components; use --include-hidden), {} with a missing install folder",
        rows.len(),
        hidden,
        stale
    );
    for problem in &snapshot.problems {
        let _ = writeln!(out, "WARNING   {}: {}", problem.source, problem.message);
    }
    out
}

pub fn run(args: AppsArgs) -> Result<ExitCode> {
    let snapshot = match &args.from {
        Some(path) => load(path)?,
        None => read_system()?,
    };

    print!("{}", render(&snapshot, args.include_hidden));

    if let Some(out) = &args.out {
        // `create_new`: never overwrite an existing file.
        let file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(out)
            .with_context(|| {
                format!(
                    "cannot create {} (it must not already exist)",
                    out.display()
                )
            })?;
        let mut writer = BufWriter::new(file);
        serde_json::to_writer_pretty(&mut writer, &snapshot).context("writing snapshot JSON")?;
        writer.write_all(b"\n")?;
        writer.flush()?;
        println!(
            "snapshot  {} apps -> {}",
            snapshot.apps.len(),
            out.display()
        );
    }

    Ok(if snapshot.problems.is_empty() {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(2)
    })
}

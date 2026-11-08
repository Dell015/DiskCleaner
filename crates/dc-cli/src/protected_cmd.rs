//! `dc protected`: ask the protected-path engine about paths (read-only).

use std::process::ExitCode;

use anyhow::Result;
use dc_protected::{FsResolver, ProtectedList, BUILTIN};

pub fn build_list() -> ProtectedList {
    let list = ProtectedList::builtin();
    #[cfg(windows)]
    let list = {
        use dc_winmeta::SystemInfoSource;
        let mut list = list;
        for folder in dc_winmeta::WindowsSource.known_folders() {
            if matches!(folder.name.as_str(), "Windows" | "System") {
                if let Some(path) = folder.path {
                    list = list.with_extra(
                        &path,
                        "known_system_folder",
                        "Windows system folder on this computer",
                    );
                }
            }
        }
        list
    };
    list
}

pub fn run(paths: &[String], list_only: bool) -> Result<ExitCode> {
    if list_only {
        println!("{:<9} {:<28} {:<28} why", "reach", "pattern", "id");
        for (pattern, scope, id, description) in BUILTIN {
            println!(
                "{:<9} {:<28} {:<28} {description}",
                scope.name(),
                pattern,
                id
            );
        }
        return Ok(ExitCode::SUCCESS);
    }
    let list = build_list();
    for warning in list.skipped_extras() {
        eprintln!("warning: could not use protected entry {warning}");
    }
    for path in paths {
        let verdict = list.check(path, &FsResolver);
        if verdict.protected {
            let ids = verdict.reason_ids().join(", ");
            println!("PROTECTED      {path}   [{ids}]");
            for reason in &verdict.reasons {
                println!("                 - {}", reason.description);
            }
        } else {
            println!("not protected  {path}   (not protected does NOT mean safe to delete)");
        }
    }
    Ok(ExitCode::SUCCESS)
}

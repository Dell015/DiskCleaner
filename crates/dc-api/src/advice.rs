//! Plain-language answers about a finished scan: what takes the space, what an entry is, who it
//! belongs to, what the checks say, and what would happen if it were deleted or moved. Shared by
//! the scan report and the MCP server. Read-only.

use serde::Serialize;

use crate::inspect::{Entry, EvidenceInfo, Inspector, MarkInfo};

const DAY: i64 = 86_400;

/// What would happen if something with this reason were deleted.
#[must_use]
pub fn if_deleted(reason: &str) -> Option<&'static str> {
    Some(match reason {
        "temp_files" => "Temporary files programs left behind. Nothing you use breaks; a program running right now may still hold a few of them (files in use are skipped).",
        "crash_dumps" => "Records of past program crashes, kept for troubleshooting. Nothing breaks; only old crash details are lost.",
        "error_reports" => "Old Windows error reports. Nothing breaks; only old problem details are lost.",
        "thumbnail_cache" => "Picture and icon previews. Windows rebuilds them when folders are opened; the first open is a little slower.",
        "web_cache" | "browser_cache" => "The browser's copy of pages and images. They are downloaded again when needed, so sites may load slower once. Logins and history are not in the cache and stay.",
        "app_cache" => "An app's cache. The app rebuilds it; it may start slower once or download some content again.",
        "shader_cache" => "Compiled graphics shaders. Games and the graphics driver rebuild them; the first launches may stutter briefly.",
        "package_cache" => "Downloaded packages kept by a developer tool or package manager. They are downloaded again when needed (needs internet).",
        "old_installers" => "Installer copies kept after updates. Repairing or removing an old version may ask for its installer again.",
        "old_logs" => "Old log files. Nothing breaks.",
        "windows_upgrade_leftover" => "Files from a previous Windows version or upgrade. After removing them you can no longer go back to that version.",
        "browser_profile_data" => "Your browser profile: logins, history, bookmarks, open tabs. Deleting it can lose them for good. Do not delete.",
        "wallet_or_vault" => "Keys or a password vault. Deleting it can lose access to accounts or money for good. Do not delete.",
        "leftover_missing_app" => "Data of an app whose program folder is already gone, so the app cannot use it. If you reinstall that app later, its old settings are gone.",
        "leftover_backup_copy" => "An old backup copy next to the folder it was made from; the real folder is not touched. Look inside first in case it holds something the current one does not.",
        "leftover_known_app_removed" => "Data left by an app that is no longer installed. If you reinstall it, its old settings and saved data are gone.",
        "leftover_unregistered_program" => "Programs that no installed app claims. If you still use one of them as a portable tool, it is gone.",
        "may_hold_personal_data" => "It may hold your own documents, pictures or other personal files, so DiskCleaner only shows it. Look inside before deciding.",
        "may_hold_installed_games" => "Apps like this keep installed games or other data you may want, so DiskCleaner only shows it.",
        "outside_app_folders" => "It sits among your own files or at the top of a drive, so DiskCleaner only shows it. Look inside before deciding.",
        "low_confidence" => "The evidence is weak, so DiskCleaner only shows it and does not suggest removing it.",
        _ => return None,
    })
}

/// Who owns an entry, as an answer.
#[derive(Debug, Clone, Serialize)]
pub struct OwnerAnswer {
    /// App, publisher or Windows.
    pub name: String,
    /// "app", "vendor" or "windows".
    pub kind: String,
    /// Publisher.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub publisher: Option<String>,
    /// "low", "medium" or "high".
    pub confidence: &'static str,
    /// Set on a folder above this entry (the entry is inside the app's folder).
    pub inherited: bool,
    /// This entry is (inside) the app's registered install folder.
    pub install_folder: bool,
    /// Last started (YYYY-MM-DD), if Windows recorded it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_used: Option<String>,
    /// The app has its own uninstaller.
    pub uninstaller: bool,
}

/// What the checks say, as an answer.
#[derive(Debug, Clone, Serialize)]
pub struct Verdict {
    /// "safe", "probably_safe" or "dont_touch".
    pub risk: String,
    /// Plain words for the risk.
    pub risk_text: &'static str,
    /// Offered as a suggestion (false: shown only or refused).
    pub offered: bool,
    /// Short title.
    pub title: String,
    /// More detail.
    pub detail: String,
    /// "rule", "leftover" or "refused".
    pub source: String,
    /// Why.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub evidence: Vec<EvidenceInfo>,
    /// Where it is set when it comes from a folder above.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub set_on: Option<String>,
}

fn risk_text(risk: &str) -> &'static str {
    match risk {
        "safe" => "Safe: rebuilds itself or is never needed again",
        "probably_safe" => "Probably safe: look at it before removing",
        _ => "Don't touch",
    }
}

fn confidence_text(c: u32) -> &'static str {
    match c {
        0 => "low",
        1 => "medium",
        _ => "high",
    }
}

fn date(secs: i64) -> Option<String> {
    crate::inspect::date_text(secs)
}

/// Plain words for the kind of place a path is.
#[must_use]
pub fn place_kind(path: &str) -> &'static str {
    let k = path.to_lowercase().replace('/', "\\");
    let after_drive = k.get(3..).unwrap_or("");
    if k.len() > 3 && k.as_bytes().get(1) == Some(&b':') && !after_drive.contains('\\') {
        return "top of the drive";
    }
    if k.contains("\\program files\\") || k.contains("\\program files (x86)\\") {
        "program files"
    } else if k.contains("\\appdata\\local\\temp\\") {
        "temporary files"
    } else if k.contains("\\appdata\\") {
        "app data"
    } else if after_drive.starts_with("programdata\\") {
        "shared app data"
    } else if k.contains("\\documents\\my games\\") || k.contains("\\saved games\\") {
        "game saves"
    } else if k.contains("\\documents\\") {
        "documents"
    } else if k.contains("\\downloads\\") {
        "downloads"
    } else if k.contains("\\desktop\\") {
        "desktop"
    } else if after_drive.starts_with("windows\\") {
        "windows"
    } else {
        "elsewhere"
    }
}

const USER_FOLDERS: [&str; 9] = [
    "downloads",
    "documents",
    "desktop",
    "pictures",
    "videos",
    "music",
    "onedrive",
    "saved games",
    "contacts",
];

fn user_folder_at(ins: &Inspector, i: u32) -> bool {
    let Some(p) = ins.parent(i) else { return false };
    let Some(pp) = ins.parent(p) else {
        return false;
    };
    pp > 0 && ins.name(pp).eq_ignore_ascii_case("users") && {
        let n = ins.name(i).to_lowercase();
        USER_FOLDERS.contains(&n.as_str()) || n.starts_with("onedrive")
    }
}

fn in_user_folder(ins: &Inspector, i: u32) -> bool {
    let mut at = Some(i);
    while let Some(a) = at {
        if a == 0 {
            break;
        }
        if user_folder_at(ins, a) {
            return true;
        }
        at = ins.parent(a);
    }
    false
}

/// Everything about one entry.
#[derive(Debug, Clone, Serialize)]
pub struct Explanation {
    /// The entry.
    #[serde(flatten)]
    pub entry: Entry,
    /// Kind of place.
    pub place: &'static str,
    /// Who it belongs to, if anyone.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub owner: Option<OwnerAnswer>,
    /// What the checks say, if anything.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verdict: Option<Verdict>,
    /// What deleting it would do.
    pub if_deleted: String,
    /// What moving it would do.
    pub if_moved: String,
    /// The biggest things inside (folders).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub biggest_inside: Vec<Entry>,
    /// Findings inside this folder: (count, bytes offered as suggestions).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub findings_inside: Option<(usize, u64)>,
    /// It looks like a software project (and how that was recognised).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project: Option<String>,
}

fn owner_answer(ins: &Inspector, i: u32) -> Option<OwnerAnswer> {
    let (o, conf, here) = ins.owner(i)?;
    let path = ins.path(i).to_lowercase();
    let install_folder = o.folder.as_deref().is_some_and(|f| {
        let f = f.to_lowercase();
        let f = f.trim_end_matches('\\');
        path == f || path.starts_with(&format!("{f}\\"))
    });
    Some(OwnerAnswer {
        name: o.name.clone(),
        kind: o.kind.clone(),
        publisher: o.publisher.clone(),
        confidence: confidence_text(conf),
        inherited: !here,
        install_folder,
        last_used: o.last_used.and_then(date),
        uninstaller: o.uninstaller,
    })
}

fn verdict(ins: &Inspector, i: u32) -> Option<Verdict> {
    let (m, at) = ins.mark(i)?;
    Some(verdict_of(ins, m, (at != i).then(|| ins.path(at))))
}

fn verdict_of(_ins: &Inspector, m: &MarkInfo, set_on: Option<String>) -> Verdict {
    Verdict {
        risk: m.risk.clone(),
        risk_text: risk_text(&m.risk),
        offered: m.offered,
        title: m.title.clone(),
        detail: m.detail.clone(),
        source: m.source.clone(),
        evidence: m.evidence.clone(),
        set_on,
    }
}

/// Project markers: a file or folder name and what it says.
const PROJECT_MARKERS: [(&str, &str); 16] = [
    (".git", "a Git repository"),
    ("package.json", "a JavaScript/Node project"),
    ("cargo.toml", "a Rust project"),
    ("pyproject.toml", "a Python project"),
    ("requirements.txt", "a Python project"),
    ("go.mod", "a Go project"),
    ("pom.xml", "a Java (Maven) project"),
    ("build.gradle", "a Java/Android (Gradle) project"),
    ("build.gradle.kts", "a Java/Android (Gradle) project"),
    ("cmakelists.txt", "a C/C++ (CMake) project"),
    ("composer.json", "a PHP project"),
    ("project.godot", "a Godot game project"),
    ("projectsettings", "a Unity project"),
    ("pubspec.yaml", "a Flutter/Dart project"),
    ("gemfile", "a Ruby project"),
    (".vscode", "a folder opened in VS Code"),
];

fn project_marker(ins: &Inspector, i: u32) -> Option<&'static str> {
    if !ins.is_dir(i) {
        return None;
    }
    for c in ins.children(i) {
        let n = ins.name(c).to_lowercase();
        if let Some((_, what)) = PROJECT_MARKERS.iter().find(|(m, _)| *m == n) {
            return Some(what);
        }
        if ins.is_file(c)
            && (n.ends_with(".sln") || n.ends_with(".csproj") || n.ends_with(".uproject"))
        {
            return Some(if n.ends_with(".uproject") {
                "an Unreal Engine project"
            } else {
                "a Visual Studio project"
            });
        }
    }
    None
}

fn deleting(
    ins: &Inspector,
    i: u32,
    owner: Option<&OwnerAnswer>,
    verdict: Option<&Verdict>,
    project: Option<&str>,
) -> String {
    let reason_text = verdict.zip(ins.mark(i).and_then(|(m, _)| if_deleted(&m.reason)));
    if let Some(v) = verdict {
        if v.source == "refused" {
            return format!("DiskCleaner refuses this: {}. Do not delete it.", v.detail);
        }
    }
    if let Some((v, text)) = reason_text {
        let prefix = if v.offered { "" } else { "Not suggested. " };
        return format!("{prefix}{text}");
    }
    if let Some(o) = owner {
        return match o.kind.as_str() {
            "windows" => "Part of Windows. Deleting it can stop Windows from working or starting. Never delete it.".to_owned(),
            _ if o.install_folder => format!(
                "Part of the installed app \"{}\". Deleting files here can break it (it may stop starting, or need a repair or reinstall). To remove the app, use {}.",
                o.name,
                if o.uninstaller { "its own uninstaller" } else { "Windows' Apps settings" }
            ),
            _ => format!(
                "Belongs to \"{}\" (its data or settings, {} confidence). Deleting it can reset or break that app: it may lose settings, saves or logins. It is not marked as safe to delete.",
                o.name, o.confidence
            ),
        };
    }
    if let Some(p) = project {
        return format!("Looks like {p}. If it is your work, deleting it loses that work unless it is backed up or pushed somewhere else.");
    }
    if in_user_folder(ins, i) {
        return "Your own files. Nothing on the system depends on them as far as the scan shows, but DiskCleaner cannot know whether you still need them: once deleted (after the holding period) they are gone.".to_owned();
    }
    "Unknown: no app claims it and no rule describes it. DiskCleaner treats unknown things as Don't touch; look inside first.".to_owned()
}

fn moving(ins: &Inspector, i: u32, owner: Option<&OwnerAnswer>, project: Option<&str>) -> String {
    if let Some(o) = owner {
        return match o.kind.as_str() {
            "windows" => "Part of Windows. Never move it.".to_owned(),
            _ if o.install_folder => format!(
                "Moving it breaks \"{}\": Windows, its shortcuts, its uninstaller and its updates expect it where it is. To move an app, uninstall it and install it again in the new place (some launchers, like Steam, can move games themselves).",
                o.name
            ),
            _ => format!(
                "\"{}\" looks for this here. After moving it, the app may act like a fresh install (settings, saves or logins missing) until it is moved back.",
                o.name
            ),
        };
    }
    let base = if in_user_folder(ins, i) || ins.parent(i).is_some_and(|p| ins.parent(p) == Some(0))
    {
        "Nothing on the system depends on it as far as the scan shows. Shortcuts, recent-files lists and programs that point at the old place will not find it any more."
    } else {
        "No app claims it, so moving it should not break an installed app; shortcuts or programs that point at the old place will not find it."
    };
    match project {
        Some(p) => format!("{base} It looks like {p}: editors and build tools remember paths, so reopen it from the new place."),
        None => base.to_owned(),
    }
}

/// Everything about one path, or `None` if it is not in the scan.
#[must_use]
pub fn explain(ins: &Inspector, path: &str) -> Option<Explanation> {
    let i = ins.find(path)?;
    let entry = ins.entry(i);
    let owner = owner_answer(ins, i);
    let verdict = verdict(ins, i);
    let project = project_marker(ins, i);
    let if_deleted = deleting(ins, i, owner.as_ref(), verdict.as_ref(), project);
    let if_moved = moving(ins, i, owner.as_ref(), project);
    let biggest_inside = if ins.is_dir(i) {
        ins.children(i)
            .into_iter()
            .take(8)
            .map(|c| ins.entry(c))
            .collect()
    } else {
        Vec::new()
    };
    let inside = ins.marks_inside(i);
    let findings_inside = (!inside.is_empty()).then(|| {
        (
            inside.len(),
            inside
                .iter()
                .filter(|(m, _)| m.offered)
                .map(|(m, _)| m.bytes)
                .sum(),
        )
    });
    Some(Explanation {
        place: place_kind(&entry.path),
        entry,
        owner,
        verdict,
        if_deleted,
        if_moved,
        biggest_inside,
        findings_inside,
        project: project.map(str::to_owned),
    })
}

/// Bytes in each kind of space.
#[derive(Debug, Clone, Default, Serialize)]
pub struct Buckets {
    /// Apps and games (anything an installed app owns).
    pub apps: u64,
    /// The person's own folders (Downloads, Documents, ...).
    pub your_files: u64,
    /// Windows.
    pub windows: u64,
    /// Everything else.
    pub other: u64,
}

/// Where the space goes.
#[must_use]
pub fn buckets(ins: &Inspector) -> Buckets {
    let mut b = Buckets::default();
    let mut mine = vec![false; ins.len()];
    for i in ins.all() {
        let p = ins.parent(i).unwrap_or(0) as usize;
        mine[i as usize] = mine[p] || user_folder_at(ins, i);
        if !ins.is_file(i) || ins.flags(i) & crate::view::flag::NOT_COUNTED != 0 {
            continue;
        }
        let bytes = ins.bytes(i);
        match ins.owner(i).map(|(o, _, _)| o.kind.as_str()) {
            Some("windows") => b.windows += bytes,
            Some(_) => b.apps += bytes,
            None if mine[i as usize] => b.your_files += bytes,
            None => b.other += bytes,
        }
    }
    b
}

/// A suggestion or a shown-only finding.
#[derive(Debug, Clone, Serialize)]
pub struct Cleanable {
    /// Where.
    pub path: String,
    /// Size on disk.
    pub bytes: u64,
    /// The verdict.
    #[serde(flatten)]
    pub verdict: Verdict,
    /// What deleting it would do.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub if_deleted: Option<&'static str>,
}

/// What the checks found, biggest first. Refusals are left out; shown-only findings only when asked.
#[must_use]
pub fn cleanable(ins: &Inspector, include_shown_only: bool) -> Vec<Cleanable> {
    let mut out: Vec<Cleanable> = ins
        .marks
        .iter()
        .filter(|m| m.source != "refused" && (m.offered || include_shown_only))
        .map(|m| Cleanable {
            path: ins.path(m.node),
            bytes: m.bytes,
            verdict: verdict_of(ins, m, None),
            if_deleted: if_deleted(&m.reason),
        })
        .collect();
    out.sort_by(|a, b| b.bytes.cmp(&a.bytes).then(a.path.cmp(&b.path)));
    out
}

/// An installed app and its space.
#[derive(Debug, Clone, Serialize)]
pub struct AppAnswer {
    /// Name.
    pub name: String,
    /// Publisher.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub publisher: Option<String>,
    /// Bytes in the scanned folders.
    pub bytes: u64,
    /// Size the installer stated (when nothing of it is in the scan).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stated_bytes: Option<u64>,
    /// Last started (YYYY-MM-DD); missing means not known, not "never".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_used: Option<String>,
    /// Days since it was last started.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub days_since_used: Option<i64>,
    /// Install date.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub installed: Option<String>,
    /// Has its own uninstaller.
    pub uninstaller: bool,
    /// Where it keeps things (biggest first): (path, bytes, kind of place).
    pub places: Vec<(String, u64, &'static str)>,
}

/// Every installed app, biggest first.
#[must_use]
pub fn apps(ins: &Inspector) -> Vec<AppAnswer> {
    let mut per: Vec<(u64, Vec<u32>)> = vec![(0, Vec::new()); ins.owners.len()];
    for i in ins.all() {
        let Some((o, true)) = ins.owner_id(i) else {
            continue;
        };
        let above = ins.parent(i).and_then(|p| ins.owner_id(p)).map(|(a, _)| a);
        if above == Some(o) {
            continue;
        }
        if let Some(slot) = per.get_mut(o as usize) {
            slot.0 += ins.bytes(i);
            slot.1.push(i);
        }
    }
    let mut out: Vec<AppAnswer> = ins
        .owners
        .iter()
        .zip(per)
        .filter(|(o, _)| o.kind == "app")
        .map(|(o, (bytes, mut places))| {
            places.sort_by_key(|p| std::cmp::Reverse(ins.bytes(*p)));
            AppAnswer {
                name: o.name.clone(),
                publisher: o.publisher.clone(),
                bytes,
                stated_bytes: (bytes == 0).then_some(o.stated_bytes).flatten(),
                last_used: o.last_used.and_then(date),
                days_since_used: o.last_used.map(|t| ((ins.created - t) / DAY).max(0)),
                installed: o.installed.clone(),
                uninstaller: o.uninstaller,
                places: places
                    .into_iter()
                    .take(20)
                    .map(|p| {
                        let path = ins.path(p);
                        let kind = place_kind(&path);
                        (path, ins.bytes(p), kind)
                    })
                    .collect(),
            }
        })
        .collect();
    out.sort_by(|a, b| {
        b.bytes
            .max(b.stated_bytes.unwrap_or(0))
            .cmp(&a.bytes.max(a.stated_bytes.unwrap_or(0)))
            .then(a.name.cmp(&b.name))
    });
    out
}

/// Entries whose name contains `query` (any case), biggest first.
#[must_use]
pub fn search(ins: &Inspector, query: &str, folders_only: bool, limit: usize) -> Vec<Entry> {
    let q = query.trim().to_lowercase();
    if q.is_empty() {
        return Vec::new();
    }
    let mut hits: Vec<u32> = ins
        .all()
        .filter(|&i| (!folders_only || ins.is_dir(i)) && ins.name(i).to_lowercase().contains(&q))
        .collect();
    hits.sort_by_key(|h| std::cmp::Reverse(ins.bytes(*h)));
    hits.into_iter().take(limit).map(|i| ins.entry(i)).collect()
}

/// A folder that looks like a software project.
#[derive(Debug, Clone, Serialize)]
pub struct Project {
    /// The folder.
    #[serde(flatten)]
    pub entry: Entry,
    /// What it looks like.
    pub kind: &'static str,
}

/// Software projects (outermost folders with a project marker), biggest first. Folders inside
/// app data, Program Files, Windows and package folders (node_modules) are left out.
#[must_use]
pub fn projects(ins: &Inspector, limit: usize) -> Vec<Project> {
    let mut found: Vec<(u32, &'static str)> = Vec::new();
    let skip = |path: &str| {
        let k = path.to_lowercase();
        [
            "\\appdata\\",
            "\\program files",
            "\\windows\\",
            "\\programdata\\",
            "node_modules",
            "\\$recycle.bin",
            "\\.cargo\\",
            "\\.rustup\\",
            "\\go\\pkg\\",
            "site-packages",
        ]
        .iter()
        .any(|s| k.contains(s))
    };
    let mut taken: Vec<u32> = Vec::new();
    for i in ins.all() {
        if !ins.is_dir(i) || taken.iter().any(|&t| ins.is_inside(i, t)) {
            continue;
        }
        if let Some(kind) = project_marker(ins, i) {
            let path = ins.path(i);
            if skip(&path) {
                continue;
            }
            taken.push(i);
            found.push((i, kind));
        }
    }
    found.sort_by_key(|f| std::cmp::Reverse(ins.bytes(f.0)));
    found
        .into_iter()
        .take(limit)
        .map(|(i, kind)| Project {
            entry: ins.entry(i),
            kind,
        })
        .collect()
}

/// The biggest files (inside `within`, if given).
#[must_use]
pub fn biggest_files(ins: &Inspector, within: Option<u32>, limit: usize) -> Vec<Entry> {
    let mut files: Vec<u32> = ins
        .all()
        .filter(|&i| ins.is_file(i) && within.is_none_or(|w| ins.is_inside(i, w)))
        .collect();
    files.sort_by_key(|f| std::cmp::Reverse(ins.bytes(*f)));
    files
        .into_iter()
        .take(limit)
        .map(|i| ins.entry(i))
        .collect()
}

/// The overview of a scan.
#[derive(Debug, Clone, Serialize)]
pub struct Overview {
    /// The scanned folders or drives.
    pub scanned: Vec<String>,
    /// When (YYYY-MM-DD).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scanned_on: Option<String>,
    /// Total size on disk.
    pub bytes: u64,
    /// Files.
    pub files: u64,
    /// Where the space goes.
    pub space: Buckets,
    /// The biggest folders one and two levels down (and in each profile).
    pub biggest_folders: Vec<Entry>,
    /// Suggestions: count and bytes.
    pub suggested: (usize, u64),
    /// Shown only (not suggested): count and bytes.
    pub shown_only: (usize, u64),
    /// Installed apps known.
    pub apps: usize,
}

/// The overview.
#[must_use]
pub fn overview(ins: &Inspector) -> Overview {
    let mut folders: Vec<u32> = Vec::new();
    for &r in ins.roots() {
        for c in ins.children(r) {
            if ins.is_dir(c) {
                folders.push(c);
                for g in ins.children(c).into_iter().take(6) {
                    if ins.is_dir(g) {
                        folders.push(g);
                    }
                }
            }
        }
    }
    folders.sort_by_key(|f| std::cmp::Reverse(ins.bytes(*f)));
    folders.dedup();
    let (mut sug, mut shown) = ((0usize, 0u64), (0usize, 0u64));
    for m in ins.marks.iter().filter(|m| m.source != "refused") {
        let t = if m.offered { &mut sug } else { &mut shown };
        t.0 += 1;
        t.1 += m.bytes;
    }
    Overview {
        scanned: ins.roots().iter().map(|&r| ins.path(r)).collect(),
        scanned_on: date(ins.created),
        bytes: ins.bytes(0),
        files: ins.entry(0).files,
        space: buckets(ins),
        biggest_folders: folders.into_iter().take(15).map(|i| ins.entry(i)).collect(),
        suggested: sug,
        shown_only: shown,
        apps: ins.owners.iter().filter(|o| o.kind == "app").count(),
    }
}

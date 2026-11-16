//! Starting an installed app's **own** uninstaller (D133): the command Windows records for it
//! (`UninstallString`), the same one Settings → Apps runs. Rhizome never removes an app's program
//! files itself, never runs anything silently, and never runs anything but that program.
//!
//! The recorded command is untrusted text, so it is never handed to a shell (`cmd.exe` would read
//! `&`, `|` and `%VAR%` in it). It is split into one program and its arguments here, and refused
//! (fail closed) unless the program is an existing `.exe` given by a full path and is not a shell
//! or script host. A refused command is not run; the person is sent to Windows' own Installed
//! apps page instead.

use std::path::{Path, PathBuf};

/// One program and its arguments, ready to start.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Command {
    /// Full path of the program.
    pub program: PathBuf,
    /// The arguments exactly as recorded (passed on as one string; Windows programs split them
    /// themselves).
    pub args: String,
}

/// Programs that run other text as commands: never started, whatever the record says.
const NEVER: [&str; 12] = [
    "cmd.exe",
    "powershell.exe",
    "pwsh.exe",
    "wscript.exe",
    "cscript.exe",
    "mshta.exe",
    "rundll32.exe",
    "regsvr32.exe",
    "conhost.exe",
    "explorer.exe",
    "bash.exe",
    "wsl.exe",
];

/// Split a recorded uninstall command into a program and its arguments. `exists` says whether a
/// path is an existing file; `system_dir` is Windows' System32 folder (for `MsiExec.exe`).
///
/// # Errors
/// Why the command cannot be started safely (it is then not started at all).
pub fn parse(
    recorded: &str,
    exists: &dyn Fn(&Path) -> bool,
    system_dir: &Path,
) -> Result<Command, String> {
    let text = recorded.trim();
    if text.is_empty() {
        return Err("no uninstall command is recorded".into());
    }
    if text.contains(['\n', '\r', '\0']) {
        return Err("the recorded uninstall command is damaged".into());
    }

    let (program, args) = if let Some(rest) = text.strip_prefix('"') {
        // "C:\path\uninstall.exe" /args
        let end = rest
            .find('"')
            .ok_or("the recorded uninstall command is damaged (an unclosed quote)")?;
        (rest[..end].to_owned(), rest[end + 1..].trim().to_owned())
    } else if let Some(args) = msiexec_args(text) {
        // MsiExec.exe /X{GUID}: Windows Installer, from System32 only.
        (
            format!(
                "{}\\msiexec.exe",
                system_dir.to_string_lossy().trim_end_matches(['\\', '/'])
            ),
            args,
        )
    } else {
        // C:\Program Files\App\uninstall.exe /args (no quotes): the shortest ".exe" prefix that
        // exists, the way Windows itself reads such a line.
        let lower = text.to_ascii_lowercase();
        let mut found = None;
        let mut from = 0;
        while let Some(at) = lower[from..].find(".exe") {
            let end = from + at + 4;
            let after = text[end..].chars().next();
            if after.is_none_or(char::is_whitespace) && exists(Path::new(&text[..end])) {
                found = Some((text[..end].to_owned(), text[end..].trim().to_owned()));
                break;
            }
            from = end;
        }
        found.ok_or("the program in the recorded uninstall command cannot be found")?
    };

    let path = PathBuf::from(&program);
    // The file name, split on either slash (the same on every system, so tests mean something).
    let name = program
        .rsplit(['\\', '/'])
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase();
    if !is_full_path(&program) {
        return Err("the recorded uninstall command does not name its program by full path".into());
    }
    if !name.ends_with(".exe") {
        return Err("the recorded uninstall command is not a program (.exe)".into());
    }
    if NEVER.contains(&name.as_str()) {
        return Err(format!(
            "the recorded uninstall command runs {name}, which Rhizome never starts"
        ));
    }
    if !exists(&path) {
        return Err("the uninstaller program is not there any more".into());
    }
    Ok(Command {
        program: path,
        args,
    })
}

/// `C:\...` or `\\server\share\...`.
fn is_full_path(p: &str) -> bool {
    let b = p.as_bytes();
    (b.len() > 3 && b[0].is_ascii_alphabetic() && b[1] == b':' && (b[2] == b'\\' || b[2] == b'/'))
        || p.starts_with(r"\\")
}

/// The arguments of an `MsiExec.exe` line (`/I{GUID}` becomes `/X{GUID}`: `/I` opens the
/// installer's change screen, `/X` removes), or `None` when the line is not one.
fn msiexec_args(text: &str) -> Option<String> {
    let (first, rest) = text.split_once(char::is_whitespace).unwrap_or((text, ""));
    let f = first.to_ascii_lowercase();
    if f != "msiexec.exe" && f != "msiexec" {
        return None;
    }
    let rest = rest.trim();
    let mut out = String::with_capacity(rest.len());
    for (k, part) in rest.split_whitespace().enumerate() {
        if k > 0 {
            out.push(' ');
        }
        let lower = part.to_ascii_lowercase();
        if (lower.starts_with("/i{") || lower.starts_with("-i{")) && part.ends_with('}') {
            out.push_str("/X");
            out.push_str(&part[2..]);
        } else {
            out.push_str(part);
        }
    }
    Some(out)
}

/// Start the app's own uninstaller with its own window (Windows may ask for permission first).
/// Returns as soon as it started; the uninstaller runs on its own.
///
/// # Errors
/// When Windows could not start it (or on other systems).
#[cfg(feature = "destructive-actions")]
pub fn start(cmd: &Command) -> std::io::Result<()> {
    crate::ops::shell_open(&cmd.program, Some(&cmd.args))
}

/// Open Windows' own Installed apps page (when the recorded command cannot be started safely).
///
/// # Errors
/// When Windows could not open it (or on other systems).
#[cfg(feature = "destructive-actions")]
pub fn open_installed_apps() -> std::io::Result<()> {
    crate::ops::shell_open(Path::new("ms-settings:appsfeatures"), None)
}

/// Windows' System32 folder.
#[must_use]
pub fn system_dir() -> PathBuf {
    let root =
        std::env::var_os("SystemRoot").map_or_else(|| PathBuf::from(r"C:\Windows"), PathBuf::from);
    root.join("System32")
}

#[cfg(test)]
mod tests {
    use super::*;

    const SYS: &str = r"C:\Windows\System32";

    fn there(list: &'static [&'static str]) -> impl Fn(&Path) -> bool {
        move |p: &Path| list.iter().any(|x| Path::new(x) == p)
    }

    #[test]
    fn quoted_programs_keep_their_arguments() {
        let ex = there(&[r"C:\Riot Games\Riot Client\RiotClientServices.exe"]);
        let c = parse(
            r#""C:\Riot Games\Riot Client\RiotClientServices.exe" --uninstall-product=valorant --uninstall-patchline=live"#,
            &ex,
            Path::new(SYS),
        )
        .unwrap();
        assert_eq!(
            c.program,
            PathBuf::from(r"C:\Riot Games\Riot Client\RiotClientServices.exe")
        );
        assert_eq!(
            c.args,
            "--uninstall-product=valorant --uninstall-patchline=live"
        );
    }

    #[test]
    fn unquoted_paths_with_spaces_are_read_like_windows_does() {
        let ex = there(&[r"C:\Program Files\Some App\uninst.exe"]);
        let c = parse(
            r"C:\Program Files\Some App\uninst.exe /S_not_added",
            &ex,
            Path::new(SYS),
        )
        .unwrap();
        assert_eq!(
            c.program,
            PathBuf::from(r"C:\Program Files\Some App\uninst.exe")
        );
        assert_eq!(c.args, "/S_not_added");
        let c = parse(r"C:\Program Files\Some App\uninst.exe", &ex, Path::new(SYS)).unwrap();
        assert_eq!(c.args, "");
    }

    #[test]
    fn windows_installer_lines_remove_instead_of_change() {
        let ex = there(&[r"C:\Windows\System32\msiexec.exe"]);
        let c = parse(
            "MsiExec.exe /I{12345678-1234-1234-1234-123456789012}",
            &ex,
            Path::new(SYS),
        )
        .unwrap();
        assert_eq!(c.program, PathBuf::from(r"C:\Windows\System32\msiexec.exe"));
        assert_eq!(c.args, "/X{12345678-1234-1234-1234-123456789012}");
        let c = parse("MsiExec.exe /X{ABC}", &ex, Path::new(SYS)).unwrap();
        assert_eq!(c.args, "/X{ABC}");
    }

    #[test]
    fn shells_scripts_and_odd_commands_are_never_started() {
        let ex = |_: &Path| true;
        for bad in [
            r#""C:\Windows\System32\cmd.exe" /c del C:\x"#,
            r"C:\Windows\System32\cmd.exe /c rd /s /q C:\",
            r#""C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe" -c x"#,
            r"RunDll32 C:\PROGRA~1\x.dll,Uninstall",
            r#""C:\x\uninstall.bat""#,
            r#""uninstall.exe" /x"#,
            r#""C:\x\un.exe /x"#,
            "",
            "   ",
            "C:\\x\\un.exe\n/x",
        ] {
            assert!(
                parse(bad, &ex, Path::new(SYS)).is_err(),
                "{bad:?} was accepted"
            );
        }
        // A program that is not there.
        let none = |_: &Path| false;
        assert!(parse(r#""C:\gone\un.exe""#, &none, Path::new(SYS)).is_err());
        assert!(parse(r"C:\gone\un.exe /x", &none, Path::new(SYS)).is_err());
    }

    #[test]
    fn an_unquoted_line_never_runs_a_shorter_program_than_written() {
        // Only places where ".exe" is written are cut at (never "C:\Program" + ".exe", the
        // classic unquoted-path trick), and the first one that exists wins.
        let ex = there(&[r"C:\Program Files\A\un.exe"]);
        let c = parse(r"C:\Program Files\A\un.exe arg.exe", &ex, Path::new(SYS)).unwrap();
        assert_eq!(c.program, PathBuf::from(r"C:\Program Files\A\un.exe"));
        assert_eq!(c.args, "arg.exe");
    }
}

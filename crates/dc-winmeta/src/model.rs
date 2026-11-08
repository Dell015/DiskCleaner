//! Data types for installed apps and known folders. Everything here compiles and serializes
//! on every platform; only the code that fills them in is Windows-specific.

use serde::{Deserialize, Serialize};

/// Version of the [`AppSnapshot`] JSON format.
pub const SNAPSHOT_SCHEMA_VERSION: u32 = 1;

/// Which registry location an uninstall entry came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RegistrySource {
    /// Machine-wide, 64-bit view.
    Machine64,
    /// Machine-wide, 32-bit view (`WOW6432Node`).
    Machine32,
    /// The current user only.
    User,
}

/// How the install folder of an app was worked out. A guess is never shown as a fact.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FolderOrigin {
    /// The `InstallLocation` value.
    InstallLocation,
    /// The folder of the file named by `DisplayIcon`.
    DisplayIcon,
    /// The folder of the program named in `UninstallString`.
    UninstallString,
    /// The folder of the target of a Start Menu or Desktop shortcut with the app's name.
    Shortcut,
}

/// One uninstall entry exactly as read from the registry (untrusted text).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RawUninstallEntry {
    /// Name of the registry key.
    pub key: String,
    /// Where it was found.
    pub source: Option<RegistrySource>,
    /// `DisplayName`.
    pub display_name: Option<String>,
    /// `Publisher`.
    pub publisher: Option<String>,
    /// `DisplayVersion`.
    pub display_version: Option<String>,
    /// `InstallLocation`.
    pub install_location: Option<String>,
    /// `DisplayIcon`.
    pub display_icon: Option<String>,
    /// `UninstallString`. Kept as text only; it is never executed.
    pub uninstall_string: Option<String>,
    /// `EstimatedSize`, in KiB.
    pub estimated_size_kb: Option<u32>,
    /// `InstallDate`, as written (usually `YYYYMMDD`).
    pub install_date: Option<String>,
    /// `SystemComponent` is 1.
    pub system_component: bool,
    /// `WindowsInstaller` is 1.
    pub windows_installer: bool,
    /// `ParentKeyName`: set on updates and patches.
    pub parent_key_name: Option<String>,
    /// `ReleaseType`, e.g. `Update` or `Security Update`.
    pub release_type: Option<String>,
}

/// Properties of an installed app that later phases may rely on.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppFlags {
    /// Marked as a system component by the installer.
    pub system_component: bool,
    /// An update, hotfix or patch rather than an app.
    pub update_or_patch: bool,
    /// Installed through Windows Installer (MSI).
    pub windows_installer: bool,
    /// The entry has no display name (usually a component).
    pub hidden_entry: bool,
    /// An install folder is known but no longer exists.
    pub stale_registration: bool,
    /// No usable install folder could be worked out.
    pub no_install_folder: bool,
}

/// A Start Menu or Desktop shortcut (`.lnk`), as far as it could be read.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Shortcut {
    /// Full path of the `.lnk` file.
    pub lnk: String,
    /// File name without `.lnk`; this is what the user sees.
    pub name: String,
    /// What it points to (absolute path), when the shortcut stores one.
    pub target: Option<String>,
    /// Command-line arguments, as text only.
    pub arguments: Option<String>,
    /// Working directory.
    pub working_dir: Option<String>,
}

/// What a program file says about itself (its version resource). All parts are optional and
/// are plain text written by the program's author, so they can be wrong or faked.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct VersionInfo {
    /// `CompanyName`.
    pub company: Option<String>,
    /// `ProductName`.
    pub product: Option<String>,
    /// `FileDescription`.
    pub description: Option<String>,
    /// `FileVersion`.
    pub file_version: Option<String>,
}

/// An installed classic program.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstalledApp {
    /// Registry key name.
    pub key: String,
    /// Registry location.
    pub source: RegistrySource,
    /// Display name (empty for hidden entries).
    pub name: String,
    /// Publisher.
    pub publisher: Option<String>,
    /// Version text.
    pub version: Option<String>,
    /// Install folder, when one could be worked out.
    pub install_folder: Option<String>,
    /// Where the install folder came from.
    pub folder_origin: Option<FolderOrigin>,
    /// `DisplayIcon` as written.
    pub display_icon: Option<String>,
    /// `UninstallString` as written (text only).
    pub uninstall_command: Option<String>,
    /// Estimated size in KiB, as stated by the installer.
    pub estimated_size_kb: Option<u32>,
    /// Install date as written.
    pub install_date: Option<String>,
    /// Properties.
    pub flags: AppFlags,
    /// The app's main program, when one could be identified inside its install folder.
    #[serde(default)]
    pub main_exe: Option<String>,
    /// Version resource of `main_exe`.
    #[serde(default)]
    pub version_info: Option<VersionInfo>,
    /// Shortcuts (paths of `.lnk` files) that point into the install folder.
    #[serde(default)]
    pub shortcuts: Vec<String>,
}

/// A Windows known folder.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KnownFolder {
    /// Stable name such as `LocalAppData`.
    pub name: String,
    /// Absolute path, or `None` when it could not be resolved.
    pub path: Option<String>,
}

/// Something that could not be read; the run carried on.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Problem {
    /// Which source failed (for example `registry:machine64`).
    pub source: String,
    /// What went wrong.
    pub message: String,
}

/// The saved result of reading the system: known folders and installed apps.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppSnapshot {
    /// Format version.
    pub schema_version: u32,
    /// Which program wrote it.
    pub created_by: String,
    /// Resolved known folders.
    pub known_folders: Vec<KnownFolder>,
    /// Installed apps, sorted by name.
    pub apps: Vec<InstalledApp>,
    /// Non-fatal problems met while reading.
    pub problems: Vec<Problem>,
}

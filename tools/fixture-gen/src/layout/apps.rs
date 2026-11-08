//! Application-like categories: fake game launchers, browsers, caches, leftovers.

use crate::error::GenError;
use crate::writer::Ctx;

const KIB: u64 = 1024;
const MIB: u64 = 1024 * 1024;

pub(crate) fn steam(ctx: &mut Ctx) -> Result<(), GenError> {
    ctx.begin_category("steam");
    let base = ["Program Files (x86)", "Steam"];
    ctx.exe(&base, "steam.exe", 200 * KIB, 400 * KIB)?;
    ctx.text(&base, "libraryfolders.vdf", "\"libraryfolders\" {}\n")?;
    for game in ["Fake Game A", "Fake Game B"] {
        let dir = ["Program Files (x86)", "Steam", "steamapps", "common", game];
        ctx.exe(&dir, "game.exe", 300 * KIB, 600 * KIB)?;
        ctx.bin(&dir, "assets.pak", MIB, 3 * MIB)?;
        let data = [dir[0], dir[1], dir[2], dir[3], dir[4], "data"];
        for i in 0..3 {
            ctx.bin(&data, &format!("chunk{i}.bin"), 64 * KIB, 512 * KIB)?;
        }
    }
    let shader = [
        "Program Files (x86)",
        "Steam",
        "steamapps",
        "shadercache",
        "0001",
    ];
    ctx.bin(&shader, "cache.bin", 128 * KIB, 256 * KIB)?;
    let dl = ["Program Files (x86)", "Steam", "steamapps", "downloading"];
    ctx.bin(&dl, "partial.tmp", 64 * KIB, 128 * KIB)
}

fn chromium(ctx: &mut Ctx, name: &str, vendor: &[&str]) -> Result<(), GenError> {
    ctx.begin_category(name);
    let mut profile: Vec<&str> = vendor.to_vec();
    profile.push("User Data");
    profile.push("Default");
    for sub in ["Cache", "Code Cache", "GPUCache"] {
        let mut dir = profile.clone();
        dir.push(sub);
        for i in 0..4 {
            ctx.bin(&dir, &format!("f_{i:06x}"), 4 * KIB, 128 * KIB)?;
        }
    }
    ctx.text(&profile, "Preferences", "{}\n")?;
    ctx.bin(&profile, "History", 32 * KIB, 96 * KIB)?;
    ctx.bin(&profile, "Cookies", 16 * KIB, 64 * KIB)
}

pub(crate) fn chrome(ctx: &mut Ctx) -> Result<(), GenError> {
    chromium(
        ctx,
        "chrome",
        &["Users", "TestUser", "AppData", "Local", "Google", "Chrome"],
    )
}

pub(crate) fn brave(ctx: &mut Ctx) -> Result<(), GenError> {
    chromium(
        ctx,
        "brave",
        &[
            "Users",
            "TestUser",
            "AppData",
            "Local",
            "BraveSoftware",
            "Brave-Browser",
        ],
    )
}

pub(crate) fn edge_browser(ctx: &mut Ctx) -> Result<(), GenError> {
    chromium(
        ctx,
        "edge",
        &["Users", "TestUser", "AppData", "Local", "Microsoft", "Edge"],
    )
}

pub(crate) fn hoyoverse(ctx: &mut Ctx) -> Result<(), GenError> {
    ctx.begin_category("hoyoverse");
    let base = ["Program Files", "HoYoverse", "Fake Launcher"];
    ctx.exe(&base, "launcher.exe", 200 * KIB, 400 * KIB)?;
    let game = ["Program Files", "HoYoverse", "Fake Game", "Game"];
    ctx.exe(&game, "FakeGame.exe", 400 * KIB, 800 * KIB)?;
    ctx.bin(&game, "data.pak", 2 * MIB, 4 * MIB)?;
    let logs = [
        "Users",
        "TestUser",
        "AppData",
        "LocalLow",
        "HoYoverse",
        "Fake Game",
    ];
    ctx.text(&logs, "output_log.txt", "log line\n")
}

pub(crate) fn appdata(ctx: &mut Ctx) -> Result<(), GenError> {
    ctx.begin_category("appdata");
    for vendor in ["Contoso", "Fabrikam", "Northwind"] {
        for root in ["Local", "Roaming"] {
            let dir = ["Users", "TestUser", "AppData", root, vendor];
            ctx.text(&dir, "settings.json", "{}\n")?;
            ctx.bin(&dir, "state.db", 8 * KIB, 64 * KIB)?;
        }
    }
    let pd = ["ProgramData", "Contoso"];
    ctx.bin(&pd, "shared.dat", 4 * KIB, 32 * KIB)
}

pub(crate) fn temp(ctx: &mut Ctx) -> Result<(), GenError> {
    ctx.begin_category("temp");
    let dir = ["Users", "TestUser", "AppData", "Local", "Temp"];
    for i in 0..8 {
        let id = ctx.hex(8);
        ctx.bin(&dir, &format!("tmp{id}{i}.tmp"), KIB, 256 * KIB)?;
    }
    ctx.bin(&["Windows", "Temp"], "setup.log.tmp", 4 * KIB, 16 * KIB)
}

pub(crate) fn leftover(ctx: &mut Ctx) -> Result<(), GenError> {
    ctx.begin_category("leftover");
    let local = ["Users", "TestUser", "AppData", "Local", "UninstalledApp"];
    ctx.bin(&local, "cache.bin", 64 * KIB, MIB)?;
    ctx.text(&local, "config.ini", "[app]\nname=UninstalledApp\n")?;
    let roaming = ["Users", "TestUser", "AppData", "Roaming", "UninstalledApp"];
    ctx.bin(&roaming, "profile.dat", 16 * KIB, 128 * KIB)?;
    let pd = ["ProgramData", "UninstalledApp"];
    ctx.bin(&pd, "license.dat", KIB, 4 * KIB)
}

pub(crate) fn system32(ctx: &mut Ctx) -> Result<(), GenError> {
    ctx.begin_category("system32");
    let dir = ["Windows", "System32"];
    for name in ["kernel32.dll", "ntdll.dll", "user32.dll"] {
        ctx.exe(&dir, name, 64 * KIB, 256 * KIB)?;
    }
    ctx.exe(
        &["Windows", "System32", "drivers"],
        "fake.sys",
        16 * KIB,
        64 * KIB,
    )?;
    ctx.bin(
        &["Windows", "WinSxS"],
        "component.manifest.bin",
        8 * KIB,
        32 * KIB,
    )
}

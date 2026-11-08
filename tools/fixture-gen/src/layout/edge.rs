//! Edge cases: deep nesting, huge file counts, zero-byte files, odd names, long names.

use crate::error::GenError;
use crate::writer::Ctx;

pub(crate) fn deep(ctx: &mut Ctx) -> Result<(), GenError> {
    ctx.begin_category("edge_deep");
    let names: Vec<String> = (0..64).map(|i| format!("d{i:02}")).collect();
    let mut comps: Vec<&str> = vec!["edge_deep"];
    comps.extend(names.iter().map(String::as_str));
    ctx.text(&comps, "bottom.txt", "bottom of a 64-level tree\n")
}

pub(crate) fn many_files(ctx: &mut Ctx, file_count: u32) -> Result<(), GenError> {
    ctx.begin_summarized_category("edge_many_files", "edge_many_files");
    ctx.dirs(&["edge_many_files"])?;
    for i in 0..file_count {
        ctx.bin(&["edge_many_files"], &format!("f{i:06}.dat"), 0, 513)?;
    }
    Ok(())
}

pub(crate) fn zero(ctx: &mut Ctx) -> Result<(), GenError> {
    ctx.begin_category("edge_zero");
    for i in 0..10 {
        ctx.empty(&["edge_zero"], &format!("zero{i}.dat"))?;
    }
    ctx.empty(&["edge_zero", "only_zero"], "zero.dat")?;
    ctx.dirs(&["edge_zero", "empty_dir"])
}

pub(crate) fn unicode(ctx: &mut Ctx) -> Result<(), GenError> {
    ctx.begin_category("edge_unicode");
    let dir = ["edge_unicode"];
    for name in [
        "日本語のファイル.txt",
        "中文文件.txt",
        "Кириллица.txt",
        "ملف_عربي.txt",
        "📁emoji🎮.txt",
        "e\u{301}combining.txt",
        "name with spaces.txt",
        ".leading_dot.txt",
    ] {
        ctx.text(&dir, name, "unicode name\n")?;
    }
    ctx.text(
        &["edge_unicode", "フォルダ 📂"],
        "inside.txt",
        "unicode dir\n",
    )
}

pub(crate) fn long(ctx: &mut Ctx) -> Result<(), GenError> {
    ctx.begin_category("edge_long");
    let parts: Vec<String> = (0..5).map(|i| format!("{}{i}", "L".repeat(59))).collect();
    let mut comps: Vec<&str> = vec!["edge_long"];
    comps.extend(parts.iter().map(String::as_str));
    let name = format!("{}.txt", "n".repeat(251));
    ctx.text(&comps, &name, "long path\n")
}

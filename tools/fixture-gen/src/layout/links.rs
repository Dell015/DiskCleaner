//! Links and sparse files. Each is recorded as created or skipped in the manifest.

use crate::error::GenError;
use crate::writer::Ctx;

pub(crate) fn build(ctx: &mut Ctx) -> Result<(), GenError> {
    ctx.begin_category("links");
    ctx.text(
        &["links", "junction_target"],
        "inside.txt",
        "junction target\n",
    )?;
    ctx.junction(&["links", "junction_link"], &["links", "junction_target"])?;

    ctx.text(&["links"], "symlink_target.txt", "symlink target\n")?;
    ctx.symlink_file(&["links", "symlink_file"], &["links", "symlink_target.txt"])?;
    ctx.dirs(&["links", "symlink_dir_target"])?;
    ctx.symlink_dir(&["links", "symlink_dir"], &["links", "symlink_dir_target"])?;

    ctx.bin(&["links"], "hardlink_a.bin", 8 * 1024, 16 * 1024)?;
    ctx.hardlink(&["links", "hardlink_a.bin"], &["links", "hardlink_b.bin"])?;

    ctx.sparse(
        &["links"],
        "sparse_64MiB.bin",
        64 * 1024 * 1024,
        32 * 1024 * 1024,
        4096,
    )
}

//! The fixture layout: which directories and files exist, grouped by category.
//!
//! Every category has its own random stream, so editing one never shifts another.

use crate::error::GenError;
use crate::writer::Ctx;

mod apps;
mod edge;
mod links;

/// Build the whole tree.
pub(crate) fn build(ctx: &mut Ctx, file_count: u32) -> Result<(), GenError> {
    apps::steam(ctx)?;
    apps::chrome(ctx)?;
    apps::brave(ctx)?;
    apps::hoyoverse(ctx)?;
    apps::appdata(ctx)?;
    apps::temp(ctx)?;
    apps::leftover(ctx)?;
    apps::system32(ctx)?;
    apps::edge_browser(ctx)?;
    edge::deep(ctx)?;
    edge::many_files(ctx, file_count)?;
    edge::zero(ctx)?;
    edge::unicode(ctx)?;
    edge::long(ctx)?;
    links::build(ctx)
}

//! `dc top`.

use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use dc_core::{query, NodeId, ScanResult, Tree};

use crate::size::format_bytes;

pub enum Report {
    Files,
    Dirs,
    Ext,
    Category,
    Oldest,
}

pub fn run(file: &Path, report: &Report, limit: usize) -> Result<()> {
    let reader = BufReader::new(
        File::open(file).with_context(|| format!("cannot open {}", file.display()))?,
    );
    let scan: ScanResult = serde_json::from_reader(reader).context("not a valid scan file")?;
    let tree = scan.tree().context("the scan file is damaged")?;
    let root = PathBuf::from(&scan.root);

    match report {
        Report::Files => print_nodes(&tree, &root, &query::top_files(&tree, limit), false),
        Report::Dirs => print_nodes(&tree, &root, &query::top_dirs(&tree, limit), true),
        Report::Oldest => print_nodes(&tree, &root, &query::oldest_files(&tree, limit), false),
        Report::Ext => print_stats(&query::by_extension(&tree), limit, "extension"),
        Report::Category => print_stats(&query::by_category(&tree), limit, "category"),
    }
    if scan.options.max_depth.is_some() || scan.options.min_size.is_some() {
        println!("(this scan stored only part of the tree; totals above include everything)");
    }
    Ok(())
}

fn print_nodes(tree: &Tree, root: &Path, ids: &[NodeId], dirs: bool) {
    println!("{:>12}  {:>12}  path", "size", "on disk");
    for &id in ids {
        let Some(node) = tree.get(id) else { continue };
        let (logical, disk) = if dirs {
            (node.agg.total_logical, node.agg.total_on_disk)
        } else {
            (node.logical_size, node.size_on_disk)
        };
        println!(
            "{:>12}  {:>12}  {}",
            format_bytes(logical),
            format_bytes(disk),
            root.join(tree.relative_path(id)).display()
        );
    }
}

fn print_stats(stats: &[query::TypeStat], limit: usize, label: &str) {
    println!("{:>12}  {:>12}  {:>8}  {label}", "size", "on disk", "files");
    for stat in stats.iter().take(limit) {
        let name = if stat.name.is_empty() {
            "(none)"
        } else {
            &stat.name
        };
        println!(
            "{:>12}  {:>12}  {:>8}  {name}",
            format_bytes(stat.logical),
            format_bytes(stat.on_disk),
            stat.files
        );
    }
}

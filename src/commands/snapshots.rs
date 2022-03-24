use anyhow::Result;
use bytesize::ByteSize;
use clap::Parser;
use prettytable::{cell, format, row, Table};

use crate::backend::DecryptReadBackend;
use crate::repo::SnapshotFile;

#[derive(Parser)]
pub(super) struct Opts {}

pub(super) async fn execute(be: &impl DecryptReadBackend, _opts: Opts) -> Result<()> {
    let mut snapshots = SnapshotFile::all_from_backend(be).await?;
    snapshots.sort();

    let mut table: Table = snapshots
        .into_iter()
        .map(|sn| {
            let paths = sn.paths.into_iter().map(|p| p + "\n").collect::<String>();
            let time = sn.time.format("%Y-%m-%d %H:%M:%S");
            let nodes = sn
                .node_count
                .map(|c| c.to_string())
                .unwrap_or_else(|| "?".to_string());
            let size = sn
                .size
                .map(|b| ByteSize(b).to_string_as(true))
                .unwrap_or_else(|| "?".to_string());
            row![sn.id, time, sn.hostname, "", paths, r->nodes, r->size]
        })
        .collect();
    table.set_titles(
        row![b->"ID", b->"Time", b->"Host", b->"Tags", b->"Paths", br->"Nodes", br->"Size"],
    );
    table.set_format(*format::consts::FORMAT_NO_BORDER_LINE_SEPARATOR);
    table.printstd();

    Ok(())
}

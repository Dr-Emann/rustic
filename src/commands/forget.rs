//! `forget` subcommand

/// App-local prelude includes `app_reader()`/`app_writer()`/`app_config()`
/// accessors along with logging macros. Customize as you see fit.
use crate::{
    commands::open_repository, helpers::table_with_titles, status_err, Application, RusticConfig,
    RUSTIC_APP,
};

use abscissa_core::{config::Override, Shutdown};
use abscissa_core::{Command, FrameworkError, Runnable};
use anyhow::Result;

use merge::Merge;
use serde::Deserialize;
use serde_with::{serde_as, DisplayFromStr};

use crate::{commands::prune::PruneCmd, filtering::SnapshotFilter};

use rustic_core::{
    ForgetGroup, ForgetGroups, ForgetSnapshot, KeepOptions, SnapshotGroup, SnapshotGroupCriterion,
};

/// `forget` subcommand
#[derive(clap::Parser, Command, Debug)]
pub(super) struct ForgetCmd {
    /// Snapshots to forget. If none is given, use filter options to filter from all snapshots
    #[clap(value_name = "ID")]
    ids: Vec<String>,

    #[clap(flatten)]
    config: ForgetOptions,

    #[clap(
        flatten,
        next_help_heading = "PRUNE OPTIONS (only when used with --prune)"
    )]
    prune_opts: PruneCmd,
}

impl Override<RusticConfig> for ForgetCmd {
    // Process the given command line options, overriding settings from
    // a configuration file using explicit flags taken from command-line
    // arguments.
    fn override_config(&self, mut config: RusticConfig) -> Result<RusticConfig, FrameworkError> {
        let mut self_config = self.config.clone();
        // merge "forget" section from config file, if given
        self_config.merge(config.forget);
        // merge "snapshot-filter" section from config file, if given
        self_config.filter.merge(config.snapshot_filter.clone());
        config.forget = self_config;
        Ok(config)
    }
}

#[serde_as]
#[derive(Clone, Default, Debug, clap::Parser, Deserialize, Merge)]
#[serde(default, rename_all = "kebab-case")]
pub struct ForgetOptions {
    /// Group snapshots by any combination of host,label,paths,tags (default: "host,label,paths")
    #[clap(long, short = 'g', value_name = "CRITERION")]
    #[serde_as(as = "Option<DisplayFromStr>")]
    group_by: Option<SnapshotGroupCriterion>,

    /// Also prune the repository
    #[clap(long)]
    #[merge(strategy = merge::bool::overwrite_false)]
    prune: bool,

    #[clap(flatten, next_help_heading = "Snapshot filter options")]
    #[serde(flatten)]
    filter: SnapshotFilter,

    #[clap(flatten, next_help_heading = "Retention options")]
    #[serde(flatten)]
    keep: KeepOptions,
}

impl Runnable for ForgetCmd {
    fn run(&self) {
        if let Err(err) = self.inner_run() {
            status_err!("{}", err);
            RUSTIC_APP.shutdown(Shutdown::Crash);
        };
    }
}

impl ForgetCmd {
    fn inner_run(&self) -> Result<()> {
        let config = RUSTIC_APP.config();
        let repo = open_repository(&config)?;

        let group_by = config.forget.group_by.unwrap_or_default();

        let groups = if self.ids.is_empty() {
            repo.get_forget_snapshots(&config.forget.keep, group_by, |sn| {
                config.forget.filter.matches(sn)
            })?
        } else {
            let item = ForgetGroup {
                group: SnapshotGroup::default(),
                snapshots: repo
                    .get_snapshots(&self.ids)?
                    .into_iter()
                    .map(|sn| ForgetSnapshot {
                        snapshot: sn,
                        keep: false,
                        reasons: vec!["id argument".to_string()],
                    })
                    .collect(),
            };
            ForgetGroups(vec![item])
        };

        for ForgetGroup { group, snapshots } in &groups.0 {
            if !group.is_empty() {
                println!("snapshots for {group}");
            }
            let mut table = table_with_titles([
                "ID", "Time", "Host", "Label", "Tags", "Paths", "Action", "Reason",
            ]);

            for ForgetSnapshot {
                snapshot: sn,
                keep,
                reasons,
            } in snapshots
            {
                let time = sn.time.format("%Y-%m-%d %H:%M:%S").to_string();
                let tags = sn.tags.formatln();
                let paths = sn.paths.formatln();
                let action = if *keep { "keep" } else { "remove" };
                let reason = reasons.join("\n");
                _ = table.add_row([
                    &sn.id.to_string(),
                    &time,
                    &sn.hostname,
                    &sn.label,
                    &tags,
                    &paths,
                    action,
                    &reason,
                ]);
            }

            println!();
            println!("{table}");
            println!();
        }

        let forget_snaps = groups.into_forget_ids();

        match (forget_snaps.is_empty(), config.global.dry_run) {
            (true, _) => println!("nothing to remove"),
            (false, true) => {
                println!("would have removed the following snapshots:\n {forget_snaps:?}");
            }
            (false, false) => {
                repo.delete_snapshots(&forget_snaps)?;
            }
        }

        if self.config.prune {
            let mut prune_opts = self.prune_opts.clone();
            prune_opts.opts.ignore_snaps = forget_snaps;
            prune_opts.run();
        }

        Ok(())
    }
}

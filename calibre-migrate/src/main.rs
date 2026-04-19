use clap::Parser;
use std::path::PathBuf;

#[derive(Debug, Parser)]
#[command(name = "calibre-migrate")]
#[command(about = "Migrate metadata and files from a Calibre library")]
struct Cli {
    #[arg(long)]
    source: PathBuf,

    #[arg(long = "target-db")]
    target_db: PathBuf,

    #[arg(long = "target-storage")]
    target_storage: PathBuf,

    #[arg(long = "dry-run", default_value_t = false)]
    dry_run: bool,

    #[arg(long = "report-path")]
    report_path: Option<PathBuf>,
}

fn main() -> anyhow::Result<()> {
    let _cli = Cli::parse();
    todo!("phase 2 scaffold: wire migration CLI execution")
}

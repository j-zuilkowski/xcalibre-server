use std::path::{Path, PathBuf};
use std::str::FromStr;

use clap::Parser;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};

use calibre_migrate::calibre::reader::CalibreReader;
use calibre_migrate::import::pipeline::{ImportPipeline, LocalFs};

#[derive(Debug, Parser)]
#[command(name = "calibre-migrate")]
#[command(about = "Migrate metadata and files from a Calibre library")]
struct Cli {
    #[arg(long)]
    source: PathBuf,

    #[arg(long = "target-db")]
    target_db: String,

    #[arg(long = "target-storage")]
    target_storage: PathBuf,

    #[arg(long = "dry-run", default_value_t = false)]
    dry_run: bool,

    #[arg(long = "report-path")]
    report_path: Option<PathBuf>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let report = run(cli).await?;

    report.print_summary();

    if report.failed > 0 {
        std::process::exit(1);
    }

    Ok(())
}

async fn run(cli: Cli) -> anyhow::Result<calibre_migrate::report::MigrationReport> {
    let target_db = connect_and_migrate(&cli.target_db).await?;

    let reader = CalibreReader::open(&cli.source)?;
    let entries = reader.read_all_entries()?;

    let pipeline = ImportPipeline::new(target_db, LocalFs::new(&cli.target_storage), cli.dry_run);
    let report = pipeline.run(entries, &reader).await?;

    if let Some(path) = cli.report_path {
        write_report(&path, &report)?;
    }

    Ok(report)
}

async fn connect_and_migrate(target_db_url: &str) -> anyhow::Result<sqlx::SqlitePool> {
    let options = SqliteConnectOptions::from_str(target_db_url)
        .map_err(anyhow::Error::from)?
        .create_if_missing(true);

    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await?;

    let migration_path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../backend/migrations/sqlite");
    let migrator = sqlx::migrate::Migrator::new(migration_path.as_path()).await?;
    migrator.run(&pool).await?;

    Ok(pool)
}

fn write_report(
    report_path: &Path,
    report: &calibre_migrate::report::MigrationReport,
) -> anyhow::Result<()> {
    if let Some(parent) = report_path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }

    std::fs::write(report_path, report.to_json())?;
    Ok(())
}

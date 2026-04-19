use std::path::Path;

use crate::report::MigrationReport;

#[derive(Debug, Default)]
pub struct ImportPipeline;

impl ImportPipeline {
    pub fn run(
        &self,
        _source: &Path,
        _target_db: &Path,
        _target_storage: &Path,
        _dry_run: bool,
    ) -> anyhow::Result<MigrationReport> {
        todo!("phase 2 scaffold: run import pipeline")
    }
}

use serde::Serialize;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct FailureRecord {
    pub calibre_id: i64,
    pub title: String,
    pub reason: String,
}

#[derive(Debug, Clone, Default, Serialize, PartialEq, Eq)]
pub struct MigrationReport {
    pub total: usize,
    pub imported: usize,
    pub skipped: usize,
    pub failed: usize,
    pub failures: Vec<FailureRecord>,
}

impl MigrationReport {
    pub fn print_summary(&self) {
        println!("Migration report");
        println!("  total: {}", self.total);
        println!("  imported: {}", self.imported);
        println!("  skipped: {}", self.skipped);
        println!("  failed: {}", self.failed);
        if !self.failures.is_empty() {
            println!("  failures:");
            for failure in &self.failures {
                println!(
                    "    calibre_id={} title={} reason={}",
                    failure.calibre_id, failure.title, failure.reason
                );
            }
        }
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_else(|_| "{}".to_string())
    }
}

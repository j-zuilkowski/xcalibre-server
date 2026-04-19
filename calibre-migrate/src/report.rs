#[derive(Debug, Clone, Default)]
pub struct MigrationReport {
    pub total_books: usize,
    pub imported_books: usize,
    pub skipped_books: usize,
    pub failed_books: usize,
    pub copied_formats: usize,
    pub copied_covers: usize,
    pub dry_run: bool,
}

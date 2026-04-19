use std::path::Path;

use super::schema::{
    CalibeSeries, CalibreAuthor, CalibreBook, CalibreComment, CalibreFormat, CalibreIdentifier,
    CalibreTag,
};

#[derive(Debug, Default)]
pub struct CalibreReader;

impl CalibreReader {
    pub fn from_metadata_db(_metadata_db_path: &Path) -> Self {
        todo!("phase 2 scaffold: initialize Calibre metadata reader")
    }

    pub fn load_books(&self) -> Vec<CalibreBook> {
        todo!("phase 2 scaffold: load books from metadata.db")
    }

    pub fn load_authors(&self) -> Vec<CalibreAuthor> {
        todo!("phase 2 scaffold: load authors from metadata.db")
    }

    pub fn load_formats(&self) -> Vec<CalibreFormat> {
        todo!("phase 2 scaffold: load formats from metadata.db")
    }

    pub fn load_identifiers(&self) -> Vec<CalibreIdentifier> {
        todo!("phase 2 scaffold: load identifiers from metadata.db")
    }

    pub fn load_comments(&self) -> Vec<CalibreComment> {
        todo!("phase 2 scaffold: load comments from metadata.db")
    }

    pub fn load_series(&self) -> Vec<CalibeSeries> {
        todo!("phase 2 scaffold: load series from metadata.db")
    }

    pub fn load_tags(&self) -> Vec<CalibreTag> {
        todo!("phase 2 scaffold: load tags from metadata.db")
    }
}

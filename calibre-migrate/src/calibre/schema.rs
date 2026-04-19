#[derive(Debug, Clone, Default)]
pub struct CalibreBook {
    pub id: i64,
    pub title: String,
    pub sort: String,
    pub author_sort: String,
    pub pubdate: Option<String>,
    pub series_index: Option<f64>,
    pub rating: Option<i64>,
    pub has_cover: bool,
    pub last_modified: String,
}

#[derive(Debug, Clone, Default)]
pub struct CalibreAuthor {
    pub id: i64,
    pub name: String,
    pub sort: String,
}

#[derive(Debug, Clone, Default)]
pub struct CalibeSeries {
    pub id: i64,
    pub name: String,
    pub sort: String,
}

#[derive(Debug, Clone, Default)]
pub struct CalibreTag {
    pub id: i64,
    pub name: String,
}

#[derive(Debug, Clone, Default)]
pub struct CalibreFormat {
    pub id: i64,
    pub book_id: i64,
    pub format: String,
    pub name: String,
    pub uncompressed_size: Option<i64>,
}

#[derive(Debug, Clone, Default)]
pub struct CalibreIdentifier {
    pub id: i64,
    pub book_id: i64,
    pub id_type: String,
    pub value: String,
}

#[derive(Debug, Clone, Default)]
pub struct CalibreComment {
    pub id: i64,
    pub book_id: i64,
    pub text: String,
}

#[derive(Debug, Clone, Default)]
pub struct CalibreEntry {
    pub book: CalibreBook,
    pub authors: Vec<CalibreAuthor>,
    pub series: Option<CalibeSeries>,
    pub tags: Vec<CalibreTag>,
    pub formats: Vec<CalibreFormat>,
    pub identifiers: Vec<CalibreIdentifier>,
    pub comment: Option<CalibreComment>,
}

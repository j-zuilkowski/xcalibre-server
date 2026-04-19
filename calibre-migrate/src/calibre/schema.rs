#[derive(Debug, Clone, Default)]
pub struct CalibreBook {
    pub id: i64,
    pub title: String,
    pub sort_title: Option<String>,
    pub path: Option<String>,
    pub has_cover: bool,
    pub authors: Vec<CalibreAuthor>,
    pub series: Option<CalibeSeries>,
    pub tags: Vec<CalibreTag>,
    pub formats: Vec<CalibreFormat>,
    pub identifiers: Vec<CalibreIdentifier>,
    pub comment: Option<CalibreComment>,
}

#[derive(Debug, Clone, Default)]
pub struct CalibreAuthor {
    pub id: i64,
    pub name: String,
    pub sort_name: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct CalibeSeries {
    pub id: i64,
    pub name: String,
    pub sort_name: Option<String>,
    pub series_index: Option<f64>,
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
    pub path: String,
    pub size_bytes: Option<i64>,
}

#[derive(Debug, Clone, Default)]
pub struct CalibreIdentifier {
    pub id: i64,
    pub book_id: i64,
    pub kind: String,
    pub value: String,
}

#[derive(Debug, Clone, Default)]
pub struct CalibreComment {
    pub id: i64,
    pub book_id: i64,
    pub text: String,
}

use std::fmt;
use std::io::Read;
use std::ops::Deref;

#[derive(Clone, Copy, Debug, Default)]
pub enum Trim {
    #[default]
    None,
    All,
}

#[derive(Debug, Clone)]
pub struct Error {
    message: String,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for Error {}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct StringRecord {
    fields: Vec<String>,
}

impl StringRecord {
    pub fn iter(&self) -> impl Iterator<Item = &str> {
        self.fields.iter().map(String::as_str)
    }

    pub fn get(&self, index: usize) -> Option<&str> {
        self.fields.get(index).map(String::as_str)
    }

    pub fn len(&self) -> usize {
        self.fields.len()
    }
}

#[derive(Debug, Default)]
pub struct ReaderBuilder {
    trim: Trim,
    flexible: bool,
}

impl ReaderBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn trim(mut self, trim: Trim) -> Self {
        self.trim = trim;
        self
    }

    pub fn flexible(mut self, flexible: bool) -> Self {
        self.flexible = flexible;
        self
    }

    pub fn from_reader<R: Read>(self, mut reader: R) -> Reader {
        let mut buffer = Vec::new();
        let _ = reader.read_to_end(&mut buffer);
        Reader::from_bytes(buffer, self.trim, self.flexible)
    }
}

#[derive(Debug)]
pub struct Reader {
    headers: StringRecord,
    records: Vec<StringRecord>,
}

impl Reader {
    fn from_bytes(bytes: Vec<u8>, trim: Trim, flexible: bool) -> Self {
        let text = String::from_utf8(bytes).unwrap_or_default();
        let mut rows = parse_csv(&text);
        let headers = rows.first().cloned().unwrap_or_default();
        let records = if rows.len() > 1 {
            rows.drain(1..).collect::<Vec<_>>()
        } else {
            Vec::new()
        };

        let headers = apply_trim(headers, trim);
        let mut records = records.into_iter().map(|record| apply_trim(record, trim)).collect::<Vec<_>>();
        if !flexible {
            let expected = headers.len();
            records.retain(|record| record.len() == expected);
        }

        Self { headers, records }
    }

    pub fn headers(&mut self) -> Result<&StringRecord, Error> {
        Ok(&self.headers)
    }

    pub fn records(&self) -> Records<'_> {
        Records {
            inner: self.records.iter(),
        }
    }
}

pub struct Records<'a> {
    inner: std::slice::Iter<'a, StringRecord>,
}

impl<'a> Iterator for Records<'a> {
    type Item = Result<StringRecord, Error>;

    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next().cloned().map(Ok)
    }
}

fn apply_trim(record: StringRecord, trim: Trim) -> StringRecord {
    match trim {
        Trim::None => record,
        Trim::All => StringRecord {
            fields: record.fields.into_iter().map(|value| value.trim().to_string()).collect(),
        },
    }
}

fn parse_csv(input: &str) -> Vec<StringRecord> {
    let mut rows = Vec::new();
    let mut current_row = Vec::new();
    let mut current_field = String::new();
    let mut chars = input.chars().peekable();
    let mut in_quotes = false;

    while let Some(ch) = chars.next() {
        match ch {
            '"' => {
                if in_quotes && matches!(chars.peek(), Some('"')) {
                    current_field.push('"');
                    let _ = chars.next();
                } else {
                    in_quotes = !in_quotes;
                }
            }
            ',' if !in_quotes => {
                current_row.push(std::mem::take(&mut current_field));
            }
            '\n' if !in_quotes => {
                current_row.push(std::mem::take(&mut current_field));
                rows.push(StringRecord {
                    fields: std::mem::take(&mut current_row),
                });
            }
            '\r' if !in_quotes => {
                if matches!(chars.peek(), Some('\n')) {
                    let _ = chars.next();
                }
                current_row.push(std::mem::take(&mut current_field));
                rows.push(StringRecord {
                    fields: std::mem::take(&mut current_row),
                });
            }
            _ => current_field.push(ch),
        }
    }

    if !current_field.is_empty() || !current_row.is_empty() {
        current_row.push(current_field);
        rows.push(StringRecord {
            fields: current_row,
        });
    }

    rows.into_iter().filter(|record| !record.fields.iter().all(|field| field.is_empty())).collect()
}

impl Deref for StringRecord {
    type Target = [String];

    fn deref(&self) -> &Self::Target {
        &self.fields
    }
}

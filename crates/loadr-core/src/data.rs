//! Data parameterization: CSV and inline data sources with shared or per-VU
//! cursors and recycle/stop-at-EOF semantics.

use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use indexmap::IndexMap;
use loadr_config::{DataMode, DataSource, OnEof};

use crate::error::EngineError;

/// One data row: column name → string value.
pub type Row = IndexMap<String, String>;

#[derive(Debug)]
struct Feed {
    rows: Vec<Arc<Row>>,
    mode: DataMode,
    on_eof: OnEof,
    shared_cursor: AtomicUsize,
}

/// All loaded data sources for a test.
#[derive(Debug, Default)]
pub struct DataFeeds {
    feeds: HashMap<String, Feed>,
}

/// Signalled when a `stop`-mode source is exhausted: the VU should retire.
#[derive(Debug, thiserror::Error)]
#[error("data source `{0}` is exhausted")]
pub struct EndOfData(pub String);

impl DataFeeds {
    /// Load every source declared in the plan. CSV paths resolve against `base_dir`.
    pub fn load(
        sources: &IndexMap<String, DataSource>,
        base_dir: &Path,
    ) -> Result<DataFeeds, EngineError> {
        let mut feeds = HashMap::new();
        for (name, source) in sources {
            let feed = match source {
                DataSource::Csv {
                    path,
                    mode,
                    on_eof,
                    delimiter,
                    has_header,
                } => {
                    let resolved = if path.is_absolute() {
                        path.clone()
                    } else {
                        base_dir.join(path)
                    };
                    let raw = std::fs::read(&resolved).map_err(|e| EngineError::Data {
                        source_name: name.clone(),
                        message: format!("cannot read {}: {e}", resolved.display()),
                    })?;
                    let mut builder = csv::ReaderBuilder::new();
                    builder.has_headers(*has_header);
                    if let Some(d) = delimiter {
                        builder.delimiter(*d as u8);
                    }
                    let mut reader = builder.from_reader(raw.as_slice());
                    let headers: Vec<String> = if *has_header {
                        reader
                            .headers()
                            .map_err(|e| EngineError::Data {
                                source_name: name.clone(),
                                message: e.to_string(),
                            })?
                            .iter()
                            .map(str::to_string)
                            .collect()
                    } else {
                        Vec::new()
                    };
                    let mut rows = Vec::new();
                    for record in reader.records() {
                        let record = record.map_err(|e| EngineError::Data {
                            source_name: name.clone(),
                            message: e.to_string(),
                        })?;
                        let mut row = Row::new();
                        for (i, field) in record.iter().enumerate() {
                            let key = headers.get(i).cloned().unwrap_or_else(|| format!("col{i}"));
                            row.insert(key, field.to_string());
                        }
                        rows.push(Arc::new(row));
                    }
                    if rows.is_empty() {
                        return Err(EngineError::Data {
                            source_name: name.clone(),
                            message: "CSV has no data rows".into(),
                        });
                    }
                    Feed {
                        rows,
                        mode: *mode,
                        on_eof: *on_eof,
                        shared_cursor: AtomicUsize::new(0),
                    }
                }
                DataSource::Inline { rows, mode, on_eof } => {
                    let converted: Vec<Arc<Row>> = rows
                        .iter()
                        .map(|r| {
                            Arc::new(
                                r.iter()
                                    .map(|(k, v)| (k.clone(), json_to_string(v)))
                                    .collect::<Row>(),
                            )
                        })
                        .collect();
                    if converted.is_empty() {
                        return Err(EngineError::Data {
                            source_name: name.clone(),
                            message: "inline data has no rows".into(),
                        });
                    }
                    Feed {
                        rows: converted,
                        mode: *mode,
                        on_eof: *on_eof,
                        shared_cursor: AtomicUsize::new(0),
                    }
                }
            };
            feeds.insert(name.clone(), feed);
        }
        Ok(DataFeeds { feeds })
    }

    pub fn has_source(&self, name: &str) -> bool {
        self.feeds.contains_key(name)
    }

    pub fn source_names(&self) -> Vec<&str> {
        self.feeds.keys().map(String::as_str).collect()
    }

    /// Fetch the next row for `source`, using the VU's cursor map for per-VU mode.
    pub fn next_row(
        &self,
        source: &str,
        vu_cursors: &mut HashMap<String, usize>,
    ) -> Result<Arc<Row>, NextRowError> {
        let feed = self
            .feeds
            .get(source)
            .ok_or_else(|| NextRowError::UnknownSource(source.to_string()))?;
        let len = feed.rows.len();
        let idx = match feed.mode {
            DataMode::Shared => feed.shared_cursor.fetch_add(1, Ordering::Relaxed),
            DataMode::PerVu => {
                let c = vu_cursors.entry(source.to_string()).or_insert(0);
                let idx = *c;
                *c += 1;
                idx
            }
        };
        if idx >= len {
            match feed.on_eof {
                OnEof::Recycle => Ok(feed.rows[idx % len].clone()),
                OnEof::Stop => Err(NextRowError::Exhausted(EndOfData(source.to_string()))),
            }
        } else {
            Ok(feed.rows[idx].clone())
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum NextRowError {
    #[error("unknown data source `{0}`")]
    UnknownSource(String),
    #[error(transparent)]
    Exhausted(#[from] EndOfData),
}

fn json_to_string(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn csv_source(dir: &Path, content: &str, mode: DataMode, on_eof: OnEof) -> DataFeeds {
        let path = dir.join("users.csv");
        let mut f = std::fs::File::create(&path).expect("create");
        f.write_all(content.as_bytes()).expect("write");
        let mut sources = IndexMap::new();
        sources.insert(
            "users".to_string(),
            DataSource::Csv {
                path: "users.csv".into(),
                mode,
                on_eof,
                delimiter: None,
                has_header: true,
            },
        );
        DataFeeds::load(&sources, dir).expect("load")
    }

    #[test]
    fn shared_cursor_distributes_rows() {
        let dir = tempfile::tempdir().expect("tmp");
        let feeds = csv_source(
            dir.path(),
            "user,pass\nu1,p1\nu2,p2\nu3,p3\n",
            DataMode::Shared,
            OnEof::Recycle,
        );
        let mut cursors = HashMap::new();
        let r1 = feeds.next_row("users", &mut cursors).expect("row");
        let r2 = feeds.next_row("users", &mut cursors).expect("row");
        assert_eq!(r1["user"], "u1");
        assert_eq!(r2["user"], "u2");
    }

    #[test]
    fn recycle_wraps() {
        let dir = tempfile::tempdir().expect("tmp");
        let feeds = csv_source(
            dir.path(),
            "user\nu1\nu2\n",
            DataMode::Shared,
            OnEof::Recycle,
        );
        let mut cursors = HashMap::new();
        for _ in 0..2 {
            feeds.next_row("users", &mut cursors).expect("row");
        }
        let wrapped = feeds.next_row("users", &mut cursors).expect("row");
        assert_eq!(wrapped["user"], "u1");
    }

    #[test]
    fn stop_at_eof() {
        let dir = tempfile::tempdir().expect("tmp");
        let feeds = csv_source(dir.path(), "user\nu1\n", DataMode::Shared, OnEof::Stop);
        let mut cursors = HashMap::new();
        feeds.next_row("users", &mut cursors).expect("row");
        assert!(matches!(
            feeds.next_row("users", &mut cursors),
            Err(NextRowError::Exhausted(_))
        ));
    }

    #[test]
    fn per_vu_cursors_are_independent() {
        let dir = tempfile::tempdir().expect("tmp");
        let feeds = csv_source(
            dir.path(),
            "user\nu1\nu2\n",
            DataMode::PerVu,
            OnEof::Recycle,
        );
        let mut vu1 = HashMap::new();
        let mut vu2 = HashMap::new();
        assert_eq!(feeds.next_row("users", &mut vu1).unwrap()["user"], "u1");
        assert_eq!(feeds.next_row("users", &mut vu2).unwrap()["user"], "u1");
        assert_eq!(feeds.next_row("users", &mut vu1).unwrap()["user"], "u2");
    }

    #[test]
    fn inline_rows() {
        let mut sources = IndexMap::new();
        let mut row = IndexMap::new();
        row.insert("id".to_string(), serde_json::json!(7));
        row.insert("name".to_string(), serde_json::json!("alpha"));
        sources.insert(
            "items".to_string(),
            DataSource::Inline {
                rows: vec![row],
                mode: DataMode::Shared,
                on_eof: OnEof::Recycle,
            },
        );
        let feeds = DataFeeds::load(&sources, Path::new(".")).expect("load");
        let mut cursors = HashMap::new();
        let r = feeds.next_row("items", &mut cursors).expect("row");
        assert_eq!(r["id"], "7");
        assert_eq!(r["name"], "alpha");
    }

    #[test]
    fn headerless_csv_gets_positional_columns() {
        let dir = tempfile::tempdir().expect("tmp");
        let path = dir.path().join("data.csv");
        std::fs::write(&path, "a,b\nc,d\n").expect("write");
        let mut sources = IndexMap::new();
        sources.insert(
            "d".to_string(),
            DataSource::Csv {
                path: "data.csv".into(),
                mode: DataMode::Shared,
                on_eof: OnEof::Recycle,
                delimiter: None,
                has_header: false,
            },
        );
        let feeds = DataFeeds::load(&sources, dir.path()).expect("load");
        let mut cursors = HashMap::new();
        let r = feeds.next_row("d", &mut cursors).expect("row");
        assert_eq!(r["col0"], "a");
        assert_eq!(r["col1"], "b");
    }
}

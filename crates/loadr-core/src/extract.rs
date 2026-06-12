//! Correlation/extraction: pull values out of responses into VU variables.

use loadr_config::{Extractor, MatchIndex};
use rand::RngExt;

use crate::protocol::ProtocolResponse;

/// A compiled extractor (regexes/paths parsed once at plan compile time).
#[derive(Debug)]
pub enum CompiledExtractor {
    Jsonpath {
        name: String,
        path: serde_json_path::JsonPath,
        default: Option<String>,
        index: MatchIndex,
    },
    Regex {
        name: String,
        regex: regex::Regex,
        group: usize,
        default: Option<String>,
        index: MatchIndex,
    },
    Xpath {
        name: String,
        expression: String,
        default: Option<String>,
    },
    Css {
        name: String,
        selector: scraper::Selector,
        attribute: Option<String>,
        default: Option<String>,
        index: MatchIndex,
    },
    Boundary {
        name: String,
        left: String,
        right: String,
        default: Option<String>,
        index: MatchIndex,
    },
    Header {
        name: String,
        header: String,
        default: Option<String>,
    },
}

#[derive(Debug, thiserror::Error)]
pub enum ExtractError {
    #[error("extractor `{0}` is invalid: {1}")]
    Invalid(String, String),
    #[error("extractor `{name}` found no match and has no default")]
    NoMatch { name: String },
}

impl CompiledExtractor {
    pub fn compile(spec: &Extractor) -> Result<Self, ExtractError> {
        Ok(match spec {
            Extractor::Jsonpath {
                name,
                expression,
                default,
                index,
            } => CompiledExtractor::Jsonpath {
                name: name.clone(),
                path: serde_json_path::JsonPath::parse(expression)
                    .map_err(|e| ExtractError::Invalid(name.clone(), e.to_string()))?,
                default: default.clone(),
                index: index.unwrap_or_default(),
            },
            Extractor::Regex {
                name,
                expression,
                group,
                default,
                index,
            } => CompiledExtractor::Regex {
                name: name.clone(),
                regex: regex::Regex::new(expression)
                    .map_err(|e| ExtractError::Invalid(name.clone(), e.to_string()))?,
                group: group.unwrap_or(1),
                default: default.clone(),
                index: index.unwrap_or_default(),
            },
            Extractor::Xpath {
                name,
                expression,
                default,
            } => CompiledExtractor::Xpath {
                name: name.clone(),
                expression: expression.clone(),
                default: default.clone(),
            },
            Extractor::Css {
                name,
                expression,
                attribute,
                default,
                index,
            } => CompiledExtractor::Css {
                name: name.clone(),
                selector: scraper::Selector::parse(expression)
                    .map_err(|e| ExtractError::Invalid(name.clone(), e.to_string()))?,
                attribute: attribute.clone(),
                default: default.clone(),
                index: index.unwrap_or_default(),
            },
            Extractor::Boundary {
                name,
                left,
                right,
                default,
                index,
            } => CompiledExtractor::Boundary {
                name: name.clone(),
                left: left.clone(),
                right: right.clone(),
                default: default.clone(),
                index: index.unwrap_or_default(),
            },
            Extractor::Header {
                name,
                header,
                default,
            } => CompiledExtractor::Header {
                name: name.clone(),
                header: header.clone(),
                default: default.clone(),
            },
        })
    }

    pub fn name(&self) -> &str {
        match self {
            CompiledExtractor::Jsonpath { name, .. }
            | CompiledExtractor::Regex { name, .. }
            | CompiledExtractor::Xpath { name, .. }
            | CompiledExtractor::Css { name, .. }
            | CompiledExtractor::Boundary { name, .. }
            | CompiledExtractor::Header { name, .. } => name,
        }
    }

    fn default(&self) -> Option<&str> {
        match self {
            CompiledExtractor::Jsonpath { default, .. }
            | CompiledExtractor::Regex { default, .. }
            | CompiledExtractor::Xpath { default, .. }
            | CompiledExtractor::Css { default, .. }
            | CompiledExtractor::Boundary { default, .. }
            | CompiledExtractor::Header { default, .. } => default.as_deref(),
        }
    }

    /// Run the extractor; returns the extracted value as a JSON value
    /// (JSONPath keeps native types; everything else yields strings).
    pub fn extract(
        &self,
        response: &ProtocolResponse,
        rng: &mut impl RngExt,
    ) -> Result<serde_json::Value, ExtractError> {
        let result: Option<serde_json::Value> = match self {
            CompiledExtractor::Jsonpath { path, index, .. } => {
                let body: serde_json::Value =
                    serde_json::from_slice(&response.body).unwrap_or(serde_json::Value::Null);
                let nodes = path.query(&body);
                let all: Vec<serde_json::Value> = nodes.iter().map(|v| (*v).clone()).collect();
                pick(all, *index, rng)
            }
            CompiledExtractor::Regex {
                regex,
                group,
                index,
                ..
            } => {
                let text = response.body_text();
                let all: Vec<serde_json::Value> = regex
                    .captures_iter(&text)
                    .filter_map(|c| {
                        c.get(*group)
                            .map(|m| serde_json::Value::String(m.as_str().to_string()))
                    })
                    .collect();
                pick(all, *index, rng)
            }
            CompiledExtractor::Xpath {
                name, expression, ..
            } => {
                let text = response.body_text();
                xpath_eval(&text, expression)
                    .map_err(|e| ExtractError::Invalid(name.clone(), e))?
                    .map(serde_json::Value::String)
            }
            CompiledExtractor::Css {
                selector,
                attribute,
                index,
                ..
            } => {
                let text = response.body_text();
                let doc = scraper::Html::parse_document(&text);
                let all: Vec<serde_json::Value> = doc
                    .select(selector)
                    .filter_map(|el| match attribute {
                        Some(attr) => el.attr(attr).map(str::to_string),
                        None => Some(el.text().collect::<String>()),
                    })
                    .map(serde_json::Value::String)
                    .collect();
                pick(all, *index, rng)
            }
            CompiledExtractor::Boundary {
                left, right, index, ..
            } => {
                let text = response.body_text();
                let mut all = Vec::new();
                let mut at = 0usize;
                while let Some(start) = text[at..].find(left.as_str()) {
                    let vstart = at + start + left.len();
                    match text[vstart..].find(right.as_str()) {
                        Some(end) => {
                            all.push(serde_json::Value::String(
                                text[vstart..vstart + end].to_string(),
                            ));
                            at = vstart + end + right.len();
                        }
                        None => break,
                    }
                }
                pick(all, *index, rng)
            }
            CompiledExtractor::Header { header, .. } => response
                .header(header)
                .map(|v| serde_json::Value::String(v.to_string())),
        };

        match result {
            Some(v) => Ok(v),
            None => match self.default() {
                Some(d) => Ok(serde_json::Value::String(d.to_string())),
                None => Err(ExtractError::NoMatch {
                    name: self.name().to_string(),
                }),
            },
        }
    }
}

fn pick(
    mut all: Vec<serde_json::Value>,
    index: MatchIndex,
    rng: &mut impl RngExt,
) -> Option<serde_json::Value> {
    if all.is_empty() {
        return None;
    }
    match index {
        MatchIndex::First => Some(all.remove(0)),
        MatchIndex::Last => all.pop(),
        MatchIndex::Random => {
            let i = rng.random_range(0..all.len());
            Some(all.swap_remove(i))
        }
        MatchIndex::All => Some(serde_json::Value::Array(all)),
    }
}

/// Evaluate an XPath 1.0 expression against an XML document.
pub fn xpath_eval(xml: &str, expression: &str) -> Result<Option<String>, String> {
    let package = sxd_document::parser::parse(xml).map_err(|e| format!("XML parse: {e}"))?;
    let doc = package.as_document();
    let factory = sxd_xpath::Factory::new();
    let xpath = factory
        .build(expression)
        .map_err(|e| format!("XPath build: {e}"))?
        .ok_or_else(|| "empty XPath".to_string())?;
    let context = sxd_xpath::Context::new();
    let value = xpath
        .evaluate(&context, doc.root())
        .map_err(|e| format!("XPath eval: {e}"))?;
    Ok(match value {
        sxd_xpath::Value::Nodeset(ns) => ns
            .document_order_first()
            .map(|n| n.string_value().trim().to_string()),
        sxd_xpath::Value::String(s) => {
            if s.is_empty() {
                None
            } else {
                Some(s)
            }
        }
        sxd_xpath::Value::Number(n) => Some(n.to_string()),
        sxd_xpath::Value::Boolean(b) => Some(b.to_string()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::Bytes;
    use rand::SeedableRng;

    fn response(body: &str, headers: &[(&str, &str)]) -> ProtocolResponse {
        ProtocolResponse {
            body: Bytes::from(body.to_string()),
            headers: headers
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
            ..Default::default()
        }
    }

    fn rng() -> rand::rngs::SmallRng {
        rand::rngs::SmallRng::seed_from_u64(42)
    }

    fn compile(yaml: &str) -> CompiledExtractor {
        let spec: Extractor = serde_yaml::from_str(yaml).expect("spec");
        CompiledExtractor::compile(&spec).expect("compile")
    }

    #[test]
    fn jsonpath_keeps_types() {
        let ex = compile(r#"{ type: jsonpath, name: id, expression: "$.items[0].id" }"#);
        let r = response(r#"{"items":[{"id":42},{"id":43}]}"#, &[]);
        assert_eq!(ex.extract(&r, &mut rng()).unwrap(), serde_json::json!(42));
    }

    #[test]
    fn jsonpath_all_matches() {
        let ex =
            compile(r#"{ type: jsonpath, name: ids, expression: "$.items[*].id", index: all }"#);
        let r = response(r#"{"items":[{"id":1},{"id":2}]}"#, &[]);
        assert_eq!(
            ex.extract(&r, &mut rng()).unwrap(),
            serde_json::json!([1, 2])
        );
    }

    #[test]
    fn regex_groups_and_index() {
        let r = response("a=1 a=2 a=3", &[]);
        let first = compile(r#"{ type: regex, name: x, expression: "a=(\\d)" }"#);
        assert_eq!(
            first.extract(&r, &mut rng()).unwrap(),
            serde_json::json!("1")
        );
        let last = compile(r#"{ type: regex, name: x, expression: "a=(\\d)", index: last }"#);
        assert_eq!(
            last.extract(&r, &mut rng()).unwrap(),
            serde_json::json!("3")
        );
    }

    #[test]
    fn regex_default_on_no_match() {
        let ex = compile(r#"{ type: regex, name: x, expression: "z=(\\d)", default: fallback }"#);
        let r = response("nothing here", &[]);
        assert_eq!(
            ex.extract(&r, &mut rng()).unwrap(),
            serde_json::json!("fallback")
        );
    }

    #[test]
    fn no_match_no_default_errors() {
        let ex = compile(r#"{ type: regex, name: x, expression: "z=(\\d)" }"#);
        let r = response("nothing", &[]);
        assert!(matches!(
            ex.extract(&r, &mut rng()),
            Err(ExtractError::NoMatch { .. })
        ));
    }

    #[test]
    fn xpath_extraction() {
        let ex = compile(r#"{ type: xpath, name: n, expression: "//item[@id='2']/name" }"#);
        let r = response(
            r#"<catalog><item id="1"><name>alpha</name></item><item id="2"><name>beta</name></item></catalog>"#,
            &[],
        );
        assert_eq!(
            ex.extract(&r, &mut rng()).unwrap(),
            serde_json::json!("beta")
        );
    }

    #[test]
    fn css_selector_attribute() {
        let ex = compile(
            r#"{ type: css, name: csrf, expression: "input[name=csrf]", attribute: value }"#,
        );
        let r = response(
            r#"<html><body><form><input name="csrf" value="tok-1"></form></body></html>"#,
            &[],
        );
        assert_eq!(
            ex.extract(&r, &mut rng()).unwrap(),
            serde_json::json!("tok-1")
        );
    }

    #[test]
    fn css_selector_text() {
        let ex = compile(r#"{ type: css, name: t, expression: "h1" }"#);
        let r = response("<html><h1>Hello</h1></html>", &[]);
        assert_eq!(
            ex.extract(&r, &mut rng()).unwrap(),
            serde_json::json!("Hello")
        );
    }

    #[test]
    fn boundary_extraction() {
        let ex = compile(r#"{ type: boundary, name: b, left: "token=\"", right: "\"" }"#);
        let r = response(r#"<a token="abc123">x</a>"#, &[]);
        assert_eq!(
            ex.extract(&r, &mut rng()).unwrap(),
            serde_json::json!("abc123")
        );
    }

    #[test]
    fn header_extraction() {
        let ex = compile(r#"{ type: header, name: loc, header: Location }"#);
        let r = response("", &[("location", "/next")]);
        assert_eq!(
            ex.extract(&r, &mut rng()).unwrap(),
            serde_json::json!("/next")
        );
    }
}

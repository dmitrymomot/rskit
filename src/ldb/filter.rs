use std::collections::{HashMap, HashSet};

use crate::error::{Error, Result};

/// Defines allowed filter fields and sort fields for an endpoint.
#[derive(Default)]
pub struct FilterSchema {
    fields: HashMap<String, FieldType>,
    sort_fields: HashSet<String>,
}

/// Column type for validation.
#[derive(Debug, Clone, Copy)]
pub enum FieldType {
    Text,
    Int,
    Float,
    Date,
    Bool,
}

impl FilterSchema {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn field(mut self, name: &str, typ: FieldType) -> Self {
        self.fields.insert(name.to_string(), typ);
        self
    }

    pub fn sort_fields(mut self, fields: &[&str]) -> Self {
        self.sort_fields = fields.iter().map(|s| s.to_string()).collect();
        self
    }

    fn field_type(&self, name: &str) -> Option<FieldType> {
        self.fields.get(name).copied()
    }

    fn is_sort_field(&self, name: &str) -> bool {
        self.sort_fields.contains(name)
    }
}

/// Parsed operator from query string.
#[derive(Debug, Clone)]
enum Operator {
    Eq,
    Ne,
    Gt,
    Gte,
    Lt,
    Lte,
    Like,
    IsNull(bool),
    In,
}

/// A single parsed filter condition.
#[derive(Debug, Clone)]
struct FilterCondition {
    column: String,
    operator: Operator,
    values: Vec<String>,
}

/// Raw parsed filter from query string. Must be validated before use.
pub struct Filter {
    conditions: Vec<FilterCondition>,
    sort: Option<String>,
}

/// Validated filter — safe to use in SQL generation.
pub struct ValidatedFilter {
    pub clauses: Vec<String>,
    pub params: Vec<libsql::Value>,
    pub sort_clause: Option<String>,
}

impl ValidatedFilter {
    pub fn is_empty(&self) -> bool {
        self.clauses.is_empty()
    }
}

impl Filter {
    /// Parse filter conditions from a query string map.
    pub fn from_query_params(params: &HashMap<String, Vec<String>>) -> Self {
        let mut conditions: HashMap<String, FilterCondition> = HashMap::new();
        let mut sort = None;

        for (key, values) in params {
            if key == "sort" {
                if let Some(v) = values.first() {
                    sort = Some(v.clone());
                }
                continue;
            }

            // Skip pagination params
            if key == "page" || key == "per_page" || key == "after" {
                continue;
            }

            // Parse operator from key: "field.op" or just "field"
            let (column, op) = if let Some(dot_pos) = key.rfind('.') {
                let col = &key[..dot_pos];
                let op_str = &key[dot_pos + 1..];
                let op = match op_str {
                    "ne" => Operator::Ne,
                    "gt" => Operator::Gt,
                    "gte" => Operator::Gte,
                    "lt" => Operator::Lt,
                    "lte" => Operator::Lte,
                    "like" => Operator::Like,
                    "null" => {
                        let is_null = values.first().map(|v| v == "true").unwrap_or(true);
                        Operator::IsNull(is_null)
                    }
                    _ => continue, // Unknown operator — skip
                };
                (col.to_string(), op)
            } else {
                // No operator — Eq (single value) or In (multiple values)
                if values.len() > 1 {
                    (key.clone(), Operator::In)
                } else {
                    (key.clone(), Operator::Eq)
                }
            };

            conditions.insert(
                key.to_string(),
                FilterCondition {
                    column,
                    operator: op,
                    values: values.clone(),
                },
            );
        }

        Self {
            conditions: conditions.into_values().collect(),
            sort,
        }
    }

    /// Validate against a schema. Unknown columns are silently ignored.
    /// Type mismatches return a 400 error.
    pub fn validate(self, schema: &FilterSchema) -> Result<ValidatedFilter> {
        let mut clauses = Vec::new();
        let mut params: Vec<libsql::Value> = Vec::new();

        for cond in &self.conditions {
            let Some(field_type) = schema.field_type(&cond.column) else {
                continue; // Unknown column — silently ignore
            };

            match &cond.operator {
                Operator::IsNull(is_null) => {
                    if *is_null {
                        clauses.push(format!("{} IS NULL", cond.column));
                    } else {
                        clauses.push(format!("{} IS NOT NULL", cond.column));
                    }
                }
                Operator::In => {
                    let placeholders: Vec<String> =
                        cond.values.iter().map(|_| "?".to_string()).collect();
                    clauses.push(format!("{} IN ({})", cond.column, placeholders.join(", ")));
                    for val in &cond.values {
                        params.push(convert_value(val, field_type)?);
                    }
                }
                op => {
                    let sql_op = match op {
                        Operator::Eq => "=",
                        Operator::Ne => "!=",
                        Operator::Gt => ">",
                        Operator::Gte => ">=",
                        Operator::Lt => "<",
                        Operator::Lte => "<=",
                        Operator::Like => "LIKE",
                        _ => unreachable!(),
                    };
                    clauses.push(format!("{} {} ?", cond.column, sql_op));
                    let val = cond.values.first().ok_or_else(|| {
                        Error::bad_request(format!("missing value for filter '{}'", cond.column))
                    })?;
                    params.push(convert_value(val, field_type)?);
                }
            }
        }

        // Validate sort
        let sort_clause = self.sort.and_then(|s| {
            let (field, desc) = if let Some(stripped) = s.strip_prefix('-') {
                (stripped, true)
            } else {
                (s.as_str(), false)
            };
            if schema.is_sort_field(field) {
                let direction = if desc { "DESC" } else { "ASC" };
                Some(format!("{field} {direction}"))
            } else {
                None // Unknown sort field — ignore
            }
        });

        Ok(ValidatedFilter {
            clauses,
            params,
            sort_clause,
        })
    }
}

fn convert_value(val: &str, field_type: FieldType) -> Result<libsql::Value> {
    match field_type {
        FieldType::Text | FieldType::Date => Ok(libsql::Value::from(val.to_string())),
        FieldType::Int => {
            let n: i64 = val
                .parse()
                .map_err(|_| Error::bad_request(format!("invalid integer value: '{val}'")))?;
            Ok(libsql::Value::from(n))
        }
        FieldType::Float => {
            let n: f64 = val
                .parse()
                .map_err(|_| Error::bad_request(format!("invalid float value: '{val}'")))?;
            Ok(libsql::Value::from(n))
        }
        FieldType::Bool => {
            let b = matches!(val, "true" | "1" | "yes");
            Ok(libsql::Value::from(b as i32))
        }
    }
}

// axum extractor
impl<S: Send + Sync> axum::extract::FromRequestParts<S> for Filter {
    type Rejection = crate::error::Error;

    async fn from_request_parts(
        parts: &mut http::request::Parts,
        _state: &S,
    ) -> std::result::Result<Self, Self::Rejection> {
        let uri = &parts.uri;
        let query = uri.query().unwrap_or("");

        // Parse query string into HashMap<String, Vec<String>>
        let mut params: HashMap<String, Vec<String>> = HashMap::new();
        for pair in query.split('&') {
            if pair.is_empty() {
                continue;
            }
            let (key, value) = match pair.split_once('=') {
                Some((k, v)) => (k, v),
                None => (pair, ""),
            };
            let key = urlencoding::decode(key)
                .unwrap_or_else(|_| key.into())
                .to_string();
            let value = urlencoding::decode(value)
                .unwrap_or_else(|_| value.into())
                .to_string();
            params.entry(key).or_default().push(value);
        }

        Ok(Filter::from_query_params(&params))
    }
}

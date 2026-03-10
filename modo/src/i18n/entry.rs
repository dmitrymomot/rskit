#[derive(Debug, Clone, PartialEq)]
pub enum Entry {
    Plain(String),
    Plural {
        zero: Option<String>,
        one: Option<String>,
        other: String,
    },
}

#[derive(Debug, Clone)]
pub enum Entry {
    Plain(String),
    Plural {
        zero: Option<String>,
        one: Option<String>,
        other: String,
    },
}

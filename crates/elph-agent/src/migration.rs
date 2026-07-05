/// One versioned SQL migration applied to a local database.
pub struct Migration {
    pub version: i64,
    pub name: &'static str,
    pub up: &'static str,
}

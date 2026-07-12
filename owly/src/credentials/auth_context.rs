//! Owly [`AuthContext`] with env aliases for elph-ai provider resolution.

use elph_ai::{AuthContext, BoxFuture};

pub struct OwlyAuthContext;

impl AuthContext for OwlyAuthContext {
    fn env<'a>(&'a self, name: &'a str) -> BoxFuture<'a, Option<String>> {
        let name = name.to_string();
        Box::pin(async move {
            std::env::var(&name).ok().or_else(|| match name.as_str() {
                // elph-ai Google provider reads GEMINI_API_KEY; Owly historically used GOOGLE_API_KEY.
                "GEMINI_API_KEY" => std::env::var("GOOGLE_API_KEY").ok(),
                _ => None,
            })
        })
    }

    fn file_exists<'a>(&'a self, path: &'a str) -> BoxFuture<'a, bool> {
        let path = path.to_string();
        Box::pin(async move {
            if path.starts_with("~/") {
                let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
                let expanded = path.replacen('~', &home, 1);
                return std::path::Path::new(&expanded).exists();
            }
            std::path::Path::new(&path).exists()
        })
    }
}

//! Small text helpers shared by adapters and analysis.

/// True when `name` appears in `sql` as a whole identifier (case-insensitive, quotes allowed).
pub fn mentions_identifier(sql: &str, name: &str) -> bool {
    if name.is_empty() {
        return false;
    }
    let hay = sql.to_lowercase();
    let needle = name.to_lowercase();
    let is_ident = |c: char| c.is_alphanumeric() || c == '_';
    let mut start = 0;
    while let Some(pos) = hay[start..].find(&needle) {
        let i = start + pos;
        let before = hay[..i].chars().last();
        let after = hay[i + needle.len()..].chars().next();
        if !before.is_some_and(is_ident) && !after.is_some_and(is_ident) {
            return true;
        }
        start = i + needle.len();
    }
    false
}

/// Quotes an identifier for display in generated SQL, doubling embedded quote characters.
pub fn quote_ident(engine: super::Engine, ident: &str) -> String {
    match engine {
        super::Engine::Mysql => format!("`{}`", ident.replace('`', "``")),
        _ => format!("\"{}\"", ident.replace('"', "\"\"")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metadata::Engine;

    #[test]
    fn whole_identifier_matching() {
        assert!(mentions_identifier("SELECT * FROM orders o", "orders"));
        assert!(mentions_identifier("select * from \"Orders\"", "orders"));
        assert!(!mentions_identifier(
            "SELECT * FROM orders_archive",
            "orders"
        ));
        assert!(!mentions_identifier("SELECT reorders FROM x", "orders"));
        assert!(!mentions_identifier("anything", ""));
    }

    #[test]
    fn identifiers_are_quoted_and_escaped() {
        assert_eq!(quote_ident(Engine::Postgres, "us\"ers"), "\"us\"\"ers\"");
        assert_eq!(quote_ident(Engine::Mysql, "a`b"), "`a``b`");
        assert_eq!(
            quote_ident(Engine::Sqlite, "x; DROP TABLE y"),
            "\"x; DROP TABLE y\""
        );
    }
}

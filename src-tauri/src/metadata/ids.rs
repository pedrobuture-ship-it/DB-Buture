//! Deterministic, order-independent identifiers for schema objects.
//!
//! Format: `<engine>:<path>` for tables and `<engine>:<kind>:<path>` for every
//! other object, where path components are joined by `.` (namespace/name) or `:`
//! (parent/child). Components are escaped so hostile metadata can never forge a
//! separator.

use serde::{Deserialize, Serialize};
use std::fmt;

use super::model::Engine;

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct StableId(String);

impl StableId {
    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn table(engine: Engine, namespace: &str, name: &str) -> Self {
        Self(format!(
            "{}:{}.{}",
            engine.slug(),
            esc(namespace),
            esc(name)
        ))
    }

    pub fn namespace(engine: Engine, name: &str) -> Self {
        Self(format!("{}:namespace:{}", engine.slug(), esc(name)))
    }

    /// Namespaced top level object (view, index, sequence, ...).
    pub fn object(engine: Engine, kind: &str, namespace: &str, name: &str) -> Self {
        Self(format!(
            "{}:{}:{}.{}",
            engine.slug(),
            kind,
            esc(namespace),
            esc(name)
        ))
    }

    /// Object that lives under a table (column, constraint, trigger).
    pub fn child(engine: Engine, kind: &str, namespace: &str, parent: &str, name: &str) -> Self {
        Self(format!(
            "{}:{}:{}.{}:{}",
            engine.slug(),
            kind,
            esc(namespace),
            esc(parent),
            esc(name)
        ))
    }

    /// Routines can be overloaded, so the argument signature is part of identity.
    pub fn routine(engine: Engine, namespace: &str, name: &str, signature: &str) -> Self {
        Self(format!(
            "{}:routine:{}.{}({})",
            engine.slug(),
            esc(namespace),
            esc(name),
            esc(signature)
        ))
    }
}

impl fmt::Display for StableId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Percent-escape the characters that carry meaning in an id, plus control characters.
fn esc(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for ch in input.chars() {
        match ch {
            '%' | ':' | '.' | '(' | ')' | ',' => out.push_str(&format!("%{:02X}", ch as u32)),
            c if c.is_control() || c == '\u{200B}' || c == '\u{FEFF}' => {
                out.push_str(&format!("%u{:04X}", c as u32))
            }
            c => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_id_matches_documented_shape() {
        let id = StableId::table(Engine::Postgres, "public", "users");
        assert_eq!(id.as_str(), "postgres:public.users");
    }

    #[test]
    fn child_and_object_ids() {
        assert_eq!(
            StableId::child(Engine::Postgres, "column", "public", "users", "id").as_str(),
            "postgres:column:public.users:id"
        );
        assert_eq!(
            StableId::object(Engine::Postgres, "index", "public", "idx_users_email").as_str(),
            "postgres:index:public.idx_users_email"
        );
    }

    #[test]
    fn ids_are_deterministic() {
        let a = StableId::table(Engine::Mysql, "shop", "orders");
        let b = StableId::table(Engine::Mysql, "shop", "orders");
        assert_eq!(a, b);
    }

    #[test]
    fn hostile_names_cannot_forge_separators() {
        let forged = StableId::table(Engine::Postgres, "public", "users:column:x");
        let real = StableId::child(Engine::Postgres, "column", "public", "users", "x");
        assert_ne!(forged, real);
        assert!(!forged.as_str()["postgres:".len()..].contains(':'));

        let dotted = StableId::table(Engine::Postgres, "a.b", "c");
        let other = StableId::table(Engine::Postgres, "a", "b.c");
        assert_ne!(dotted, other);
    }

    #[test]
    fn control_and_invisible_chars_are_escaped() {
        let id = StableId::table(Engine::Sqlite, "main", "a\u{0}b\u{200B}c\n");
        assert!(id
            .as_str()
            .chars()
            .all(|c| !c.is_control() && c != '\u{200B}'));
    }

    #[test]
    fn unicode_names_pass_through() {
        let id = StableId::table(Engine::Postgres, "público", "usuários");
        assert_eq!(id.as_str(), "postgres:público.usuários");
    }

    #[test]
    fn routine_overloads_are_distinct() {
        let a = StableId::routine(Engine::Postgres, "public", "f", "integer");
        let b = StableId::routine(Engine::Postgres, "public", "f", "text");
        assert_ne!(a, b);
    }
}

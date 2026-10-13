//! Exports of the normalized schema. They are built only from the schema model, which by
//! construction holds no credentials; connection details are never part of an export.

use std::fmt::Write;

use crate::analysis::snapshot::canonical_json;
use crate::metadata::*;

pub const FORMAT_VERSION: u32 = 1;

pub fn schema_to_json(schema: &DatabaseSchema) -> String {
    // canonical_json is compact and key-sorted; re-pretty-print for humans.
    let v: serde_json::Value =
        serde_json::from_str(&canonical_json(schema)).unwrap_or(serde_json::Value::Null);
    let wrapped =
        serde_json::json!({ "format": "db-buture-schema", "version": FORMAT_VERSION, "schema": v });
    serde_json::to_string_pretty(&wrapped).unwrap_or_default()
}

fn cell(s: &str) -> String {
    s.replace('|', "\\|").replace('\n', " ")
}

pub fn schema_to_markdown(schema: &DatabaseSchema) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "# {}\n", schema.database);
    let _ = writeln!(
        out,
        "Engine: {:?}{}  \nTables: {} · Views: {} · Routines: {}\n",
        schema.engine,
        schema
            .server_version
            .as_deref()
            .map(|v| format!(" {v}"))
            .unwrap_or_default(),
        schema.tables.len(),
        schema.views.len(),
        schema.routines.len()
    );
    let name_of = |id: &StableId| -> String {
        schema
            .tables
            .iter()
            .find(|t| &t.stable_id == id)
            .map(|t| format!("{}.{}", t.namespace, t.name))
            .unwrap_or_else(|| id.to_string())
    };
    for t in &schema.tables {
        let _ = writeln!(out, "## {}.{}\n", t.namespace, t.name);
        if let Some(c) = &t.comment {
            let _ = writeln!(out, "{c}\n");
        }
        let _ = writeln!(
            out,
            "| Column | Type | Null | Default | Key | Comment |\n|---|---|---|---|---|---|"
        );
        for c in &t.columns {
            let mut keys = vec![];
            if t.primary_key
                .as_ref()
                .is_some_and(|p| p.columns.contains(&c.name))
            {
                keys.push("PK");
            }
            if t.foreign_keys
                .iter()
                .any(|f| f.source_columns.contains(&c.name))
            {
                keys.push("FK");
            }
            if t.unique_constraints
                .iter()
                .any(|u| u.columns.len() == 1 && u.columns[0] == c.name)
            {
                keys.push("UQ");
            }
            let _ = writeln!(
                out,
                "| `{}` | `{}` | {} | {} | {} | {} |",
                cell(&c.name),
                cell(&c.native_type),
                if c.nullable { "yes" } else { "no" },
                c.default
                    .as_deref()
                    .map(|d| format!("`{}`", cell(d)))
                    .unwrap_or_default(),
                keys.join(", "),
                cell(c.comment.as_deref().unwrap_or(""))
            );
        }
        if !t.foreign_keys.is_empty() {
            let _ = writeln!(out, "\nForeign keys:\n");
            for f in &t.foreign_keys {
                let _ = writeln!(
                    out,
                    "- `{}` ({}) → {} ({})",
                    f.name,
                    f.source_columns.join(", "),
                    name_of(&f.target_table),
                    f.target_columns.join(", ")
                );
            }
        }
        if !t.indexes.is_empty() {
            let _ = writeln!(out, "\nIndexes:\n");
            for i in &t.indexes {
                let cols: Vec<String> = i
                    .columns
                    .iter()
                    .cloned()
                    .chain(i.expressions.iter().map(|e| format!("({e})")))
                    .collect();
                let _ = writeln!(
                    out,
                    "- `{}`{} ({})",
                    i.name,
                    if i.unique { " unique" } else { "" },
                    cols.join(", ")
                );
            }
        }
        let _ = writeln!(out);
    }
    if !schema.views.is_empty() {
        let _ = writeln!(out, "## Views\n");
        for v in &schema.views {
            let _ = writeln!(
                out,
                "- `{}.{}`{}",
                v.namespace,
                v.name,
                if v.materialized {
                    " (materialized)"
                } else {
                    ""
                }
            );
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::test_support::sample;

    #[test]
    fn json_export_is_wrapped_and_has_no_statistics() {
        let mut s = sample();
        s.tables[0].estimated_rows = Some(123);
        let j = schema_to_json(&s);
        assert!(j.contains("\"format\": \"db-buture-schema\""));
        assert!(!j.contains("estimatedRows"));
        let v: serde_json::Value = serde_json::from_str(&j).unwrap();
        assert_eq!(v["schema"]["tables"].as_array().unwrap().len(), 4);
    }

    #[test]
    fn markdown_lists_tables_and_escapes_pipes() {
        let mut s = sample();
        s.tables[0].comment = Some("a | b".into());
        let md = schema_to_markdown(&s);
        assert!(md.starts_with("# app"));
        assert!(md.contains("## public.users"));
        assert!(md.contains("## Views"));
        assert!(md.contains("`public.v_orders`"));
    }

    #[test]
    fn exports_never_contain_secret_like_fields() {
        let s = sample();
        for text in [schema_to_json(&s), schema_to_markdown(&s)] {
            let lower = text.to_lowercase();
            assert!(!lower.contains("password"));
            assert!(!lower.contains("secret"));
        }
    }
}

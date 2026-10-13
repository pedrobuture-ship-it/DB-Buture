//! Maps engine specific type names onto a small common vocabulary.

use super::model::Engine;

/// Returns the normalized type name, e.g. `character varying(255)` -> `varchar`.
/// Unknown types fall back to the lowercase base name so nothing is lost.
pub fn normalize_type(engine: Engine, native: &str) -> String {
    let lower = native.trim().to_lowercase();
    let is_array = lower.ends_with("[]");
    let base = lower.trim_end_matches("[]");
    let base = base.split('(').next().unwrap_or(base).trim();
    let base = base.strip_suffix(" unsigned").unwrap_or(base).trim();

    let mapped = match (engine, base) {
        (_, "int" | "int4" | "integer" | "serial" | "mediumint") => "integer",
        (_, "int8" | "bigint" | "bigserial") => "bigint",
        (_, "int2" | "smallint" | "smallserial" | "tinyint") => "smallint",
        (_, "character varying" | "varchar" | "nvarchar") => "varchar",
        (_, "character" | "char" | "bpchar" | "nchar") => "char",
        (_, "text" | "tinytext" | "mediumtext" | "longtext" | "clob") => "text",
        (_, "bool" | "boolean") => "boolean",
        (_, "numeric" | "decimal") => "numeric",
        (_, "float4" | "real" | "float") => "real",
        (_, "float8" | "double precision" | "double") => "double",
        (_, "timestamp without time zone" | "timestamp" | "datetime") => "timestamp",
        (_, "timestamp with time zone" | "timestamptz") => "timestamptz",
        (_, "bytea" | "blob" | "longblob" | "mediumblob" | "tinyblob" | "varbinary" | "binary") => "binary",
        (_, "json" | "jsonb") => base,
        (_, "uuid") => "uuid",
        (_, "date") => "date",
        (_, "time without time zone" | "time") => "time",
        _ => base,
    };
    if is_array {
        format!("{mapped}[]")
    } else {
        mapped.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn postgres_types() {
        assert_eq!(normalize_type(Engine::Postgres, "character varying(255)"), "varchar");
        assert_eq!(normalize_type(Engine::Postgres, "integer"), "integer");
        assert_eq!(normalize_type(Engine::Postgres, "timestamp with time zone"), "timestamptz");
        assert_eq!(normalize_type(Engine::Postgres, "numeric(10,2)"), "numeric");
        assert_eq!(normalize_type(Engine::Postgres, "text[]"), "text[]");
    }

    #[test]
    fn mysql_types() {
        assert_eq!(normalize_type(Engine::Mysql, "int unsigned"), "integer");
        assert_eq!(normalize_type(Engine::Mysql, "varchar(40)"), "varchar");
        assert_eq!(normalize_type(Engine::Mysql, "datetime"), "timestamp");
    }

    #[test]
    fn unknown_types_are_preserved() {
        assert_eq!(normalize_type(Engine::Postgres, "geometry(Point,4326)"), "geometry");
        assert_eq!(normalize_type(Engine::Postgres, "citext"), "citext");
    }
}

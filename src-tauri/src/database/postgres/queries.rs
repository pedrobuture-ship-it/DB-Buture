//! Catalog queries. Every query is static SQL; user supplied values are bound.
//! `$1` is `include_system` (show pg_catalog / information_schema / pg_toast).

pub const VERSION: &str = "SHOW server_version";

pub const DATABASE: &str = "SELECT current_database()::text";

pub const NAMESPACES: &str = r#"
SELECT n.nspname::text AS name,
       (n.nspname = 'information_schema' OR n.nspname LIKE 'pg\_%') AS is_system
FROM pg_namespace n
WHERE $1::bool OR (n.nspname <> 'information_schema' AND n.nspname NOT LIKE 'pg\_%')
ORDER BY 1
"#;

pub const RELATIONS: &str = r#"
SELECT n.nspname::text AS namespace,
       c.relname::text AS name,
       c.relkind::text AS kind,
       obj_description(c.oid, 'pg_class') AS comment,
       CASE WHEN c.relkind IN ('r','p','m') THEN c.reltuples::bigint END AS estimated_rows,
       CASE WHEN c.relkind IN ('r','p','m') THEN pg_total_relation_size(c.oid) END AS size_bytes,
       CASE WHEN c.relkind IN ('v','m') THEN pg_get_viewdef(c.oid, true) END AS view_definition,
       c.relispartition AS is_partition
FROM pg_class c
JOIN pg_namespace n ON n.oid = c.relnamespace
WHERE c.relkind IN ('r','p','v','m')
  AND ($1::bool OR (n.nspname <> 'information_schema' AND n.nspname NOT LIKE 'pg\_%'))
ORDER BY 1, 2
"#;

pub const COLUMNS: &str = r#"
SELECT n.nspname::text AS namespace,
       c.relname::text AS relation,
       a.attname::text AS name,
       a.attnum::int AS ordinal,
       format_type(a.atttypid, a.atttypmod) AS native_type,
       a.attnotnull AS not_null,
       CASE WHEN a.atthasdef AND a.attgenerated = '' THEN pg_get_expr(d.adbin, d.adrelid) END AS "default",
       NULLIF(a.attidentity::text, '') AS identity,
       NULLIF(a.attgenerated::text, '') AS generated,
       col_description(c.oid, a.attnum) AS comment
FROM pg_attribute a
JOIN pg_class c ON c.oid = a.attrelid
JOIN pg_namespace n ON n.oid = c.relnamespace
LEFT JOIN pg_attrdef d ON d.adrelid = a.attrelid AND d.adnum = a.attnum
WHERE a.attnum > 0 AND NOT a.attisdropped
  AND c.relkind IN ('r','p','v','m')
  AND ($1::bool OR (n.nspname <> 'information_schema' AND n.nspname NOT LIKE 'pg\_%'))
ORDER BY 1, 2, 4
"#;

pub const CONSTRAINTS: &str = r#"
SELECT n.nspname::text AS namespace,
       c.relname::text AS relation,
       con.conname::text AS name,
       con.contype::text AS kind,
       ARRAY(SELECT a.attname::text
             FROM unnest(con.conkey) WITH ORDINALITY k(attnum, ord)
             JOIN pg_attribute a ON a.attrelid = con.conrelid AND a.attnum = k.attnum
             ORDER BY k.ord) AS columns,
       rn.nspname::text AS ref_namespace,
       rc.relname::text AS ref_relation,
       CASE WHEN con.contype = 'f' THEN ARRAY(
             SELECT a.attname::text
             FROM unnest(con.confkey) WITH ORDINALITY k(attnum, ord)
             JOIN pg_attribute a ON a.attrelid = con.confrelid AND a.attnum = k.attnum
             ORDER BY k.ord) END AS ref_columns,
       CASE WHEN con.contype = 'f' THEN con.confupdtype::text END AS on_update,
       CASE WHEN con.contype = 'f' THEN con.confdeltype::text END AS on_delete,
       pg_get_constraintdef(con.oid, true) AS definition,
       con.condeferrable AS deferrable,
       con.convalidated AS validated
FROM pg_constraint con
JOIN pg_class c ON c.oid = con.conrelid
JOIN pg_namespace n ON n.oid = c.relnamespace
LEFT JOIN pg_class rc ON rc.oid = con.confrelid
LEFT JOIN pg_namespace rn ON rn.oid = rc.relnamespace
WHERE con.contype IN ('p','f','u','c')
  AND c.relkind IN ('r','p')
  AND ($1::bool OR (n.nspname <> 'information_schema' AND n.nspname NOT LIKE 'pg\_%'))
ORDER BY 1, 2, 3
"#;

pub const INDEXES: &str = r#"
SELECT n.nspname::text AS namespace,
       t.relname::text AS "table",
       i.relname::text AS name,
       am.amname::text AS method,
       x.indisunique AS is_unique,
       x.indisprimary AS is_primary,
       x.indisvalid AS is_valid,
       ARRAY(SELECT CASE WHEN (string_to_array(x.indkey::text, ' '))[k]::int = 0
                         THEN pg_get_indexdef(x.indexrelid, k, true)
                         ELSE (SELECT a.attname::text FROM pg_attribute a WHERE a.attrelid = x.indrelid
                               AND a.attnum = (string_to_array(x.indkey::text, ' '))[k]::int) END
             FROM generate_series(1, x.indnkeyatts::int) k ORDER BY k) AS key_entries,
       ARRAY(SELECT (string_to_array(x.indkey::text, ' '))[k]::int = 0
             FROM generate_series(1, x.indnkeyatts::int) k ORDER BY k) AS key_is_expr,
       ARRAY(SELECT (SELECT a.attname::text FROM pg_attribute a WHERE a.attrelid = x.indrelid
                     AND a.attnum = (string_to_array(x.indkey::text, ' '))[k]::int)
             FROM generate_series(x.indnkeyatts::int + 1, x.indnatts::int) k ORDER BY k) AS included,
       pg_get_expr(x.indpred, x.indrelid, true) AS predicate,
       pg_get_indexdef(x.indexrelid) AS definition
FROM pg_index x
JOIN pg_class i ON i.oid = x.indexrelid
JOIN pg_class t ON t.oid = x.indrelid
JOIN pg_namespace n ON n.oid = t.relnamespace
JOIN pg_am am ON am.oid = i.relam
WHERE t.relkind IN ('r','p','m')
  AND ($1::bool OR (n.nspname <> 'information_schema' AND n.nspname NOT LIKE 'pg\_%'))
ORDER BY 1, 2, 3
"#;

pub const VIEW_DEPENDENCIES: &str = r#"
SELECT DISTINCT
       vn.nspname::text AS view_namespace,
       vc.relname::text AS view_name,
       sn.nspname::text AS source_namespace,
       sc.relname::text AS source_name
FROM pg_depend d
JOIN pg_rewrite r ON r.oid = d.objid
JOIN pg_class vc ON vc.oid = r.ev_class
JOIN pg_namespace vn ON vn.oid = vc.relnamespace
JOIN pg_class sc ON sc.oid = d.refobjid
JOIN pg_namespace sn ON sn.oid = sc.relnamespace
WHERE d.classid = 'pg_rewrite'::regclass
  AND d.refclassid = 'pg_class'::regclass
  AND vc.oid <> sc.oid
  AND vc.relkind IN ('v','m')
  AND sc.relkind IN ('r','p','v','m')
  AND ($1::bool OR (vn.nspname <> 'information_schema' AND vn.nspname NOT LIKE 'pg\_%'))
ORDER BY 1, 2, 3, 4
"#;

pub const TRIGGERS: &str = r#"
SELECT n.nspname::text AS namespace,
       c.relname::text AS "table",
       t.tgname::text AS name,
       CASE WHEN t.tgtype & 64 <> 0 THEN 'INSTEAD OF'
            WHEN t.tgtype & 2 <> 0 THEN 'BEFORE' ELSE 'AFTER' END AS timing,
       ARRAY_REMOVE(ARRAY[
         CASE WHEN t.tgtype & 4 <> 0 THEN 'INSERT' END,
         CASE WHEN t.tgtype & 8 <> 0 THEN 'DELETE' END,
         CASE WHEN t.tgtype & 16 <> 0 THEN 'UPDATE' END,
         CASE WHEN t.tgtype & 32 <> 0 THEN 'TRUNCATE' END], NULL) AS events,
       pg_get_triggerdef(t.oid, true) AS definition,
       fn.nspname::text AS function_namespace,
       p.proname::text AS function_name,
       pg_get_function_identity_arguments(p.oid) AS function_args
FROM pg_trigger t
JOIN pg_class c ON c.oid = t.tgrelid
JOIN pg_namespace n ON n.oid = c.relnamespace
JOIN pg_proc p ON p.oid = t.tgfoid
JOIN pg_namespace fn ON fn.oid = p.pronamespace
WHERE NOT t.tgisinternal
  AND ($1::bool OR (n.nspname <> 'information_schema' AND n.nspname NOT LIKE 'pg\_%'))
ORDER BY 1, 2, 3
"#;

pub const SEQUENCES: &str = r#"
SELECT n.nspname::text AS namespace,
       c.relname::text AS name,
       format_type(s.seqtypid, NULL) AS data_type,
       tn.nspname::text AS owner_namespace,
       tc.relname::text AS owner_table,
       ta.attname::text AS owner_column
FROM pg_class c
JOIN pg_namespace n ON n.oid = c.relnamespace
JOIN pg_sequence s ON s.seqrelid = c.oid
LEFT JOIN pg_depend d ON d.classid = 'pg_class'::regclass AND d.objid = c.oid
     AND d.refclassid = 'pg_class'::regclass AND d.deptype IN ('a','i') AND d.refobjsubid > 0
LEFT JOIN pg_class tc ON tc.oid = d.refobjid
LEFT JOIN pg_namespace tn ON tn.oid = tc.relnamespace
LEFT JOIN pg_attribute ta ON ta.attrelid = d.refobjid AND ta.attnum = d.refobjsubid
WHERE c.relkind = 'S'
  AND ($1::bool OR (n.nspname <> 'information_schema' AND n.nspname NOT LIKE 'pg\_%'))
ORDER BY 1, 2
"#;

pub const ROUTINES: &str = r#"
SELECT n.nspname::text AS namespace,
       p.proname::text AS name,
       (p.prokind = 'p') AS is_procedure,
       l.lanname::text AS language,
       pg_get_function_identity_arguments(p.oid) AS arguments,
       CASE WHEN p.prokind = 'f' THEN pg_get_function_result(p.oid) END AS return_type,
       pg_get_functiondef(p.oid) AS definition
FROM pg_proc p
JOIN pg_namespace n ON n.oid = p.pronamespace
JOIN pg_language l ON l.oid = p.prolang
WHERE p.prokind IN ('f','p')
  AND NOT EXISTS (SELECT 1 FROM pg_depend d WHERE d.classid = 'pg_proc'::regclass
                  AND d.objid = p.oid AND d.deptype = 'e')
  AND ($1::bool OR (n.nspname <> 'information_schema' AND n.nspname NOT LIKE 'pg\_%'))
ORDER BY 1, 2, 5
"#;

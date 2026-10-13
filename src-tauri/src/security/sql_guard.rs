//! Static SQL classifier used by the SQL editor.
//!
//! This is a *second* line of defense. The primary barrier is the read-only connection
//! (`default_transaction_read_only`, `SET SESSION TRANSACTION READ ONLY`, SQLite opened
//! read-only) and, ideally, a database account without write privileges.

use sqlparser::ast::Statement;
use sqlparser::dialect::{Dialect, GenericDialect, MySqlDialect, PostgreSqlDialect, SQLiteDialect};
use sqlparser::keywords::Keyword;
use sqlparser::parser::Parser;
use sqlparser::tokenizer::{Token, Tokenizer};

use crate::error::{AppError, AppResult};
use crate::metadata::Engine;

/// Keywords that start or embed a mutation, even inside a CTE or subquery.
const DENIED_KEYWORDS: &[Keyword] = &[
    Keyword::INSERT,
    Keyword::UPDATE,
    Keyword::DELETE,
    Keyword::MERGE,
    Keyword::DROP,
    Keyword::ALTER,
    Keyword::CREATE,
    Keyword::TRUNCATE,
    Keyword::GRANT,
    Keyword::REVOKE,
    Keyword::COPY,
    Keyword::CALL,
    Keyword::INTO,
    Keyword::EXECUTE,
    Keyword::VACUUM,
    Keyword::ATTACH,
    Keyword::DETACH,
];

/// Functions with side effects (sequences, locks, backend control, large objects, file access).
const DENIED_FUNCTIONS: &[&str] = &[
    "nextval",
    "setval",
    "pg_terminate_backend",
    "pg_cancel_backend",
    "pg_reload_conf",
    "lo_import",
    "lo_export",
    "lo_unlink",
    "lo_create",
    "pg_advisory_lock",
    "pg_advisory_xact_lock",
    "pg_read_file",
    "pg_read_binary_file",
    "pg_ls_dir",
    "dblink",
    "dblink_exec",
    "load_file",
    "sleep",
    "get_lock",
    "release_lock",
    "load_extension",
    "readfile",
    "writefile",
];

fn dialect(engine: Engine) -> Box<dyn Dialect> {
    match engine {
        Engine::Postgres => Box::new(PostgreSqlDialect {}),
        Engine::Mysql => Box::new(MySqlDialect {}),
        Engine::Sqlite => Box::new(SQLiteDialect {}),
        Engine::Generic => Box::new(GenericDialect {}),
    }
}

pub fn check_read_only(sql: &str, engine: Engine) -> AppResult<()> {
    let d = dialect(engine);
    if sql.trim().is_empty() {
        return Err(AppError::Validation("empty statement".into()));
    }

    let statements = Parser::parse_sql(d.as_ref(), sql)
        .map_err(|e| AppError::Validation(format!("could not parse SQL: {e}")))?;
    if statements.len() != 1 {
        return Err(AppError::ReadOnly(
            "only a single statement is allowed".into(),
        ));
    }

    match &statements[0] {
        Statement::Query(_) => {}
        Statement::Explain { analyze, .. } => {
            if *analyze {
                return Err(AppError::ReadOnly(
                    "EXPLAIN ANALYZE executes the statement and is blocked".into(),
                ));
            }
        }
        Statement::ExplainTable { .. }
        | Statement::ShowVariable { .. }
        | Statement::ShowVariables { .. }
        | Statement::ShowTables { .. }
        | Statement::ShowColumns { .. }
        | Statement::ShowCreate { .. }
        | Statement::ShowDatabases { .. }
        | Statement::ShowSchemas { .. }
        | Statement::ShowStatus { .. } => {}
        other => {
            return Err(AppError::ReadOnly(format!(
                "{} statements are blocked",
                statement_kind(other)
            )));
        }
    }

    // Token level deny list catches mutations hidden in CTEs/subqueries (e.g. WITH x AS (DELETE ...)).
    let tokens = Tokenizer::new(d.as_ref(), sql)
        .tokenize()
        .map_err(|e| AppError::Validation(format!("could not tokenize SQL: {e}")))?;
    for (i, tok) in tokens.iter().enumerate() {
        if let Token::Word(w) = tok {
            if w.quote_style.is_some() {
                continue;
            }
            if DENIED_KEYWORDS.contains(&w.keyword) {
                return Err(AppError::ReadOnly(format!(
                    "`{}` is not allowed in read-only mode",
                    w.value.to_uppercase()
                )));
            }
            let lower = w.value.to_lowercase();
            if DENIED_FUNCTIONS.contains(&lower.as_str()) && next_is_paren(&tokens, i) {
                return Err(AppError::ReadOnly(format!(
                    "function `{lower}` has side effects and is blocked"
                )));
            }
        }
    }
    Ok(())
}

fn next_is_paren(tokens: &[Token], i: usize) -> bool {
    tokens[i + 1..]
        .iter()
        .find(|t| !matches!(t, Token::Whitespace(_)))
        .map(|t| matches!(t, Token::LParen))
        .unwrap_or(false)
}

fn statement_kind(s: &Statement) -> String {
    let dbg = format!("{s:?}");
    dbg.split(|c: char| !c.is_alphanumeric())
        .next()
        .unwrap_or("this kind of")
        .to_uppercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ok(sql: &str) {
        check_read_only(sql, Engine::Postgres).unwrap_or_else(|e| panic!("{sql}: {e}"));
    }
    fn blocked(sql: &str) {
        assert!(
            check_read_only(sql, Engine::Postgres).is_err(),
            "should block: {sql}"
        );
    }

    #[test]
    fn allows_plain_reads() {
        ok("SELECT * FROM users WHERE id = 1");
        ok("select count(*) from orders o join users u on u.id = o.user_id");
        ok("WITH t AS (SELECT 1) SELECT * FROM t");
        ok("SELECT replace(name, 'a', 'b') FROM users");
        ok("SELECT \"update\" FROM \"delete\"");
        ok("EXPLAIN SELECT 1");
        ok("SELECT 'DROP TABLE users' AS text");
        ok("SELECT * FROM users -- DELETE FROM users");
    }

    #[test]
    fn blocks_mutations() {
        blocked("INSERT INTO users (id) VALUES (1)");
        blocked("UPDATE users SET name = 'x'");
        blocked("DELETE FROM users");
        blocked("DROP TABLE users");
        blocked("TRUNCATE users");
        blocked("ALTER TABLE users ADD COLUMN x int");
        blocked("CREATE INDEX i ON users (id)");
        blocked("GRANT ALL ON users TO bob");
    }

    #[test]
    fn blocks_hidden_mutations() {
        blocked("WITH d AS (DELETE FROM users RETURNING *) SELECT * FROM d");
        blocked("SELECT * INTO backup FROM users");
        blocked("SELECT * FROM users FOR UPDATE");
        blocked("SELECT * FROM (WITH d AS (UPDATE users SET a = 1 RETURNING *) SELECT * FROM d) x");
    }

    #[test]
    fn blocks_multiple_statements() {
        blocked("SELECT 1; DROP TABLE users");
        blocked("SELECT 1; SELECT 2");
    }

    #[test]
    fn blocks_side_effect_functions() {
        blocked("SELECT nextval('s')");
        blocked("SELECT pg_terminate_backend(123)");
        blocked("SELECT lo_import('/etc/passwd')");
        ok("SELECT nextval_count FROM stats");
    }

    #[test]
    fn explain_analyze_is_blocked() {
        blocked("EXPLAIN ANALYZE SELECT 1");
    }

    #[test]
    fn unicode_and_garbage_do_not_panic() {
        blocked("");
        blocked("   ");
        blocked("SELECT 'é' ; \u{0} DROP");
        let _ = check_read_only("SELECT ' unterminated", Engine::Postgres);
        let _ = check_read_only("😀😀😀", Engine::Sqlite);
    }
}

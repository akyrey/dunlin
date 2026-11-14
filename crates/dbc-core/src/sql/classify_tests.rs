use rstest::rstest;

use super::*;

fn rules() -> SplitterRules {
    SplitterRules {
        dollar_quoting: true,
        nested_block_comments: true,
        ..SplitterRules::default()
    }
}

fn class_of(sql: &str) -> StatementClass {
    classify(sql, rules()).class
}

fn danger_of(sql: &str) -> Option<Danger> {
    classify(sql, rules()).danger
}

#[rstest]
#[case::select("SELECT * FROM t", StatementClass::Read)]
#[case::lowercase("select 1", StatementClass::Read)]
#[case::leading_whitespace_and_comments("  -- hi\n /* x */ SELECT 1", StatementClass::Read)]
#[case::parenthesised_select("(SELECT 1) UNION (SELECT 2)", StatementClass::Read)]
#[case::values("VALUES (1), (2)", StatementClass::Read)]
#[case::show("SHOW TABLES", StatementClass::Read)]
#[case::describe("DESCRIBE t", StatementClass::Read)]
#[case::select_for_update("SELECT * FROM t FOR UPDATE", StatementClass::Read)]
#[case::select_replace_function("SELECT replace(a, 'x', 'y') FROM t", StatementClass::Read)]
#[case::select_into_creates_table("SELECT * INTO new_t FROM t", StatementClass::Write)]
#[case::cte_select("WITH x AS (SELECT 1) SELECT * FROM x", StatementClass::Read)]
#[case::cte_delete(
    "WITH x AS (SELECT 1) DELETE FROM t WHERE id IN (SELECT * FROM x)",
    StatementClass::Write
)]
#[case::cte_with_modifying_body(
    "WITH d AS (DELETE FROM t WHERE a = 1 RETURNING *) SELECT * FROM d",
    StatementClass::Write
)]
#[case::recursive_cte(
    "WITH RECURSIVE r(n) AS (SELECT 1 UNION ALL SELECT n + 1 FROM r) SELECT n FROM r",
    StatementClass::Read
)]
#[case::insert("INSERT INTO t VALUES (1)", StatementClass::Write)]
#[case::update("UPDATE t SET a = 1 WHERE id = 2", StatementClass::Write)]
#[case::delete("DELETE FROM t WHERE id = 1", StatementClass::Write)]
#[case::merge(
    "MERGE INTO t USING s ON t.id = s.id WHEN MATCHED THEN DELETE",
    StatementClass::Write
)]
#[case::replace("REPLACE INTO t VALUES (1)", StatementClass::Write)]
#[case::create("CREATE TABLE t (a int)", StatementClass::Ddl)]
#[case::alter("ALTER TABLE t ADD COLUMN b int", StatementClass::Ddl)]
#[case::drop("DROP TABLE t", StatementClass::Ddl)]
#[case::truncate("TRUNCATE t", StatementClass::Ddl)]
#[case::begin("BEGIN", StatementClass::Transaction)]
#[case::start_transaction("START TRANSACTION", StatementClass::Transaction)]
#[case::commit("COMMIT", StatementClass::Transaction)]
#[case::rollback("ROLLBACK TO SAVEPOINT a", StatementClass::Transaction)]
#[case::set("SET search_path = public", StatementClass::Session)]
#[case::use_database("USE shop", StatementClass::Session)]
#[case::explain("EXPLAIN SELECT 1", StatementClass::Read)]
#[case::explain_query_plan("EXPLAIN QUERY PLAN DELETE FROM t", StatementClass::Read)]
#[case::explain_analyze_runs_the_statement(
    "EXPLAIN ANALYZE DELETE FROM t WHERE a = 1",
    StatementClass::Write
)]
#[case::explain_analyze_option_list(
    "EXPLAIN (ANALYZE, BUFFERS) UPDATE t SET a = 1 WHERE b = 2",
    StatementClass::Write
)]
#[case::explain_analyze_select("EXPLAIN ANALYZE SELECT 1", StatementClass::Read)]
#[case::pragma_read("PRAGMA table_info(t)", StatementClass::Read)]
#[case::pragma_write("PRAGMA foreign_keys = OFF", StatementClass::Session)]
#[case::unknown("FROBNICATE everything", StatementClass::Unknown)]
#[case::empty("", StatementClass::Unknown)]
#[case::only_comments("-- nothing here", StatementClass::Unknown)]
#[case::starts_with_string("'SELECT'", StatementClass::Unknown)]
#[case::set_read_write_escapes_read_only(
    "SET SESSION CHARACTERISTICS AS TRANSACTION READ WRITE",
    StatementClass::Write
)]
#[case::set_read_only_variable("SET default_transaction_read_only = off", StatementClass::Write)]
#[case::begin_read_write("BEGIN READ WRITE", StatementClass::Write)]
#[case::reset_all("RESET ALL", StatementClass::Write)]
fn classifies_statements(#[case] sql: &str, #[case] expected: StatementClass) {
    assert_eq!(class_of(sql), expected);
}

#[rstest]
#[case::update_without_where("UPDATE t SET a = 1", Some(Danger::UpdateWithoutWhere))]
#[case::delete_without_where("DELETE FROM t", Some(Danger::DeleteWithoutWhere))]
#[case::update_with_where("UPDATE t SET a = 1 WHERE id = 1", None)]
#[case::delete_with_where("DELETE FROM t WHERE id = 1", None)]
#[case::where_only_in_string("DELETE FROM t -- WHERE\n", Some(Danger::DeleteWithoutWhere))]
#[case::where_inside_string_literal(
    "UPDATE t SET note = 'WHERE id = 1'",
    Some(Danger::UpdateWithoutWhere)
)]
#[case::where_inside_quoted_identifier(
    "UPDATE t SET \"where\" = 1",
    Some(Danger::UpdateWithoutWhere)
)]
#[case::where_only_in_subquery(
    "UPDATE t SET a = (SELECT max(b) FROM u WHERE u.id = 1)",
    Some(Danger::UpdateWithoutWhere)
)]
#[case::where_only_in_from_subquery(
    "DELETE FROM t USING (SELECT id FROM u WHERE x) s",
    Some(Danger::DeleteWithoutWhere)
)]
#[case::cte_main_delete_without_where(
    "WITH x AS (SELECT 1 WHERE true) DELETE FROM t",
    Some(Danger::DeleteWithoutWhere)
)]
#[case::cte_body_delete_without_where(
    "WITH d AS (DELETE FROM t RETURNING *) SELECT * FROM d WHERE true",
    Some(Danger::DeleteWithoutWhere)
)]
#[case::cte_body_delete_with_where(
    "WITH d AS (DELETE FROM t WHERE a = 1 RETURNING *) SELECT * FROM d",
    None
)]
#[case::drop("DROP TABLE t", Some(Danger::Drop))]
#[case::drop_lowercase("drop index i", Some(Danger::Drop))]
#[case::truncate("TRUNCATE TABLE t", Some(Danger::Truncate))]
#[case::explain_analyze_delete("EXPLAIN ANALYZE DELETE FROM t", Some(Danger::DeleteWithoutWhere))]
#[case::explain_delete_is_harmless("EXPLAIN DELETE FROM t", None)]
#[case::upsert_do_update("INSERT INTO t VALUES (1) ON CONFLICT (id) DO UPDATE SET a = 1", None)]
#[case::on_duplicate_key_update("INSERT INTO t VALUES (1) ON DUPLICATE KEY UPDATE a = 1", None)]
#[case::select_for_update("SELECT * FROM t FOR UPDATE", None)]
#[case::trigger_body_is_ddl("CREATE TRIGGER g AFTER INSERT ON a BEGIN DELETE FROM b; END", None)]
#[case::alter_drop_column("ALTER TABLE t DROP COLUMN a", None)]
fn flags_dangerous_statements(#[case] sql: &str, #[case] expected: Option<Danger>) {
    assert_eq!(danger_of(sql), expected);
}

#[test]
fn danger_reason_is_human_readable() {
    assert_eq!(
        Danger::DeleteWithoutWhere.reason(),
        "DELETE without WHERE affects every row"
    );
}

#[test]
fn read_only_allows_only_reads_sessions_and_transactions() {
    let allowed = [
        StatementClass::Read,
        StatementClass::Session,
        StatementClass::Transaction,
    ];
    let rejected = [
        StatementClass::Write,
        StatementClass::Ddl,
        StatementClass::Unknown,
    ];

    assert!(allowed.iter().all(|c| c.allowed_on_read_only()));
    assert!(rejected.iter().all(|c| !c.allowed_on_read_only()));
}

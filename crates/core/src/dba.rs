//! DBA 리포트: 세션·락, 느린 쿼리, 용량·인덱스·bloat, 유지보수·VACUUM 진행, 복제·서버, 권한. 쿼리 결과를 표로 보이고 몇 가지 동작(세션 종료, SQL 만들기)을 붙인다.
//! GUI·TUI 는 같은 리포트 목록을 그대로 보여 준다
use crate::app::{AppState, ensure_pool};
use crate::db::QueryResult;
use crate::error::{AppError, AppResult};
use crate::store::DbKind;
use std::collections::HashMap;

/// 리포트 행에 붙는 동작
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum Action {
    /// 세션 종료 (terminate = 연결을 끊는다, 아니면 지금 쿼리만 취소). pid 열을 쓴다
    Kill { label: &'static str, terminate: bool },
    /// 콘솔에 SQL 을 넣는다. {col} = 그 열 값을 식별자로 (따옴표), {=col} = 그대로
    Sql { label: &'static str, template: &'static str },
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReportInfo {
    pub id: &'static str,
    pub title: &'static str,
    pub group: &'static str,
    /// 무엇을 보는지, 어떻게 읽는지
    pub help: &'static str,
    pub actions: Vec<Action>,
}

/// 리포트 하나의 SQL. 앞에서부터 해 보고 되는 것을 쓴다 (버전·권한에 따라 뷰가 다르다)
struct Def {
    info: fn() -> ReportInfo,
    pg: &'static [&'static str],
    my: &'static [&'static str],
}

const KILL: [Action; 2] = [Action::Kill { label: "Cancel Query", terminate: false }, Action::Kill { label: "Terminate Session", terminate: true }];

fn info(id: &'static str, title: &'static str, group: &'static str, help: &'static str, actions: Vec<Action>) -> ReportInfo {
    ReportInfo { id, title, group, help, actions }
}

const DEFS: &[Def] = &[
    Def {
        info: || {
            let mut a = KILL.to_vec();
            a.push(Action::Sql { label: "Open Query in Console", template: "{=query}" });
            info("sessions", "Sessions", "Activity", "Connections and what they run. blocking = how many sessions wait for this one; blocked_by = whom this one waits for. Long query_sec / xact_sec are the usual suspects.", a)
        },
        pg: &[r#"SELECT * FROM (SELECT a.pid, a.usename AS "user", a.datname AS db, a.application_name AS app, a.client_addr::text AS client, a.state,
       a.wait_event_type || coalesce(':' || a.wait_event, '') AS wait,
       (SELECT count(*) FROM pg_stat_activity b WHERE a.pid = ANY(pg_blocking_pids(b.pid))) AS blocking,
       array_to_string(pg_blocking_pids(a.pid), ',') AS blocked_by,
       round(extract(epoch FROM now() - a.query_start)::numeric, 1) AS query_sec,
       round(extract(epoch FROM now() - a.xact_start)::numeric, 1) AS xact_sec,
       left(a.query, 4000) AS query
FROM pg_stat_activity a
WHERE a.backend_type = 'client backend' AND a.pid <> pg_backend_pid()) s
ORDER BY blocking DESC, blocked_by <> '' DESC, state = 'idle', query_sec DESC NULLS LAST"#],
        my: &[
            r#"SELECT * FROM (SELECT p.ID AS pid, p.USER AS user, p.DB AS db, p.HOST AS client, p.COMMAND AS command, p.STATE AS state,
       (SELECT COUNT(*) FROM performance_schema.data_lock_waits w JOIN performance_schema.threads t ON t.THREAD_ID = w.BLOCKING_THREAD_ID WHERE t.PROCESSLIST_ID = p.ID) AS blocking,
       (SELECT GROUP_CONCAT(DISTINCT t2.PROCESSLIST_ID) FROM performance_schema.data_lock_waits w JOIN performance_schema.threads t1 ON t1.THREAD_ID = w.REQUESTING_THREAD_ID JOIN performance_schema.threads t2 ON t2.THREAD_ID = w.BLOCKING_THREAD_ID WHERE t1.PROCESSLIST_ID = p.ID) AS blocked_by,
       p.TIME AS query_sec, LEFT(p.INFO, 4000) AS query
FROM information_schema.PROCESSLIST p WHERE p.ID <> CONNECTION_ID()) s
ORDER BY blocking DESC, blocked_by IS NOT NULL DESC, command = 'Sleep', query_sec DESC"#,
            r#"SELECT p.ID AS pid, p.USER AS user, p.DB AS db, p.HOST AS client, p.COMMAND AS command, p.STATE AS state, p.TIME AS query_sec, LEFT(p.INFO, 4000) AS query
FROM information_schema.PROCESSLIST p WHERE p.ID <> CONNECTION_ID() ORDER BY p.COMMAND = 'Sleep', p.TIME DESC"#,
        ],
    },
    Def {
        info: || info("locks", "Lock Waits", "Activity", "Who waits for whom right now. Terminate the blocking session (or finish its transaction) to free the others.", vec![
            Action::Kill { label: "Terminate Blocking Session", terminate: true },
        ]),
        pg: &[r#"SELECT b.pid, b.usename AS blocking_user, b.state AS blocking_state, round(extract(epoch FROM now() - b.xact_start)::numeric, 1) AS blocking_xact_sec, left(b.query, 2000) AS blocking_query,
       a.pid AS waiting_pid, a.usename AS waiting_user, round(extract(epoch FROM now() - a.query_start)::numeric, 1) AS waiting_sec, left(a.query, 2000) AS waiting_query
FROM pg_stat_activity a CROSS JOIN LATERAL unnest(pg_blocking_pids(a.pid)) AS bp(pid) JOIN pg_stat_activity b ON b.pid = bp.pid
ORDER BY waiting_sec DESC"#],
        my: &[r#"SELECT blocking_pid AS pid, blocking_query, waiting_pid, wait_age_secs AS waiting_sec, waiting_query, locked_table, locked_index, locked_type FROM sys.innodb_lock_waits ORDER BY wait_age_secs DESC"#],
    },
    Def {
        info: || info("slow_queries", "Slow Queries (Top 100)", "Performance", "Statements by total time spent since the statistics were reset. High mean_ms = slow each time; high calls with modest mean = worth caching or batching.", vec![
            Action::Sql { label: "Open Query in Console", template: "{=query}" },
            Action::Sql { label: "Explain in Console", template: "EXPLAIN {=query}" },
        ]),
        pg: &[
            r#"SELECT round(s.total_exec_time::numeric, 1) AS total_ms, s.calls, round(s.mean_exec_time::numeric, 2) AS mean_ms, round(s.max_exec_time::numeric, 2) AS max_ms, s.rows,
       round(100 * s.shared_blks_hit::numeric / nullif(s.shared_blks_hit + s.shared_blks_read, 0), 1) AS cache_hit_pct, left(s.query, 4000) AS query
FROM pg_stat_statements s WHERE s.dbid = (SELECT oid FROM pg_database WHERE datname = current_database()) ORDER BY s.total_exec_time DESC LIMIT 100"#,
            r#"SELECT round(s.total_time::numeric, 1) AS total_ms, s.calls, round(s.mean_time::numeric, 2) AS mean_ms, round(s.max_time::numeric, 2) AS max_ms, s.rows, left(s.query, 4000) AS query
FROM pg_stat_statements s WHERE s.dbid = (SELECT oid FROM pg_database WHERE datname = current_database()) ORDER BY s.total_time DESC LIMIT 100"#,
        ],
        my: &[r#"SELECT ROUND(SUM_TIMER_WAIT / 1e9, 1) AS total_ms, COUNT_STAR AS calls, ROUND(AVG_TIMER_WAIT / 1e9, 2) AS mean_ms, ROUND(MAX_TIMER_WAIT / 1e9, 2) AS max_ms,
       SUM_ROWS_EXAMINED AS rows_examined, SUM_ROWS_SENT AS rows_sent, SUM_NO_INDEX_USED AS no_index_used, LEFT(DIGEST_TEXT, 4000) AS query
FROM performance_schema.events_statements_summary_by_digest WHERE DIGEST_TEXT IS NOT NULL AND (DATABASE() IS NULL OR SCHEMA_NAME = DATABASE())
ORDER BY SUM_TIMER_WAIT DESC LIMIT 100"#],
    },
    Def {
        info: || info("table_sizes", "Table Sizes", "Storage", "Biggest tables first: data, indexes and TOAST (large values) separately.", vec![]),
        pg: &[r#"SELECT n.nspname AS schema, c.relname AS "table", pg_size_pretty(pg_total_relation_size(c.oid)) AS total, pg_size_pretty(pg_relation_size(c.oid)) AS data,
       pg_size_pretty(pg_indexes_size(c.oid)) AS indexes, pg_size_pretty(greatest(pg_total_relation_size(c.oid) - pg_relation_size(c.oid) - pg_indexes_size(c.oid), 0)) AS toast,
       c.reltuples::bigint AS est_rows, pg_total_relation_size(c.oid) AS total_bytes
FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace
WHERE c.relkind IN ('r', 'p', 'm') AND n.nspname NOT IN ('pg_catalog', 'information_schema') AND n.nspname !~ '^pg_toast'
ORDER BY total_bytes DESC LIMIT 300"#],
        my: &[r#"SELECT TABLE_SCHEMA AS `schema`, TABLE_NAME AS `table`, ROUND((DATA_LENGTH + INDEX_LENGTH) / 1048576, 1) AS total_mb, ROUND(DATA_LENGTH / 1048576, 1) AS data_mb,
       ROUND(INDEX_LENGTH / 1048576, 1) AS index_mb, ROUND(DATA_FREE / 1048576, 1) AS free_mb, TABLE_ROWS AS est_rows, ENGINE AS engine
FROM information_schema.TABLES WHERE TABLE_TYPE = 'BASE TABLE' AND TABLE_SCHEMA NOT IN ('mysql', 'sys', 'performance_schema', 'information_schema')
ORDER BY DATA_LENGTH + INDEX_LENGTH DESC LIMIT 300"#],
    },
    // bloat: pg_stats 의 평균 행 크기로 있어야 할 페이지 수를 어림해 지금 페이지 수와 비교한다 (pgstattuple 없이). 통계가 없는 테이블은 뺀다
    Def {
        info: || info("table_bloat", "Table Bloat", "Storage", "Space held by dead or deleted rows (estimated from statistics, most accurate right after ANALYZE). Plain VACUUM makes it reusable; VACUUM FULL gives it back to the disk but locks the table while it rewrites it (pg_repack does it online).", vec![
            Action::Sql { label: "Generate VACUUM FULL", template: "VACUUM (FULL, ANALYZE) {schema}.{table};" },
        ]),
        pg: &[r#"SELECT schemaname AS schema, tblname AS "table", pg_size_pretty((bs * tblpages)::bigint) AS size,
       pg_size_pretty(greatest((tblpages - est_pages_ff) * bs, 0)::bigint) AS wasted,
       CASE WHEN tblpages > 0 AND tblpages > est_pages_ff THEN round(100 * (tblpages - est_pages_ff) / tblpages, 1) ELSE 0 END AS bloat_pct,
       fillfactor, greatest((tblpages - est_pages_ff) * bs, 0)::bigint AS wasted_bytes
FROM (
  SELECT ceil(reltuples / ((bs - page_hdr) * fillfactor / (tpl_size * 100))) + ceil(toasttuples / 4) AS est_pages_ff, tblpages, fillfactor, bs, schemaname, tblname
  FROM (
    SELECT (4 + tpl_hdr_size + tpl_data_size + (2 * ma)
            - CASE WHEN tpl_hdr_size % ma = 0 THEN ma ELSE tpl_hdr_size % ma END
            - CASE WHEN ceil(tpl_data_size)::int % ma = 0 THEN ma ELSE ceil(tpl_data_size)::int % ma END)::numeric AS tpl_size,
           (heappages + toastpages)::numeric AS tblpages, greatest(reltuples, 0)::numeric AS reltuples, toasttuples::numeric AS toasttuples, bs, page_hdr, schemaname, tblname, fillfactor, is_na
    FROM (
      SELECT ns.nspname AS schemaname, tbl.relname AS tblname, tbl.reltuples, tbl.relpages AS heappages, coalesce(toast.relpages, 0) AS toastpages, coalesce(toast.reltuples, 0) AS toasttuples,
             coalesce(substring(array_to_string(tbl.reloptions, ' ') FROM 'fillfactor=([0-9]+)')::int, 100) AS fillfactor,
             current_setting('block_size')::numeric AS bs, 8 AS ma, 24 AS page_hdr,
             23 + CASE WHEN max(coalesce(s.null_frac, 0)) > 0 THEN (7 + count(s.attname)::int) / 8 ELSE 0 END AS tpl_hdr_size,
             sum((1 - coalesce(s.null_frac, 0)) * coalesce(s.avg_width, 0)) AS tpl_data_size,
             bool_or(att.atttypid = 'pg_catalog.name'::regtype) OR sum(CASE WHEN att.attnum > 0 THEN 1 ELSE 0 END) <> count(s.attname) AS is_na
      FROM pg_attribute att
      JOIN pg_class tbl ON tbl.oid = att.attrelid
      JOIN pg_namespace ns ON ns.oid = tbl.relnamespace
      LEFT JOIN pg_stats s ON s.schemaname = ns.nspname AND s.tablename = tbl.relname AND s.attname = att.attname AND NOT s.inherited
      LEFT JOIN pg_class toast ON toast.oid = tbl.reltoastrelid
      WHERE NOT att.attisdropped AND att.attnum > 0 AND tbl.relkind IN ('r', 'm')
        AND ns.nspname NOT IN ('pg_catalog', 'information_schema') AND ns.nspname !~ '^pg_toast'
      GROUP BY 1, 2, 3, 4, 5, 6, 7, 8, 9, 10
    ) s
  ) s2
  WHERE NOT is_na
) s3
ORDER BY wasted_bytes DESC LIMIT 300"#],
        my: &[],
    },
    Def {
        info: || info("index_bloat", "Index Bloat", "Storage", "B-tree indexes bigger than their rows need (estimated from statistics). REINDEX CONCURRENTLY rebuilds one without blocking writes.", vec![
            Action::Sql { label: "Generate REINDEX", template: "REINDEX INDEX CONCURRENTLY {schema}.{index};" },
        ]),
        pg: &[r#"SELECT nspname AS schema, tblname AS "table", idxname AS "index", pg_size_pretty((bs * relpages)::bigint) AS size,
       pg_size_pretty(greatest((relpages - est_pages_ff) * bs, 0)::bigint) AS wasted,
       CASE WHEN relpages > est_pages_ff THEN round(100 * (relpages - est_pages_ff) / relpages, 1) ELSE 0 END AS bloat_pct,
       fillfactor, greatest((relpages - est_pages_ff) * bs, 0)::bigint AS wasted_bytes
FROM (
  SELECT coalesce(1 + ceil(reltuples / nullif(floor((bs - pageopqdata - pagehdr) * fillfactor / (100 * (4 + nulldatahdrwidth))), 0)), 0) AS est_pages_ff,
         bs, nspname, tblname, idxname, relpages::numeric AS relpages, fillfactor, is_na
  FROM (
    SELECT bs, nspname, tblname, idxname, greatest(reltuples, 0)::numeric AS reltuples, relpages, fillfactor, pagehdr, pageopqdata, is_na,
           (index_tuple_hdr_bm + maxalign - CASE WHEN index_tuple_hdr_bm % maxalign = 0 THEN maxalign ELSE index_tuple_hdr_bm % maxalign END
            + nulldatawidth + maxalign - CASE WHEN nulldatawidth = 0 THEN 0 WHEN nulldatawidth::int % maxalign = 0 THEN maxalign ELSE nulldatawidth::int % maxalign END)::numeric AS nulldatahdrwidth
    FROM (
      SELECT n.nspname, i.tblname, i.idxname, i.reltuples, i.relpages, i.idxoid, i.fillfactor, current_setting('block_size')::numeric AS bs, 8 AS maxalign, 24 AS pagehdr, 16 AS pageopqdata,
             CASE WHEN max(coalesce(s.null_frac, 0)) = 0 THEN 8 ELSE 8 + ((32 + 8 - 1) / 8) END AS index_tuple_hdr_bm,
             sum((1 - coalesce(s.null_frac, 0)) * coalesce(s.avg_width, 1024)) AS nulldatawidth,
             max(CASE WHEN i.atttypid = 'pg_catalog.name'::regtype THEN 1 ELSE 0 END) > 0 AS is_na
      FROM (
        SELECT ct.relname AS tblname, ct.relnamespace, ic.idxname, ic.reltuples, ic.relpages, ic.idxoid, ic.fillfactor,
               coalesce(a1.attname, a2.attname) AS attname, coalesce(a1.atttypid, a2.atttypid) AS atttypid,
               CASE WHEN a1.attnum IS NULL THEN ic.idxname ELSE ct.relname END AS attrelname
        FROM (
          SELECT ci.relname AS idxname, ci.reltuples, ci.relpages, i.indrelid AS tbloid, i.indexrelid AS idxoid,
                 coalesce(substring(array_to_string(ci.reloptions, ' ') FROM 'fillfactor=([0-9]+)')::int, 90) AS fillfactor,
                 string_to_array(i.indkey::text, ' ')::int[] AS indkey, generate_series(1, i.indnatts) AS attpos
          FROM pg_index i JOIN pg_class ci ON ci.oid = i.indexrelid
          WHERE ci.relam = (SELECT oid FROM pg_am WHERE amname = 'btree') AND ci.relpages > 0
        ) ic
        JOIN pg_class ct ON ct.oid = ic.tbloid
        LEFT JOIN pg_attribute a1 ON ic.indkey[ic.attpos] <> 0 AND a1.attrelid = ic.tbloid AND a1.attnum = ic.indkey[ic.attpos]
        LEFT JOIN pg_attribute a2 ON ic.indkey[ic.attpos] = 0 AND a2.attrelid = ic.idxoid AND a2.attnum = ic.attpos
      ) i
      JOIN pg_namespace n ON n.oid = i.relnamespace
      JOIN pg_stats s ON s.schemaname = n.nspname AND s.tablename = i.attrelname AND s.attname = i.attname AND NOT s.inherited
      WHERE n.nspname NOT IN ('pg_catalog', 'information_schema') AND n.nspname !~ '^pg_toast'
      GROUP BY 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11
    ) d
  ) h
  WHERE NOT is_na
) r
ORDER BY wasted_bytes DESC LIMIT 300"#],
        my: &[],
    },
    Def {
        info: || info("fragmentation", "Table Fragmentation", "Storage", "Free space inside each table's file (DATA_FREE), left behind by deletes and updates. OPTIMIZE TABLE rebuilds the table to give it back. Tables in the shared system tablespace all show that tablespace's free space.", vec![
            Action::Sql { label: "Generate OPTIMIZE TABLE", template: "OPTIMIZE TABLE {schema}.{table};" },
        ]),
        pg: &[],
        my: &[r#"SELECT TABLE_SCHEMA AS `schema`, TABLE_NAME AS `table`, ROUND((DATA_LENGTH + INDEX_LENGTH) / 1048576, 1) AS total_mb, ROUND(DATA_FREE / 1048576, 1) AS free_mb,
       ROUND(100 * DATA_FREE / NULLIF(DATA_LENGTH + INDEX_LENGTH + DATA_FREE, 0), 1) AS free_pct, ENGINE AS engine
FROM information_schema.TABLES WHERE TABLE_TYPE = 'BASE TABLE' AND DATA_FREE > 0 AND TABLE_SCHEMA NOT IN ('mysql', 'sys', 'performance_schema', 'information_schema')
ORDER BY DATA_FREE DESC LIMIT 300"#],
    },
    Def {
        info: || info("unused_indexes", "Unused Indexes", "Storage", "Indexes never used since statistics were reset (unique and primary keys are left out). They cost disk and slow down writes. Check replicas too before dropping.", vec![
            Action::Sql { label: "Generate DROP INDEX", template: "DROP INDEX CONCURRENTLY {schema}.{index};" },
        ]),
        pg: &[r#"SELECT s.schemaname AS schema, s.relname AS "table", s.indexrelname AS "index", s.idx_scan AS scans, pg_size_pretty(pg_relation_size(s.indexrelid)) AS size, pg_get_indexdef(s.indexrelid) AS definition
FROM pg_stat_user_indexes s JOIN pg_index i ON i.indexrelid = s.indexrelid
WHERE s.idx_scan = 0 AND NOT i.indisunique AND NOT i.indisprimary
ORDER BY pg_relation_size(s.indexrelid) DESC"#],
        my: &[
            r#"SELECT object_schema AS `schema`, object_name AS `table`, index_name AS `index` FROM sys.schema_unused_indexes ORDER BY object_schema, object_name"#,
            r#"SELECT OBJECT_SCHEMA AS `schema`, OBJECT_NAME AS `table`, INDEX_NAME AS `index` FROM performance_schema.table_io_waits_summary_by_index_usage
WHERE INDEX_NAME IS NOT NULL AND INDEX_NAME <> 'PRIMARY' AND COUNT_STAR = 0 AND OBJECT_SCHEMA NOT IN ('mysql', 'sys', 'performance_schema') ORDER BY 1, 2"#,
        ],
    },
    Def {
        info: || info("duplicate_indexes", "Duplicate Indexes", "Storage", "Indexes with the same columns, operators and conditions on one table. Keep one.", vec![]),
        pg: &[r#"SELECT n.nspname AS schema, ct.relname AS "table", string_agg(ci.relname, ', ' ORDER BY ci.relname) AS indexes, pg_size_pretty(sum(pg_relation_size(ci.oid))::bigint) AS total_size, min(pg_get_indexdef(ci.oid)) AS definition
FROM pg_index i JOIN pg_class ci ON ci.oid = i.indexrelid JOIN pg_class ct ON ct.oid = i.indrelid JOIN pg_namespace n ON n.oid = ct.relnamespace
WHERE n.nspname NOT IN ('pg_catalog', 'information_schema')
GROUP BY n.nspname, ct.relname, i.indrelid, i.indkey::text, i.indclass::text, coalesce(i.indexprs::text, ''), coalesce(i.indpred::text, '')
HAVING count(*) > 1 ORDER BY sum(pg_relation_size(ci.oid)) DESC"#],
        my: &[r#"SELECT table_schema AS `schema`, table_name AS `table`, redundant_index_name AS `index`, dominant_index_name AS covered_by, sql_drop_index FROM sys.schema_redundant_indexes"#],
    },
    Def {
        info: || info("fk_without_index", "Foreign Keys Without an Index", "Storage", "Deleting or updating a referenced row scans the whole child table when the foreign key columns have no index.", vec![
            Action::Sql { label: "Generate CREATE INDEX", template: "CREATE INDEX CONCURRENTLY ON {schema}.{table} ({=columns});" },
        ]),
        pg: &[r#"SELECT n.nspname AS schema, c.relname AS "table", con.conname AS "constraint",
       (SELECT string_agg(quote_ident(a.attname), ', ' ORDER BY k.ord) FROM unnest(con.conkey) WITH ORDINALITY k(attnum, ord) JOIN pg_attribute a ON a.attrelid = con.conrelid AND a.attnum = k.attnum) AS columns,
       con.confrelid::regclass::text AS "references", pg_size_pretty(pg_relation_size(c.oid)) AS table_size
FROM pg_constraint con JOIN pg_class c ON c.oid = con.conrelid JOIN pg_namespace n ON n.oid = c.relnamespace
WHERE con.contype = 'f' AND NOT EXISTS (
  SELECT 1 FROM pg_index i WHERE i.indrelid = con.conrelid AND (string_to_array(i.indkey::text, ' ')::int2[])[1:cardinality(con.conkey)] @> con.conkey)
ORDER BY pg_relation_size(c.oid) DESC"#],
        my: &[],
    },
    Def {
        info: || info("vacuum", "Vacuum & Dead Rows", "Maintenance", "Dead rows waiting for VACUUM and when (auto)vacuum and analyze last ran. A high dead_pct or an old last_autovacuum means autovacuum can't keep up.", vec![
            Action::Sql { label: "Generate VACUUM ANALYZE", template: "VACUUM (ANALYZE, VERBOSE) {schema}.{table};" },
        ]),
        pg: &[r#"SELECT schemaname AS schema, relname AS "table", n_live_tup AS live_rows, n_dead_tup AS dead_rows, round(100 * n_dead_tup::numeric / nullif(n_live_tup + n_dead_tup, 0), 1) AS dead_pct,
       last_vacuum, last_autovacuum, last_analyze, last_autoanalyze, vacuum_count, autovacuum_count
FROM pg_stat_user_tables ORDER BY n_dead_tup DESC LIMIT 300"#],
        my: &[],
    },
    Def {
        info: || info("vacuum_progress", "Vacuum Progress", "Maintenance", "VACUUM running right now (autovacuum workers and manual VACUUM, all databases). phase and scanned_pct show how far it got. Cancelling autovacuum is safe but it starts again later; one marked to prevent wraparound restarts right away.", vec![
            Action::Kill { label: "Cancel Vacuum", terminate: false },
        ]),
        pg: &[r#"SELECT p.pid, p.datname AS db, coalesce(n.nspname || '.' || c.relname, p.relid::text) AS "table", p.phase,
       a.backend_type = 'autovacuum worker' AS auto, a.query LIKE '%(to prevent wraparound)%' AS wraparound,
       round(100 * p.heap_blks_scanned::numeric / nullif(p.heap_blks_total, 0), 1) AS scanned_pct,
       round(100 * p.heap_blks_vacuumed::numeric / nullif(p.heap_blks_total, 0), 1) AS vacuumed_pct,
       p.index_vacuum_count AS index_passes, pg_size_pretty(p.heap_blks_total * current_setting('block_size')::bigint) AS table_size,
       round(extract(epoch FROM now() - a.xact_start)::numeric, 1) AS running_sec, left(a.query, 2000) AS query
FROM pg_stat_progress_vacuum p
LEFT JOIN pg_stat_activity a ON a.pid = p.pid
LEFT JOIN pg_class c ON c.oid = p.relid AND p.datname = current_database()
LEFT JOIN pg_namespace n ON n.oid = c.relnamespace
ORDER BY running_sec DESC NULLS LAST"#],
        my: &[],
    },
    Def {
        info: || info("wraparound", "Transaction ID Age", "Maintenance", "How close each database is to transaction ID wraparound. Over ~50% needs attention: autovacuum must freeze old rows (check long transactions and VACUUM FREEZE).", vec![]),
        pg: &[r#"SELECT datname AS db, age(datfrozenxid) AS xid_age, round(100 * age(datfrozenxid) / 2147483648.0, 1) AS pct_to_wraparound, current_setting('autovacuum_freeze_max_age')::bigint AS freeze_max_age
FROM pg_database ORDER BY age(datfrozenxid) DESC"#],
        my: &[],
    },
    Def {
        info: || info("replication", "Replication", "Server", "Replicas of this server and how far behind they are (or, on a standby, how old the last replayed transaction is).", vec![]),
        pg: &[r#"SELECT 'replica' AS role, client_addr::text AS host, application_name AS name, state, sync_state, write_lag::text AS write_lag, flush_lag::text AS flush_lag, replay_lag::text AS replay_lag,
       pg_size_pretty(pg_wal_lsn_diff(sent_lsn, replay_lsn)) AS behind
FROM pg_stat_replication
UNION ALL
SELECT 'this server is a standby', NULL, NULL, 'recovering', NULL, NULL, NULL, (now() - pg_last_xact_replay_timestamp())::text, NULL WHERE pg_is_in_recovery()"#],
        my: &["SHOW REPLICA STATUS", "SHOW SLAVE STATUS"],
    },
    Def {
        info: || info("server", "Server Overview", "Server", "Version, uptime, connections, cache hit ratio and error counters at a glance.", vec![]),
        pg: &[r#"SELECT 'version' AS metric, version() AS value
UNION ALL SELECT 'uptime', date_trunc('second', now() - pg_postmaster_start_time())::text
UNION ALL SELECT 'connections', (SELECT count(*) FROM pg_stat_activity WHERE backend_type = 'client backend')::text || ' / ' || current_setting('max_connections')
UNION ALL SELECT 'database size', pg_size_pretty(pg_database_size(current_database()))
UNION ALL SELECT 'cache hit ratio', coalesce((SELECT round(100 * blks_hit::numeric / nullif(blks_hit + blks_read, 0), 2)::text || '%' FROM pg_stat_database WHERE datname = current_database()), '-')
UNION ALL SELECT 'commits / rollbacks', (SELECT xact_commit || ' / ' || xact_rollback FROM pg_stat_database WHERE datname = current_database())
UNION ALL SELECT 'deadlocks', (SELECT deadlocks::text FROM pg_stat_database WHERE datname = current_database())
UNION ALL SELECT 'temp files', (SELECT temp_files || ' (' || pg_size_pretty(temp_bytes) || ')' FROM pg_stat_database WHERE datname = current_database())
UNION ALL SELECT 'in recovery (standby)', pg_is_in_recovery()::text"#],
        my: &[r#"SELECT 'version' AS metric, VERSION() AS value
UNION ALL SELECT 'uptime (s)', VARIABLE_VALUE FROM performance_schema.global_status WHERE VARIABLE_NAME = 'Uptime'
UNION ALL SELECT 'connections', CONCAT((SELECT VARIABLE_VALUE FROM performance_schema.global_status WHERE VARIABLE_NAME = 'Threads_connected'), ' / ', @@max_connections)
UNION ALL SELECT 'buffer pool hit ratio', CONCAT(ROUND(100 - 100 * (SELECT VARIABLE_VALUE FROM performance_schema.global_status WHERE VARIABLE_NAME = 'Innodb_buffer_pool_reads') / NULLIF((SELECT VARIABLE_VALUE FROM performance_schema.global_status WHERE VARIABLE_NAME = 'Innodb_buffer_pool_read_requests'), 0), 2), '%')
UNION ALL SELECT 'slow queries', VARIABLE_VALUE FROM performance_schema.global_status WHERE VARIABLE_NAME = 'Slow_queries'
UNION ALL SELECT 'aborted connections', VARIABLE_VALUE FROM performance_schema.global_status WHERE VARIABLE_NAME = 'Aborted_connects'"#],
    },
    Def {
        info: || info("settings", "Settings", "Server", "Server settings; changed ones first.", vec![]),
        pg: &[r#"SELECT name, setting, unit, source, context, short_desc FROM pg_settings ORDER BY source = 'default', category, name"#],
        my: &[r#"SELECT VARIABLE_NAME AS name, VARIABLE_VALUE AS value FROM performance_schema.global_variables ORDER BY VARIABLE_NAME"#, "SHOW GLOBAL VARIABLES"],
    },
    Def {
        info: || info("roles", "Users & Roles", "Security", "Login roles and their powers. superuser / createrole are the dangerous ones.", vec![]),
        pg: &[r#"SELECT r.rolname AS role, r.rolcanlogin AS login, r.rolsuper AS superuser, r.rolcreaterole AS createrole, r.rolcreatedb AS createdb, r.rolreplication AS replication,
       r.rolconnlimit AS conn_limit, r.rolvaliduntil AS valid_until,
       (SELECT string_agg(g.rolname, ', ' ORDER BY g.rolname) FROM pg_auth_members m JOIN pg_roles g ON g.oid = m.roleid WHERE m.member = r.oid) AS member_of
FROM pg_roles r WHERE r.rolname !~ '^pg_' ORDER BY r.rolcanlogin DESC, r.rolname"#],
        my: &[
            r#"SELECT CONCAT(User, '@', Host) AS role, account_locked, password_expired, Super_priv AS super, Grant_priv AS grant_option, Create_user_priv AS create_user FROM mysql.user ORDER BY User"#,
            r#"SELECT GRANTEE AS role, GROUP_CONCAT(PRIVILEGE_TYPE ORDER BY PRIVILEGE_TYPE) AS global_privileges FROM information_schema.USER_PRIVILEGES GROUP BY GRANTEE ORDER BY GRANTEE"#,
        ],
    },
    Def {
        info: || info("grants", "Table Privileges", "Security", "Who may do what on each table (owners are left out). Revoke from the row, or write a GRANT in the console.", vec![
            Action::Sql { label: "Generate REVOKE", template: "REVOKE {=privileges} ON {schema}.{table} FROM {grantee};" },
            Action::Sql { label: "Generate GRANT (template)", template: "GRANT SELECT ON {schema}.{table} TO {grantee};" },
        ]),
        pg: &[r#"SELECT g.grantee, g.table_schema AS schema, g.table_name AS "table", string_agg(g.privilege_type, ', ' ORDER BY g.privilege_type) AS privileges, bool_or(g.is_grantable = 'YES') AS grantable
FROM information_schema.role_table_grants g JOIN pg_class c ON c.relname = g.table_name JOIN pg_namespace n ON n.oid = c.relnamespace AND n.nspname = g.table_schema
WHERE g.table_schema NOT IN ('pg_catalog', 'information_schema') AND g.grantee <> pg_get_userbyid(c.relowner)
GROUP BY g.grantee, g.table_schema, g.table_name ORDER BY g.table_schema, g.table_name, g.grantee"#],
        my: &[r#"SELECT GRANTEE AS grantee, TABLE_SCHEMA AS `schema`, '*' AS `table`, GROUP_CONCAT(PRIVILEGE_TYPE ORDER BY PRIVILEGE_TYPE SEPARATOR ', ') AS privileges FROM information_schema.SCHEMA_PRIVILEGES GROUP BY GRANTEE, TABLE_SCHEMA
UNION ALL
SELECT GRANTEE, TABLE_SCHEMA, TABLE_NAME, GROUP_CONCAT(PRIVILEGE_TYPE ORDER BY PRIVILEGE_TYPE SEPARATOR ', ') FROM information_schema.TABLE_PRIVILEGES GROUP BY GRANTEE, TABLE_SCHEMA, TABLE_NAME
ORDER BY 2, 3, 1"#],
    },
];

fn sqls(d: &Def, kind: DbKind) -> &'static [&'static str] {
    match kind {
        DbKind::Postgres => d.pg,
        DbKind::Mysql => d.my,
        _ => &[],
    }
}

/// 이 DB 에서 볼 수 있는 리포트
pub fn reports(kind: DbKind) -> Vec<ReportInfo> {
    DEFS.iter().filter(|d| !sqls(d, kind).is_empty()).map(|d| (d.info)()).collect()
}

/// 동작의 SQL: 템플릿에 행 값을 넣는다. {col} = 식별자 (필요하면 따옴표), {=col} = 그대로
pub fn action_sql(kind: DbKind, template: &str, row: &HashMap<String, String>) -> String {
    let mut out = String::new();
    let mut rest = template;
    while let Some(i) = rest.find('{') {
        out.push_str(&rest[..i]);
        let Some(j) = rest[i..].find('}') else { break };
        let key = &rest[i + 1..i + j];
        let (raw, name) = match key.strip_prefix('=') {
            Some(n) => (true, n),
            None => (false, key),
        };
        let v = row.get(name).cloned().unwrap_or_default();
        out.push_str(&if raw { v } else { crate::sql::quote_ident(kind, &v) });
        rest = &rest[i + j + 1..];
    }
    out.push_str(rest);
    out
}

/// 꺼진 기능이면 무엇을 켜야 하는지
fn explain(report: &str, kind: DbKind, e: AppError) -> AppError {
    let m = e.to_string();
    let lower = m.to_lowercase();
    let hint = if report == "slow_queries" && kind == DbKind::Postgres && lower.contains("pg_stat_statements") {
        Some("pg_stat_statements is not enabled. Add it to shared_preload_libraries (restart), then run: CREATE EXTENSION pg_stat_statements;")
    } else if lower.contains("denied") || lower.contains("permission") {
        Some("You don't have the privilege to read this (ask for pg_monitor / PROCESS / performance_schema access).")
    } else if kind == DbKind::Mysql && (lower.contains("performance_schema") || lower.contains("sys.")) {
        Some("performance_schema / sys schema is not available on this server — enable performance_schema to see this.")
    } else {
        None
    };
    match hint {
        Some(h) => AppError::Msg(format!("{h}\n({m})")),
        None => e,
    }
}

impl AppState {
    /// 리포트를 돌린다 (읽기만 한다). 뷰가 버전·권한마다 다르면 되는 쿼리를 차례로 해 본다
    pub async fn dba_report(&self, id: String, report: String) -> AppResult<QueryResult> {
        let kind = self.kind_of(&id)?;
        let d = DEFS.iter().find(|d| (d.info)().id == report).ok_or_else(|| AppError::Msg(format!("Unknown report: {report}")))?;
        let list = sqls(d, kind);
        if list.is_empty() {
            return Err(AppError::Msg(format!("{} is not available for this database", (d.info)().title)));
        }
        let pool = ensure_pool(self, &id).await?;
        let mut last = None;
        for q in list {
            match pool.run_query(q, 2000).await {
                // SHOW … 가 빈 결과면 열도 없다 (복제가 없는 서버 등)
                Ok(r) if !r.has_result_set || r.columns.is_empty() => {
                    return Ok(QueryResult {
                        columns: vec![crate::db::ColumnInfo { name: "info".into(), type_name: "TEXT".into() }],
                        rows: vec![vec![serde_json::Value::from("Nothing to show (e.g. no replication on this server)")]],
                        rows_affected: 0,
                        truncated: false,
                        elapsed_ms: r.elapsed_ms,
                        has_result_set: true,
                        in_transaction: false,
                    });
                }
                Ok(r) => return Ok(r),
                Err(e) => last = Some(e),
            }
        }
        Err(explain(&report, kind, last.unwrap()))
    }

    /// PROD 안전장치: 이 문장이 바꿀 행 수 (같은 테이블·조건으로 SELECT count(*)). 셀 수 없는 문장이면 None
    pub async fn preview_impact(&self, id: String, sql: String) -> AppResult<Option<i64>> {
        let kind = self.kind_of(&id)?;
        let Some(q) = crate::guard::impact_count_sql(&sql, kind) else { return Ok(None) };
        let r = ensure_pool(self, &id).await?.run_query(&q, 1).await?;
        Ok(r.rows.first().and_then(|x| x.first()).and_then(|v| v.as_i64().or_else(|| v.as_str().and_then(|s| s.parse().ok()))))
    }

    /// 세션의 지금 쿼리를 취소하거나 (terminate) 연결을 끊는다. Read-only Data Source 는 막는다
    pub async fn kill_session(&self, id: String, pid: i64, terminate: bool) -> AppResult<bool> {
        let cfg = self.source(&id)?;
        if cfg.read_only {
            return Err(AppError::Msg(format!("Data source \"{}\" is read-only", cfg.name)));
        }
        let q = match (cfg.kind, terminate) {
            (DbKind::Postgres, false) => format!("SELECT pg_cancel_backend({pid})"),
            (DbKind::Postgres, true) => format!("SELECT pg_terminate_backend({pid})"),
            (DbKind::Mysql, false) => format!("KILL QUERY {pid}"),
            (DbKind::Mysql, true) => format!("KILL {pid}"),
            (k, _) => return Err(AppError::Msg(format!("Not available for {k:?}"))),
        };
        let r = ensure_pool(self, &id).await?.run_query(&q, 1).await?;
        Ok(match r.rows.first().and_then(|x| x.first()) {
            Some(serde_json::Value::Bool(b)) => *b,
            _ => true,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn action_templates_quote_identifiers() {
        let row: HashMap<String, String> = [("schema", "public"), ("index", "Idx Orders"), ("columns", "a, b"), ("query", "select 1")].iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
        assert_eq!(action_sql(DbKind::Postgres, "DROP INDEX CONCURRENTLY {schema}.{index};", &row), "DROP INDEX CONCURRENTLY public.\"Idx Orders\";");
        assert_eq!(action_sql(DbKind::Mysql, "x {schema}.{index} ({=columns}) {=query}", &row), "x public.`Idx Orders` (a, b) select 1");
    }

    #[test]
    fn every_db_has_the_core_reports() {
        for kind in [DbKind::Postgres, DbKind::Mysql] {
            let ids: Vec<_> = reports(kind).iter().map(|r| r.id).collect();
            for want in ["sessions", "locks", "slow_queries", "table_sizes", "unused_indexes", "server", "settings", "roles", "grants"] {
                assert!(ids.contains(&want), "{kind:?} lacks {want}");
            }
        }
        let pg: Vec<_> = reports(DbKind::Postgres).iter().map(|r| r.id).collect();
        for want in ["duplicate_indexes", "fk_without_index", "table_bloat", "index_bloat", "vacuum", "vacuum_progress", "wraparound", "replication"] {
            assert!(pg.contains(&want), "pg lacks {want}");
        }
        assert!(reports(DbKind::Mysql).iter().any(|r| r.id == "fragmentation"));
        assert!(reports(DbKind::Redis).is_empty());
    }

    fn cfg(kind: DbKind, env: &str) -> Option<(crate::store::ConnectionConfig, Option<String>)> {
        let e = std::env::var(env).ok()?;
        let p: Vec<&str> = e.split(':').collect();
        let (user, pass, db) = if kind == DbKind::Mysql { (p[2], Some(p[3].to_string()), p[4]) } else { (p[2], None, p[3]) };
        Some((
            crate::store::ConnectionConfig {
                id: "d".into(), name: "d".into(), kind, host: p[0].into(), port: p[1].parse().unwrap(), user: user.into(), database: db.into(),
                color: None, ssh: None, password_saved: false, ssh_secret_saved: false, scope: Default::default(), read_only: false, options: Default::default(), ssl: None, redis: None,
            },
            pass,
        ))
    }

    async fn state(kind: DbKind, env: &str) -> Option<AppState> {
        let (c, pass) = cfg(kind, env)?;
        let dir = std::env::temp_dir().join(format!("qurry-dba-{}", uuid::Uuid::new_v4()));
        let st = crate::open_state(Some(dir)).unwrap();
        st.save_connection(c, pass, None).await.unwrap();
        Some(st)
    }

    /// 모든 리포트가 돌거나, 꺼진 기능이면 무엇을 켜야 하는지 알려 준다 (pg_stat_statements 등)
    async fn all_reports_run(kind: DbKind, env: &str) {
        let Some(st) = state(kind, env).await else { return };
        for r in reports(kind) {
            match st.dba_report("d".into(), r.id.into()).await {
                Ok(res) => assert!(res.has_result_set, "{}", r.id),
                Err(e) => {
                    let e = e.to_string();
                    assert!(e.contains("enable") || e.contains("privilege") || e.contains("not available"), "{}: {e}", r.id);
                }
            }
        }
    }

    #[tokio::test]
    #[ignore]
    async fn pg_reports_run() {
        all_reports_run(DbKind::Postgres, "QURRY_TEST_PG").await;
    }

    #[tokio::test]
    #[ignore]
    async fn mysql_reports_run() {
        all_reports_run(DbKind::Mysql, "QURRY_TEST_MY").await;
    }

    /// PROD 안전장치: 바꿀 행 수를 미리 센다 (실제로 바꾸지 않는다)
    #[tokio::test]
    #[ignore]
    async fn pg_preview_impact_counts_rows() {
        let Some(st) = state(DbKind::Postgres, "QURRY_TEST_PG").await else { return };
        let q = |s: &str| st.run_query("d".into(), s.into(), None, None);
        q("drop table if exists qurry_impact").await.ok();
        q("create table qurry_impact as select g as id from generate_series(1, 100) g").await.unwrap();
        assert_eq!(st.preview_impact("d".into(), "delete from qurry_impact where id <= 30".into()).await.unwrap(), Some(30));
        assert_eq!(st.preview_impact("d".into(), "update qurry_impact set id = id".into()).await.unwrap(), Some(100));
        assert_eq!(st.preview_impact("d".into(), "select 1".into()).await.unwrap(), None);
        assert_eq!(q("select count(*) from qurry_impact").await.unwrap().rows[0][0], serde_json::json!(100));
        q("drop table qurry_impact").await.ok();
    }

    /// 지운 행이 VACUUM 전까지 자리를 차지하면 테이블·인덱스 bloat 로 보인다
    #[tokio::test]
    #[ignore]
    async fn pg_bloat_shows_deleted_space() {
        let Some(st) = state(DbKind::Postgres, "QURRY_TEST_PG").await else { return };
        let q = |s: &str| st.run_query("d".into(), s.into(), None, None);
        q("drop table if exists qurry_bloat").await.ok();
        q("create table qurry_bloat (id int primary key, pad text) with (autovacuum_enabled = false)").await.unwrap();
        q("insert into qurry_bloat select g, repeat('x', 200) from generate_series(1, 20000) g").await.unwrap();
        q("delete from qurry_bloat where id % 10 <> 0").await.unwrap();
        q("analyze qurry_bloat").await.unwrap();
        let pct = |r: &QueryResult, key: &str, name: &str| {
            let col = |n: &str| r.columns.iter().position(|c| c.name == n).unwrap();
            let row = r.rows.iter().find(|x| crate::db::cell_text(&x[col(key)]) == name).unwrap_or_else(|| panic!("{name} not in report"));
            crate::db::cell_text(&row[col("bloat_pct")]).parse::<f64>().unwrap()
        };
        let t = st.dba_report("d".into(), "table_bloat".into()).await.unwrap();
        assert!(pct(&t, "table", "qurry_bloat") > 50.0);
        let i = st.dba_report("d".into(), "index_bloat".into()).await.unwrap();
        assert!(pct(&i, "index", "qurry_bloat_pkey") > 50.0);
        q("drop table qurry_bloat").await.ok();
    }

    /// 락을 잡은 세션과 기다리는 세션이 보이고, 잡은 쪽을 끊으면 기다리던 쪽이 끝난다
    #[tokio::test]
    #[ignore]
    async fn pg_blocking_session_is_shown_and_killed() {
        let Some(st) = state(DbKind::Postgres, "QURRY_TEST_PG").await else { return };
        let (c, _) = cfg(DbKind::Postgres, "QURRY_TEST_PG").unwrap();
        let url = format!("postgres://{}@{}:{}/{}", c.user, c.host, c.port, c.database);
        let a = sqlx::postgres::PgPoolOptions::new().max_connections(1).connect(&url).await.unwrap();
        let b = sqlx::postgres::PgPoolOptions::new().max_connections(1).connect(&url).await.unwrap();
        sqlx::raw_sql("drop table if exists qurry_dba_lock; create table qurry_dba_lock (id int primary key); insert into qurry_dba_lock values (1)").execute(&a).await.unwrap();
        let mut ta = a.begin().await.unwrap();
        let holder: i32 = sqlx::query_scalar("select pg_backend_pid()").fetch_one(&mut *ta).await.unwrap();
        sqlx::query("update qurry_dba_lock set id = 1").execute(&mut *ta).await.unwrap();
        let waiter = tokio::spawn(async move { sqlx::query("update qurry_dba_lock set id = 1").execute(&b).await });
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;

        let s = st.dba_report("d".into(), "sessions".into()).await.unwrap();
        let col = |n: &str| s.columns.iter().position(|c| c.name == n).unwrap();
        let blocked = s.rows.iter().find(|r| crate::db::cell_text(&r[col("blocked_by")]) == holder.to_string()).expect("waiting session");
        let _ = blocked;
        let holder_row = s.rows.iter().find(|r| crate::db::cell_text(&r[col("pid")]) == holder.to_string()).unwrap();
        assert_eq!(crate::db::cell_text(&holder_row[col("blocking")]), "1");
        let l = st.dba_report("d".into(), "locks".into()).await.unwrap();
        assert_eq!(l.rows.len(), 1);

        assert!(st.kill_session("d".into(), holder as i64, true).await.unwrap());
        let r = tokio::time::timeout(std::time::Duration::from_secs(5), waiter).await.expect("waiter finished").unwrap();
        assert!(r.is_ok());
        drop(ta);
        sqlx::raw_sql("drop table if exists qurry_dba_lock").execute(&a).await.ok();
    }
}

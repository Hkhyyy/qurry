//! DBA reports: sessions/locks, slow queries, size/indexes/bloat, maintenance/VACUUM progress, replication/server, privileges. Shows query results as tables with a few actions (end session, generate SQL).
//! GUI and TUI show the same report list as is
use crate::app::{AppState, checked_pool};
use crate::db::{Pool, QueryResult};
use crate::error::{AppError, AppResult};
use crate::store::DbKind;
use std::collections::HashMap;

/// Action attached to a report row
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum Action {
    /// End a session (terminate = drop the connection, otherwise cancel only the current query). Uses the pid column
    Kill { label: &'static str, terminate: bool },
    /// Put SQL in the console. {col} = that column's value as an identifier (quoted), {=col} = as is
    Sql { label: &'static str, template: &'static str },
    /// EXPLAIN of the query column in a console (normalized statements: generic plan on PG16+, otherwise a note to fill in the parameters)
    Explain { label: &'static str },
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReportInfo {
    pub id: &'static str,
    pub title: &'static str,
    pub group: &'static str,
    /// What it shows and how to read it
    pub help: &'static str,
    pub actions: Vec<Action>,
}

/// One report's SQL. Tried in order, the first that works is used (views differ by version and privileges)
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
        // pg_blocking_pids() is costly (the docs warn against frequent calls): only for sessions waiting for a lock
        pg: &[r#"WITH w AS (SELECT pid, pg_blocking_pids(pid) AS by FROM pg_stat_activity WHERE wait_event_type = 'Lock')
SELECT * FROM (SELECT a.pid, a.usename AS "user", a.datname AS db, a.application_name AS app, a.client_addr::text AS client, a.state,
       a.wait_event_type || coalesce(':' || a.wait_event, '') AS wait,
       (SELECT count(*) FROM w WHERE a.pid = ANY(w.by)) AS blocking,
       coalesce((SELECT array_to_string(w.by, ',') FROM w WHERE w.pid = a.pid), '') AS blocked_by,
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
            Action::Explain { label: "Explain in Console" },
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
    // bloat: estimate the expected page count from the average row size in pg_stats and compare with the current page count (without pgstattuple). Tables without stats are excluded
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
        info: || info("fragmentation", "Table Fragmentation", "Storage", "Free space inside each table's file (DATA_FREE), left behind by deletes and updates. OPTIMIZE TABLE rebuilds the table to give it back. MySQL 8.0+ caches these numbers (information_schema_stats_expiry, 24 h by default), so run ANALYZE TABLE first for current ones. DATA_FREE counts whole free extents: small tables often show a few MB free that OPTIMIZE can't give back. Tables in the shared system tablespace all show that tablespace's free space.", vec![
            Action::Sql { label: "Generate OPTIMIZE TABLE", template: "OPTIMIZE TABLE {schema}.{table};" },
        ]),
        pg: &[],
        my: &[r#"SELECT TABLE_SCHEMA AS `schema`, TABLE_NAME AS `table`, ROUND((DATA_LENGTH + INDEX_LENGTH) / 1048576, 1) AS total_mb, ROUND(DATA_FREE / 1048576, 1) AS free_mb,
       ROUND(100 * DATA_FREE / NULLIF(DATA_LENGTH + INDEX_LENGTH + DATA_FREE, 0), 1) AS free_pct, ENGINE AS engine
FROM information_schema.TABLES WHERE TABLE_TYPE = 'BASE TABLE' AND DATA_FREE > 0 AND TABLE_SCHEMA NOT IN ('mysql', 'sys', 'performance_schema', 'information_schema')
ORDER BY DATA_FREE DESC LIMIT 300"#],
    },
    Def {
        info: || info("unused_indexes", "Unused Indexes", "Storage", "Indexes never used since statistics were reset (unique and primary keys are left out); counting_since tells since when (MySQL: since the server started). They cost disk and slow down writes. Check replicas too before dropping.", vec![
            Action::Sql { label: "Generate DROP INDEX", template: "DROP INDEX CONCURRENTLY {schema}.{index};" },
        ]),
        pg: &[r#"SELECT s.schemaname AS schema, s.relname AS "table", s.indexrelname AS "index", s.idx_scan AS scans,
       coalesce((SELECT stats_reset FROM pg_stat_database WHERE datname = current_database()), pg_postmaster_start_time()) AS counting_since,
       pg_size_pretty(pg_relation_size(s.indexrelid)) AS size, pg_get_indexdef(s.indexrelid) AS definition
FROM pg_stat_user_indexes s JOIN pg_index i ON i.indexrelid = s.indexrelid
WHERE s.idx_scan = 0 AND NOT i.indisunique AND NOT i.indisprimary
ORDER BY pg_relation_size(s.indexrelid) DESC"#],
        // performance_schema counts since the server started
        my: &[
            r#"SELECT object_schema AS `schema`, object_name AS `table`, index_name AS `index`,
       NOW() - INTERVAL (SELECT VARIABLE_VALUE FROM performance_schema.global_status WHERE VARIABLE_NAME = 'Uptime') SECOND AS counting_since
FROM sys.schema_unused_indexes ORDER BY object_schema, object_name"#,
            r#"SELECT OBJECT_SCHEMA AS `schema`, OBJECT_NAME AS `table`, INDEX_NAME AS `index`,
       NOW() - INTERVAL (SELECT VARIABLE_VALUE FROM performance_schema.global_status WHERE VARIABLE_NAME = 'Uptime') SECOND AS counting_since
FROM performance_schema.table_io_waits_summary_by_index_usage
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
        info: || info("vacuum_progress", "Vacuum Progress", "Maintenance", "VACUUM running right now (autovacuum workers and manual VACUUM, all databases). phase and scanned_pct show how far it got; VACUUM FULL shows as phase 'VACUUM FULL: …' (PostgreSQL 12+), with no vacuumed_pct as it rewrites the table. Cancelling autovacuum is safe but it starts again later; one marked to prevent wraparound restarts right away.", vec![
            Action::Kill { label: "Cancel Vacuum", terminate: false },
        ]),
        // VACUUM FULL reports in pg_stat_progress_cluster (12+), not pg_stat_progress_vacuum. Older servers: the second query
        pg: &[r#"SELECT p.pid, p.datname AS db, coalesce(n.nspname || '.' || c.relname, p.relid::text) AS "table", p.phase,
       a.backend_type = 'autovacuum worker' AS auto, a.query LIKE '%(to prevent wraparound)%' AS wraparound,
       round(100 * p.heap_blks_scanned::numeric / nullif(p.heap_blks_total, 0), 1) AS scanned_pct,
       round(100 * p.heap_blks_vacuumed::numeric / nullif(p.heap_blks_total, 0), 1) AS vacuumed_pct,
       p.index_vacuum_count AS index_passes, pg_size_pretty(p.heap_blks_total * current_setting('block_size')::bigint) AS table_size,
       round(extract(epoch FROM now() - a.xact_start)::numeric, 1) AS running_sec, left(a.query, 2000) AS query
FROM (SELECT pid, datname, relid, phase, heap_blks_total, heap_blks_scanned, heap_blks_vacuumed, index_vacuum_count FROM pg_stat_progress_vacuum
      UNION ALL
      SELECT pid, datname, relid, 'VACUUM FULL: ' || phase, heap_blks_total, heap_blks_scanned, NULL, NULL FROM pg_stat_progress_cluster WHERE command = 'VACUUM FULL') p
LEFT JOIN pg_stat_activity a ON a.pid = p.pid
LEFT JOIN pg_class c ON c.oid = p.relid AND p.datname = current_database()
LEFT JOIN pg_namespace n ON n.oid = c.relnamespace
ORDER BY running_sec DESC NULLS LAST"#, r#"SELECT p.pid, p.datname AS db, coalesce(n.nspname || '.' || c.relname, p.relid::text) AS "table", p.phase,
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
        // MariaDB keeps information_schema.GLOBAL_STATUS (its performance_schema is off by default); MySQL 5.7+ moved it to performance_schema
        my: &[
            r#"SELECT 'version' AS metric, VERSION() AS value
UNION ALL SELECT 'uptime (s)', VARIABLE_VALUE FROM information_schema.GLOBAL_STATUS WHERE VARIABLE_NAME = 'UPTIME'
UNION ALL SELECT 'connections', CONCAT((SELECT VARIABLE_VALUE FROM information_schema.GLOBAL_STATUS WHERE VARIABLE_NAME = 'THREADS_CONNECTED'), ' / ', @@max_connections)
UNION ALL SELECT 'buffer pool hit ratio', CONCAT(ROUND(100 - 100 * (SELECT VARIABLE_VALUE FROM information_schema.GLOBAL_STATUS WHERE VARIABLE_NAME = 'INNODB_BUFFER_POOL_READS') / NULLIF((SELECT VARIABLE_VALUE FROM information_schema.GLOBAL_STATUS WHERE VARIABLE_NAME = 'INNODB_BUFFER_POOL_READ_REQUESTS'), 0), 2), '%')
UNION ALL SELECT 'slow queries', VARIABLE_VALUE FROM information_schema.GLOBAL_STATUS WHERE VARIABLE_NAME = 'SLOW_QUERIES'
UNION ALL SELECT 'aborted connections', VARIABLE_VALUE FROM information_schema.GLOBAL_STATUS WHERE VARIABLE_NAME = 'ABORTED_CONNECTS'"#,
            r#"SELECT 'version' AS metric, VERSION() AS value
UNION ALL SELECT 'uptime (s)', VARIABLE_VALUE FROM performance_schema.global_status WHERE VARIABLE_NAME = 'Uptime'
UNION ALL SELECT 'connections', CONCAT((SELECT VARIABLE_VALUE FROM performance_schema.global_status WHERE VARIABLE_NAME = 'Threads_connected'), ' / ', @@max_connections)
UNION ALL SELECT 'buffer pool hit ratio', CONCAT(ROUND(100 - 100 * (SELECT VARIABLE_VALUE FROM performance_schema.global_status WHERE VARIABLE_NAME = 'Innodb_buffer_pool_reads') / NULLIF((SELECT VARIABLE_VALUE FROM performance_schema.global_status WHERE VARIABLE_NAME = 'Innodb_buffer_pool_read_requests'), 0), 2), '%')
UNION ALL SELECT 'slow queries', VARIABLE_VALUE FROM performance_schema.global_status WHERE VARIABLE_NAME = 'Slow_queries'
UNION ALL SELECT 'aborted connections', VARIABLE_VALUE FROM performance_schema.global_status WHERE VARIABLE_NAME = 'Aborted_connects'"#,
        ],
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

/// Report columns that hold statement text, which can carry values from the rows (sessions, locks, slow_queries, vacuum_progress).
/// MCP masks their literals (sql::mask_literals); the test query_text_columns_are_listed fails when a report adds another one.
/// Index definitions (definition, sql_drop_index) are schema, as get_ddl shows them, so they stay
pub const QUERY_TEXT_COLUMNS: &[&str] = &["query", "blocking_query", "waiting_query"];

/// Server messages that quote row values or the failed statement: MySQL's SHOW REPLICA STATUS (Last_SQL_Error: Error 'Duplicate
/// entry '…' for key …' on query. … Query: '…'). Their quotes nest without escaping, so MCP masks them as a whole, not by literal
pub const ERROR_TEXT_COLUMNS: &[&str] = &["last_error", "last_sql_error", "last_io_error"];

/// Reports available for this DB
pub fn reports(kind: DbKind) -> Vec<ReportInfo> {
    DEFS.iter().filter(|d| !sqls(d, kind).is_empty()).map(|d| (d.info)()).collect()
}

/// Action SQL: fill the template with row values. {col} = identifier (quoted if needed), {=col} = as is
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

/// EXPLAIN for a statement from Slow Queries. Their text is normalized ($1 from pg_stat_statements; ?, (...) for value lists and /* , ... */ for more rows from DIGEST_TEXT), which EXPLAIN can't run:
/// PostgreSQL 16+ plans it generically, otherwise the console gets a note to fill in the parameters first
pub fn explain_sql(kind: DbKind, pg_major: Option<u64>, query: &str) -> String {
    let (params, mark) = match kind {
        DbKind::Postgres => (query.as_bytes().windows(2).any(|w| w[0] == b'$' && w[1].is_ascii_digit()), "$1, $2 …"),
        _ => (query.contains('?') || query.contains("(...)") || query.contains("/* , ... */"), "?, (...)"),
    };
    match params {
        false => format!("EXPLAIN {query}"),
        true if kind == DbKind::Postgres && pg_major.is_some_and(|m| m >= 16) => format!("EXPLAIN (GENERIC_PLAN) {query}"),
        true => format!("-- Replace the parameters ({mark}) with real values, then run:\nEXPLAIN {query}"),
    }
}

/// Time limit of one report query (bloat estimates on huge catalogs can run long)
const REPORT_TIMEOUT_S: u32 = 30;
const REPORT_ROWS: usize = 2000;

/// MySQL: the time limit travels with the statement — the MAX_EXECUTION_TIME hint (MySQL), SET STATEMENT … FOR in a comment only MariaDB runs.
/// Other statements (SHOW …) are quick and take neither
fn my_time_limited(sql: &str, secs: u32) -> String {
    match sql.strip_prefix("SELECT ") {
        Some(rest) => format!("/*M!100108 SET STATEMENT max_statement_time = {secs} FOR */ SELECT /*+ MAX_EXECUTION_TIME({}) */ {rest}", secs * 1000),
        None => sql.to_string(),
    }
}

/// Run a report query with a time limit that ends with it, not on the pooled session. The limit only tightens: a stricter server / role / session limit stays
async fn run_limited(pool: &Pool, kind: DbKind, sql: &str, secs: u32) -> AppResult<QueryResult> {
    let ms = secs * 1000;
    match pool {
        // BEGIN and SET LOCAL in one round trip (setting 0 = no limit)
        Pool::Pg(p) => {
            let begin = format!("BEGIN; SELECT set_config('statement_timeout', '{ms}', true) WHERE (SELECT setting::bigint FROM pg_settings WHERE name = 'statement_timeout') NOT BETWEEN 1 AND {ms}");
            let mut tx = match p.begin_with(begin).await {
                Ok(tx) => tx,
                // pgbouncer in statement pooling mode allows no transaction: run without the limit
                Err(e) if e.to_string().contains("transaction blocks not allowed") => return pool.run_query(sql, REPORT_ROWS).await,
                Err(e) => return Err(e.into()),
            };
            let r = crate::db::run_pg_on(&mut tx, sql, REPORT_ROWS, None).await;
            let _ = tx.rollback().await;
            r
        }
        _ if kind == DbKind::Mysql && sql.starts_with("SELECT ") => {
            let cur = pool.run_query("SHOW VARIABLES WHERE Variable_name IN ('max_execution_time', 'max_statement_time')", 2).await?;
            let stricter = cur.rows.iter().any(|r| {
                let (name, v) = (crate::db::cell_text(&r[0]), crate::db::cell_text(&r[1]).parse::<f64>().unwrap_or(0.0));
                // MySQL max_execution_time is in ms, MariaDB max_statement_time in seconds
                let cur_ms = if name == "max_statement_time" { v * 1000.0 } else { v };
                cur_ms > 0.0 && cur_ms <= ms as f64
            });
            pool.run_query(&if stricter { sql.to_string() } else { my_time_limited(sql, secs) }, REPORT_ROWS).await
        }
        _ => pool.run_query(sql, REPORT_ROWS).await,
    }
}

/// Run the first of a report's queries that works (views differ by version and privileges). A time limit ends the search: the next query would hide it
async fn run_first(pool: &Pool, kind: DbKind, list: &[&str], secs: u32) -> AppResult<QueryResult> {
    let mut last = None;
    for q in list {
        match run_limited(pool, kind, q, secs).await {
            Err(e) if is_timeout(&e) => return Err(e),
            Err(e) => last = Some(e),
            ok => return ok,
        }
    }
    Err(last.expect("report has queries"))
}

/// Statement time limit hit (PG 57014 query_canceled, MySQL 3024, MariaDB 1969)
fn is_timeout(e: &AppError) -> bool {
    match e {
        AppError::Db(sqlx::Error::Database(d)) => d.code().as_deref() == Some("57014") || matches!(my_errno(e), Some(3024 | 1969)),
        _ => false,
    }
}

/// MySQL error number (1095 = not owner of the thread; 1305 = no such procedure, or 1370 = may not execute it — MySQL checks that first)
fn my_errno(e: &AppError) -> Option<u16> {
    match e {
        AppError::Db(sqlx::Error::Database(d)) => d.try_downcast_ref::<sqlx::mysql::MySqlDatabaseError>().map(|m| m.number()),
        _ => None,
    }
}

/// What Slow Queries needs when pg_stat_statements is missing, from the server's shared_preload_libraries (None: not readable).
/// Managed servers (RDS, Aurora) usually preload it already, so only the extension is missing
fn pg_stat_statements_hint(preload: Option<&str>) -> String {
    const CREATE: &str = "CREATE EXTENSION pg_stat_statements; in this database";
    const PRIV: &str = "(as a superuser; rds_superuser on RDS / Aurora, cloudsqlsuperuser on Cloud SQL)";
    const LOAD: &str = "add pg_stat_statements to shared_preload_libraries and restart the server (on RDS / Aurora / Cloud SQL: in the parameter group / database flags, then reboot)";
    match preload.map(|l| l.split(',').any(|x| x.trim().trim_matches('"') == "pg_stat_statements")) {
        Some(true) => format!("pg_stat_statements is preloaded but not enabled in this database: run {CREATE} {PRIV}."),
        Some(false) => format!("pg_stat_statements is not enabled: {LOAD}, then {CREATE} if it isn't there yet {PRIV}."),
        None => format!("pg_stat_statements is not enabled. If it is not in shared_preload_libraries, {LOAD}. Then {CREATE} if it isn't there yet {PRIV}."),
    }
}

/// A Slow Queries error that comes from pg_stat_statements missing (not a time limit or a lost connection)
fn pg_stat_statements_missing(report: &str, kind: DbKind, e: &AppError) -> bool {
    report == "slow_queries" && kind == DbKind::Postgres && !is_timeout(e) && e.to_string().to_lowercase().contains("pg_stat_statements")
}

/// If the feature is off, what needs to be enabled (`preload`: PostgreSQL's shared_preload_libraries, if read)
fn explain(report: &str, kind: DbKind, e: AppError, preload: Option<&str>) -> AppError {
    let m = e.to_string();
    let lower = m.to_lowercase();
    let timeout = format!("The report ran into its time limit ({REPORT_TIMEOUT_S} s, or a stricter one set on the server). The server may be busy or the statistics very large: try again later.");
    let pss;
    let hint = if is_timeout(&e) {
        Some(timeout.as_str())
    } else if pg_stat_statements_missing(report, kind, &e) {
        pss = pg_stat_statements_hint(preload);
        Some(pss.as_str())
    } else if lower.contains("denied") || lower.contains("permission") {
        Some("You don't have the privilege to read this (ask for pg_monitor / PROCESS / performance_schema access).")
    } else if kind == DbKind::Mysql && (lower.contains("performance_schema") || lower.contains("sys.")) {
        Some("performance_schema / sys schema is not available on this server — enable performance_schema to see this (MariaDB has it off by default, and some of these tables only exist on MySQL).")
    } else {
        None
    };
    match hint {
        Some(h) => AppError::Msg(format!("{h}\n({m})")),
        None => e,
    }
}

impl AppState {
    /// Run a report (read-only)
    pub async fn dba_report(&self, id: String, report: String) -> AppResult<QueryResult> {
        let kind = self.kind_of(&id)?;
        let d = DEFS.iter().find(|d| (d.info)().id == report).ok_or_else(|| AppError::Msg(format!("Unknown report: {report}")))?;
        let list = sqls(d, kind);
        if list.is_empty() {
            return Err(AppError::Msg(format!("{} is not available for this database", (d.info)().title)));
        }
        let pool = match checked_pool(self, &id).await {
            Ok(p) => p,
            Err(e) => return Err(self.connect_failed(&id, e)),
        };
        match run_first(&pool, kind, list, REPORT_TIMEOUT_S).await {
            // An empty SHOW … result has no columns either (servers without replication, etc.)
            Ok(r) if !r.has_result_set || r.columns.is_empty() => Ok(QueryResult {
                columns: vec![crate::db::ColumnInfo { name: "info".into(), type_name: "TEXT".into() }],
                rows: vec![vec![serde_json::Value::from("Nothing to show (e.g. no replication on this server)")]],
                rows_affected: 0,
                truncated: false,
                elapsed_ms: r.elapsed_ms,
                has_result_set: true,
                in_transaction: false,
                warning: None,
                editable: None,
                notices: vec![],
            }),
            Ok(r) => Ok(r),
            Err(e) => {
                let preload = match pg_stat_statements_missing(&report, kind, &e) {
                    true => pool.run_query("SELECT current_setting('shared_preload_libraries')", 1).await.ok().and_then(|r| r.rows.first().map(|x| crate::db::cell_text(&x[0]))),
                    false => None,
                };
                Err(explain(&report, kind, e, preload.as_deref()))
            }
        }
    }

    /// The server turned down the report's database (it doesn't exist, or no CONNECT privilege): name it, so another one can be
    /// picked (not on a one-database source). Server down, sign-in, SSH and TLS errors stay as they are
    fn connect_failed(&self, id: &str, e: AppError) -> AppError {
        let db_refused = match &e {
            AppError::Db(sqlx::Error::Database(d)) => matches!(d.code().as_deref(), Some("3D000" | "42501")) || matches!(my_errno(&e), Some(1049 | 1044)),
            _ => false,
        };
        let (base, db) = crate::app::split_target(id);
        let cfg = self.store.get(base).ok().flatten();
        let Some(db) = db.map(str::to_string).or_else(|| cfg.as_ref().map(crate::sql::default_db)).filter(|d| db_refused && !d.is_empty()) else { return e };
        match cfg.is_some_and(|c| c.options.single_db) {
            true => AppError::Msg(format!("Couldn't connect to database {db}.\n({e})")),
            false => AppError::Msg(format!("Couldn't connect to database {db}. Pick another database for this report (GUI: the database list in its toolbar · TUI: the report's database list).\n({e})")),
        }
    }

    /// Explain in Console for a Slow Queries row (see explain_sql)
    pub async fn dba_explain(&self, id: String, query: String) -> AppResult<String> {
        let kind = self.kind_of(&id)?;
        let major = match kind {
            DbKind::Postgres => crate::backup::pg_major(self, &id).await,
            _ => None,
        };
        Ok(explain_sql(kind, major, &query))
    }

    /// PROD safeguard: number of rows this statement would change (SELECT count(*) with the same table and condition). None if it cannot be counted
    pub async fn preview_impact(&self, id: String, sql: String) -> AppResult<Option<i64>> {
        let kind = self.kind_of(&id)?;
        let Some(q) = crate::guard::impact_count_sql(&sql, kind) else { return Ok(None) };
        let r = checked_pool(self, &id).await?.run_query(&q, 1).await?;
        Ok(crate::guard::impact_count(&r))
    }

    /// Cancel the session's current query, or (terminate) drop the connection. Blocked for Read-only Data Sources
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
        let pool = checked_pool(self, &id).await?;
        let r = match pool.run_query(&q, 1).await {
            // RDS / Aurora MySQL: killing another user's thread needs CONNECTION_ADMIN, which the master user lacks; mysql.rds_kill(_query) does it.
            // Elsewhere (no such procedure) the KILL error stays
            Err(e) if my_errno(&e) == Some(1095) => {
                let call = format!("CALL mysql.{}({pid})", if terminate { "rds_kill" } else { "rds_kill_query" });
                pool.run_query(&call, 1).await.map_err(|e2| if matches!(my_errno(&e2), Some(1305 | 1370)) { e } else { e2 })?
            }
            r => r?,
        };
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

    /// Output names of the columns a report SQL fills with statement text: a server column that holds it (pg_stat_activity.query,
    /// pg_stat_statements.query, PROCESSLIST.INFO, DIGEST_TEXT, sys.innodb_lock_waits' …_query), bare or in left(…, n), and the
    /// name it gets (AS x, otherwise its own). Used in a condition (LIKE, IS NULL) it is no output
    fn query_text_outputs(sql: &str, mysql: bool) -> Vec<String> {
        use qurry_shared::scope::{K, lex};
        const SOURCES: &[&str] = &["query", "info", "digest_text", "blocking_query", "waiting_query", "sql_text", "query_sample_text"];
        let toks = lex(sql, mysql);
        let w = |i: usize| toks.get(i).map(|t| sql[t.s..t.e].trim_matches(|c| c == '"' || c == '`').to_lowercase()).unwrap_or_default();
        let mut out = vec![];
        for (i, t) in toks.iter().enumerate() {
            if t.k != K::Word || !SOURCES.contains(&w(i).as_str()) || (i > 0 && w(i - 1) == "as") {
                continue;
            }
            // left(x, 4000)
            let next = if w(i + 1) == "," && toks.get(i + 2).is_some_and(|t| t.k == K::Num) && w(i + 3) == ")" { i + 4 } else { i + 1 };
            match w(next).as_str() {
                "as" => out.push(w(next + 1)),
                "," | "from" | "" => out.push(w(i)),
                _ => {}
            }
        }
        out
    }

    /// Sees only the columns a report's SELECT names: a SHOW statement's columns come from the server, so they never reach
    /// query_text_outputs (SHOW REPLICA STATUS's Last_Error, Last_SQL_Error, Last_IO_Error are listed by hand in ERROR_TEXT_COLUMNS)
    #[test]
    fn query_text_columns_are_listed() {
        assert_eq!(query_text_outputs("SELECT left(a.query, 4000) AS statement, a.query LIKE '%x%' AS w, b.query FROM t", false), ["statement", "query"]);
        let mut seen = std::collections::BTreeSet::new();
        for d in DEFS {
            for (sql, mysql) in d.pg.iter().map(|s| (s, false)).chain(d.my.iter().map(|s| (s, true))) {
                for name in query_text_outputs(sql, mysql) {
                    assert!(QUERY_TEXT_COLUMNS.contains(&name.as_str()), "{}: column {name} holds statement text; add it to QUERY_TEXT_COLUMNS", (d.info)().id);
                    seen.insert((d.info)().id);
                }
            }
        }
        assert_eq!(seen.into_iter().collect::<Vec<_>>(), ["locks", "sessions", "slow_queries", "vacuum_progress"]);
    }

    /// Normalized statements ($1 from pg_stat_statements, ? from DIGEST_TEXT) can't be explained as they are
    #[test]
    fn explain_handles_placeholders() {
        let pg = "select * from t where id = $1";
        assert_eq!(explain_sql(DbKind::Postgres, Some(16), pg), "EXPLAIN (GENERIC_PLAN) select * from t where id = $1");
        assert_eq!(explain_sql(DbKind::Postgres, Some(17), "select 1"), "EXPLAIN select 1");
        let old = explain_sql(DbKind::Postgres, Some(15), pg);
        assert!(old.starts_with("-- ") && old.contains("$1") && old.ends_with("\nEXPLAIN select * from t where id = $1"), "{old}");
        let my = explain_sql(DbKind::Mysql, None, "SELECT * FROM `t` WHERE `id` = ?");
        assert!(my.starts_with("-- ") && my.contains('?') && my.ends_with("\nEXPLAIN SELECT * FROM `t` WHERE `id` = ?"), "{my}");
        assert_eq!(explain_sql(DbKind::Mysql, None, "SELECT 1"), "EXPLAIN SELECT 1");
        // DIGEST_TEXT collapses value lists and extra rows
        for q in ["SELECT * FROM `mysql` . `user` WHERE SYSTEM_USER IN (...)", "INSERT INTO `t` VALUES (...) /* , ... */"] {
            let my = explain_sql(DbKind::Mysql, None, q);
            assert!(my.starts_with("-- ") && my.contains("(...)") && my.ends_with(&format!("\nEXPLAIN {q}")), "{my}");
        }
    }

    /// MySQL report queries carry their own time limit (MySQL hint, MariaDB executable comment), so it doesn't stay on the pooled session
    #[test]
    fn mysql_report_queries_are_time_limited() {
        assert_eq!(my_time_limited("SELECT a FROM t", 30), "/*M!100108 SET STATEMENT max_statement_time = 30 FOR */ SELECT /*+ MAX_EXECUTION_TIME(30000) */ a FROM t");
        assert_eq!(my_time_limited("SHOW REPLICA STATUS", 30), "SHOW REPLICA STATUS");
    }

    /// MariaDB: performance_schema is off by default and some MySQL tables don't exist there at all — the hint says so
    #[test]
    fn performance_schema_hint_covers_mariadb() {
        let e = explain("settings", DbKind::Mysql, AppError::Msg("Table 'performance_schema.global_variables' doesn't exist".into()), None).to_string();
        assert!(e.contains("not available") && e.contains("MariaDB"), "{e}");
    }

    /// pg_stat_statements missing: only CREATE EXTENSION when the library is preloaded, else preload + restart first (#85)
    #[test]
    fn pg_stat_statements_hint_follows_shared_preload_libraries() {
        for loaded in ["pg_stat_statements", "rdsutils, pg_stat_statements", "auto_explain,\"pg_stat_statements\""] {
            let h = pg_stat_statements_hint(Some(loaded));
            assert!(h.contains("CREATE EXTENSION pg_stat_statements;") && !h.contains("restart") && h.contains("rds_superuser") && h.contains("cloudsqlsuperuser"), "{loaded}: {h}");
        }
        for not in ["", "rdsutils", "pg_stat_statements_ext"] {
            let h = pg_stat_statements_hint(Some(not));
            // The extension may already exist (only the library is missing): CREATE EXTENSION only if it isn't there yet
            assert!(h.contains("shared_preload_libraries") && h.contains("parameter group") && h.contains("CREATE EXTENSION") && h.contains("isn't there yet"), "{not}: {h}");
        }
        // Setting not readable (needs pg_read_all_settings): both steps, the first one conditional
        let h = pg_stat_statements_hint(None);
        assert!(h.contains("If ") && h.contains("parameter group") && h.contains("CREATE EXTENSION"), "{h}");
    }

    /// shared_preload_libraries is read only for a Slow Queries error about pg_stat_statements, not for every failure
    #[test]
    fn pg_stat_statements_missing_only_for_its_error() {
        let e = |m: &str| AppError::Msg(m.into());
        let missing = e("relation \"pg_stat_statements\" does not exist");
        assert!(pg_stat_statements_missing("slow_queries", DbKind::Postgres, &missing));
        assert!(!pg_stat_statements_missing("slow_queries", DbKind::Mysql, &missing));
        assert!(!pg_stat_statements_missing("sessions", DbKind::Postgres, &missing));
        assert!(!pg_stat_statements_missing("slow_queries", DbKind::Postgres, &e("connection reset by peer")));
    }

    /// pg_blocking_pids() is costly: only sessions in a lock wait get it
    #[test]
    fn sessions_call_pg_blocking_pids_only_for_lock_waits() {
        let q = DEFS.iter().find(|d| (d.info)().id == "sessions").unwrap().pg[0];
        assert_eq!(q.matches("pg_blocking_pids(").count(), 1, "{q}");
        let at = q.find("pg_blocking_pids(").unwrap();
        assert!(q[at..].split(')').nth(1).is_some_and(|rest| rest.contains("wait_event_type = 'Lock'")), "{q}");
    }

    fn cfg(kind: DbKind) -> Option<(crate::store::ConnectionConfig, Option<String>)> {
        let crate::testenv::Server { cfg: mut c, password } = crate::testenv::server(kind)?;
        c.id = "d".into();
        Some((c, password))
    }

    /// A connect failure that isn't about the database (here the server hangs up) stays as it is: no "pick another database"
    #[tokio::test]
    async fn report_keeps_a_server_connect_error() {
        // A server that hangs up on every connection
        let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = l.local_addr().unwrap().port();
        tokio::spawn(async move {
            while let Ok((s, _)) = l.accept().await {
                drop(s);
            }
        });
        let mut c = crate::testenv::parse_pg(&format!("127.0.0.1:{port}:nobody:shop")).cfg;
        c.id = "d".into();
        let dir = std::env::temp_dir().join(format!("qurry-dba-{}", uuid::Uuid::new_v4()));
        let st = crate::open_state(Some(dir)).unwrap();
        st.save_connection(c, None, None).await.unwrap();
        for target in ["d", "d::billing"] {
            let e = st.dba_report(target.into(), "table_sizes".into()).await.unwrap_err().to_string();
            assert!(!e.contains("Couldn't connect to database") && !e.contains("Pick another"), "{e}");
        }
    }

    async fn state(kind: DbKind) -> Option<AppState> {
        state_with(kind, "").await
    }

    async fn state_with(kind: DbKind, startup_script: &str) -> Option<AppState> {
        let (mut c, pass) = cfg(kind)?;
        c.options.startup_script = startup_script.into();
        let dir = std::env::temp_dir().join(format!("qurry-dba-{}", uuid::Uuid::new_v4()));
        let st = crate::open_state(Some(dir)).unwrap();
        st.save_connection(c, pass, None).await.unwrap();
        Some(st)
    }

    /// Every report runs, or tells what to enable if the feature is off (pg_stat_statements, etc.)
    async fn all_reports_run(kind: DbKind) {
        let Some(st) = state(kind).await else { return };
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
        all_reports_run(DbKind::Postgres).await;
    }

    #[tokio::test]
    #[ignore]
    async fn mysql_reports_run() {
        all_reports_run(DbKind::Mysql).await;
    }

    /// Table Sizes follows the target's database: a table of a second database shows there, not on the source's default one (#99)
    #[tokio::test]
    #[ignore]
    async fn pg_table_sizes_follow_the_database() {
        let Some(st) = state(DbKind::Postgres).await else { return };
        let q = |t: &str, s: &str| st.run_query(t.into(), s.into(), None, None);
        q("d", "drop database if exists qurry_dba_other").await.ok();
        q("d", "create database qurry_dba_other").await.unwrap();
        q("d::qurry_dba_other", "create table qurry_only_here (id int)").await.unwrap();
        let tables = |r: QueryResult| r.rows.iter().map(|x| crate::db::cell_text(&x[1])).collect::<Vec<_>>();
        let other = tables(st.dba_report("d::qurry_dba_other".into(), "table_sizes".into()).await.unwrap());
        let default = tables(st.dba_report("d".into(), "table_sizes".into()).await.unwrap());
        // Sessions is server-wide: it runs on either database
        st.dba_report("d::qurry_dba_other".into(), "sessions".into()).await.unwrap();
        let missing = st.dba_report("d::qurry_dba_missing".into(), "table_sizes".into()).await.unwrap_err().to_string();
        // An account without CONNECT on the source's default database: listing through that database fails, through another works
        // (the pickers list through the report's own database); the report names the refused database. Wrong password: unchanged
        q("d", "drop role if exists qurry_dba_nc").await.ok();
        q("d", "create role qurry_dba_nc login password 'nc'").await.unwrap();
        q("d", "revoke connect on database qurry_dba_other from public").await.unwrap();
        let (base, _) = cfg(DbKind::Postgres).unwrap();
        for (id, one, pass) in [("nc", false, "nc"), ("one", true, "nc"), ("bad", false, "wrong")] {
            let mut c = base.clone();
            (c.id, c.user, c.database, c.options.single_db) = (id.into(), "qurry_dba_nc".into(), "qurry_dba_other".into(), one);
            st.save_connection(c, Some(pass.into()), None).await.unwrap();
        }
        let listed = (st.list_databases("nc".into()).await.is_err(), st.list_databases("nc::postgres".into()).await.map(|d| d.contains(&"qurry_dba_other".to_string())));
        let refused = st.dba_report("nc".into(), "table_sizes".into()).await.unwrap_err().to_string();
        let one = st.dba_report("one".into(), "table_sizes".into()).await.unwrap_err().to_string();
        // A server with trust auth (CI) lets a wrong password in: then there is no error to check
        let bad = st.dba_report("bad::postgres".into(), "sessions".into()).await.err().map(|e| e.to_string());
        for id in ["d", "nc", "one", "bad"] {
            st.disconnect(id.into()).await.ok();
        }
        q("d", "drop database qurry_dba_other with (force)").await.ok();
        q("d", "drop role qurry_dba_nc").await.ok();
        assert!(other.contains(&"qurry_only_here".to_string()) && !default.contains(&"qurry_only_here".to_string()), "{other:?} / {default:?}");
        assert!(missing.starts_with("Couldn't connect to database qurry_dba_missing. Pick another database") && missing.contains("TUI: the report's database list)"), "{missing}");
        assert_eq!((listed.0, listed.1.ok()), (true, Some(true)));
        assert!(refused.starts_with("Couldn't connect to database qurry_dba_other. Pick another database"), "{refused}");
        assert!(one.starts_with("Couldn't connect to database qurry_dba_other.\n("), "{one}");
        assert!(bad.as_ref().is_none_or(|b| !b.contains("Couldn't connect to database")), "{bad:?}");
    }

    /// Server Overview works without performance_schema (MariaDB has no performance_schema.global_status)
    #[tokio::test]
    #[ignore]
    async fn mysql_server_overview_runs() {
        let Some(st) = state(DbKind::Mysql).await else { return };
        let r = st.dba_report("d".into(), "server".into()).await.unwrap();
        assert!(r.rows.iter().any(|x| crate::db::cell_text(&x[0]).starts_with("uptime") && !crate::db::cell_text(&x[1]).is_empty()), "{:?}", r.rows);
    }

    /// Users & Roles with a role valid until infinity (rdsadmin on RDS) shows "infinity" instead of panicking (#84)
    #[tokio::test]
    #[ignore]
    async fn pg_roles_report_with_infinity_role() {
        let Some(st) = state(DbKind::Postgres).await else { return };
        let q = |s: &str| st.run_query("d".into(), s.into(), None, None);
        q("drop role if exists qurry_inf_role").await.ok();
        q("create role qurry_inf_role login valid until 'infinity'").await.unwrap();
        let r = st.dba_report("d".into(), "roles".into()).await;
        q("drop role qurry_inf_role").await.ok();
        let r = r.unwrap();
        let col = |n: &str| r.columns.iter().position(|c| c.name == n).unwrap();
        let row = r.rows.iter().find(|x| crate::db::cell_text(&x[col("role")]) == "qurry_inf_role").expect("role in report");
        assert_eq!(crate::db::cell_text(&row[col("valid_until")]), "infinity");
    }

    /// Explain from Slow Queries: a generic plan on PG16+, otherwise a note to fill in the parameters
    #[tokio::test]
    #[ignore]
    async fn pg_explain_of_normalized_query() {
        let Some(st) = state(DbKind::Postgres).await else { return };
        let sql = st.dba_explain("d".into(), "select * from pg_class where oid = $1".into()).await.unwrap();
        let major = crate::backup::pg_major(&st, "d").await.unwrap();
        if major >= 16 {
            assert!(sql.starts_with("EXPLAIN (GENERIC_PLAN) "), "{sql}");
            // Runs in the console (its session) and on the pool
            for tab in [Some("t".to_string()), None] {
                let r = st.run_query("d".into(), sql.clone(), None, tab).await.unwrap();
                assert!(crate::db::cell_text(&r.rows[0][0]).contains("Scan"), "{:?}", r.rows);
            }
        } else {
            assert!(sql.starts_with("-- "), "{sql}");
        }
    }

    /// Slow Queries without pg_stat_statements says what this server needs: CREATE EXTENSION when the library is preloaded,
    /// else preload + restart first (#85). The shared test PG preloads nothing; a server started with
    /// -c shared_preload_libraries=pg_stat_statements checks the other case
    #[tokio::test]
    #[ignore]
    async fn pg_slow_queries_say_what_is_missing() {
        let Some(st) = state(DbKind::Postgres).await else { return };
        let q = |s: &str| st.run_query("d".into(), s.into(), None, None);
        let cell = |r: QueryResult| crate::db::cell_text(&r.rows[0][0]);
        // The extension is put back afterwards if the server had it
        let had = cell(q("select count(*) from pg_extension where extname = 'pg_stat_statements'").await.unwrap()) != "0";
        q("drop extension if exists pg_stat_statements").await.unwrap();
        let preload = cell(q("select current_setting('shared_preload_libraries')").await.unwrap());
        let e = st.dba_report("d".into(), "slow_queries".into()).await.unwrap_err().to_string();
        if had {
            q("create extension pg_stat_statements").await.unwrap();
        }
        assert!(e.contains("CREATE EXTENSION pg_stat_statements;"), "{e}");
        let loaded = preload.contains("pg_stat_statements");
        assert_eq!(e.contains("restart") && e.contains("parameter group"), !loaded, "{preload}: {e}");
    }

    /// Report queries stop at the time limit, which applies to that query only
    async fn report_time_limit(kind: DbKind, sleep: &str) {
        let Some(st) = state(kind).await else { return };
        let pool = crate::app::ensure_pool(&st, "d").await.unwrap();
        let t = std::time::Instant::now();
        let e = run_limited(&pool, kind, sleep, 1).await.expect_err("time limit");
        assert!(t.elapsed() < std::time::Duration::from_millis(2900), "{e}");
        let e = e.to_string().to_lowercase();
        assert!(e.contains("timeout") || e.contains("maximum statement execution time") || e.contains("max_statement_time"), "{e}");
        assert!(run_limited(&pool, kind, "SELECT 1", 1).await.is_ok());
        // A time limit ends the search for a working query: the next one would hide it
        let e = run_first(&pool, kind, &[sleep, "SELECT 1"], 1).await.expect_err("time limit");
        assert!(is_timeout(&e), "{e}");
        assert!(explain("x", kind, e, None).to_string().contains("time limit"));
    }

    /// The report limit only tightens: a stricter session limit (here from the startup script; a role or global setting works the same) stays
    async fn stricter_server_limit_stays(kind: DbKind, sleep: &str) {
        let Some(st) = state(kind).await else { return };
        let mariadb = kind == DbKind::Mysql && st.run_query("d".into(), "SELECT VERSION()".into(), None, None).await.unwrap().rows[0][0].to_string().to_lowercase().contains("mariadb");
        let script = match kind {
            DbKind::Postgres => "SET statement_timeout = '1s'",
            _ if mariadb => "SET SESSION max_statement_time = 1",
            _ => "SET SESSION max_execution_time = 1000",
        };
        let st = state_with(kind, script).await.unwrap();
        let pool = crate::app::ensure_pool(&st, "d").await.unwrap();
        let t = std::time::Instant::now();
        let e = run_limited(&pool, kind, sleep, 30).await.expect_err("server limit");
        assert!(t.elapsed() < std::time::Duration::from_millis(2900) && is_timeout(&e), "{e}");
    }

    #[tokio::test]
    #[ignore]
    async fn pg_report_time_limit() {
        report_time_limit(DbKind::Postgres, "SELECT pg_sleep(3)").await;
        stricter_server_limit_stays(DbKind::Postgres, "SELECT pg_sleep(3)").await;
        let Some(st) = state(DbKind::Postgres).await else { return };
        let r = st.run_query("d".into(), "SHOW statement_timeout".into(), None, None).await.unwrap();
        assert_eq!(crate::db::cell_text(&r.rows[0][0]), "0");
    }

    #[tokio::test]
    #[ignore]
    async fn mysql_report_time_limit() {
        // SLEEP() / BENCHMARK() cut short return a value instead of an error; a long join fails like a report query would
        let slow = "SELECT COUNT(*) FROM information_schema.COLUMNS a, information_schema.COLUMNS b, information_schema.COLUMNS c";
        report_time_limit(DbKind::Mysql, slow).await;
        stricter_server_limit_stays(DbKind::Mysql, slow).await;
    }

    /// Unused Indexes tells since when the scans were counted, right next to the scans (MySQL lists unused ones only, after the index name)
    async fn unused_indexes_show_counting_window(kind: DbKind) {
        let Some(st) = state(kind).await else { return };
        let q = |s: &str| st.run_query("d".into(), s.into(), None, None);
        q("drop table if exists qurry_dba_unused").await.ok();
        q("create table qurry_dba_unused (id int, v int)").await.unwrap();
        q("create index qurry_dba_unused_v on qurry_dba_unused (v)").await.unwrap();
        // MySQL counts index use for tables it has opened
        q("select * from qurry_dba_unused").await.unwrap();
        let r = st.dba_report("d".into(), "unused_indexes".into()).await;
        q("drop table qurry_dba_unused").await.ok();
        let r = match r {
            // MariaDB has performance_schema off by default
            Err(e) if kind == DbKind::Mysql && e.to_string().contains("performance_schema") => return,
            r => r.unwrap(),
        };
        let col = r.columns.iter().position(|c| c.name == "counting_since").expect("counting_since column");
        let before = r.columns.iter().position(|c| c.name == if kind == DbKind::Postgres { "scans" } else { "index" }).unwrap();
        assert_eq!(col, before + 1);
        let idx = r.columns.iter().position(|c| c.name == "index").unwrap();
        let row = r.rows.iter().find(|x| crate::db::cell_text(&x[idx]) == "qurry_dba_unused_v").expect("unused index");
        assert!(!crate::db::cell_text(&row[col]).is_empty());
    }

    #[tokio::test]
    #[ignore]
    async fn pg_unused_indexes_show_counting_window() {
        unused_indexes_show_counting_window(DbKind::Postgres).await;
    }

    #[tokio::test]
    #[ignore]
    async fn mysql_unused_indexes_show_counting_window() {
        unused_indexes_show_counting_window(DbKind::Mysql).await;
    }

    /// RDS MySQL: KILL of another user's thread is denied without CONNECTION_ADMIN → mysql.rds_kill / rds_kill_query when the server has them.
    /// Without them the KILL error stays. The procedures are created only on MariaDB (a throwaway test server)
    #[tokio::test]
    #[ignore]
    async fn mysql_kill_falls_back_to_rds_kill() {
        let Some(server) = crate::testenv::my() else { return };
        // Connected the way the app connects (testenv::my_conn: MySQL 5.7's TLS doesn't agree with rustls)
        let mut root = crate::testenv::my_conn(&server).await;
        let mut victim = crate::testenv::my_conn(&server).await;
        let c = crate::store::ConnectionConfig { id: "d".into(), ..server.cfg };
        let vid: u64 = sqlx::query_scalar("SELECT CONNECTION_ID()").fetch_one(&mut victim).await.unwrap();
        let mariadb = sqlx::query_scalar::<_, String>("SELECT VERSION()").fetch_one(&mut root).await.unwrap().to_lowercase().contains("mariadb");
        sqlx::raw_sql("DROP USER IF EXISTS 'qurry_dba_kill'@'%'; CREATE USER 'qurry_dba_kill'@'%' IDENTIFIED BY 'pw'").execute(&mut root).await.unwrap();
        let dir = std::env::temp_dir().join(format!("qurry-dba-{}", uuid::Uuid::new_v4()));
        let st = crate::open_state(Some(dir)).unwrap();
        st.save_connection(crate::store::ConnectionConfig { user: "qurry_dba_kill".into(), database: String::new(), ..c }, Some("pw".into()), None).await.unwrap();

        let e = st.kill_session("d".into(), vid as i64, true).await.expect_err("not owner");
        assert!(e.to_string().contains("not owner"), "{e}");
        if mariadb {
            sqlx::raw_sql(
                "CREATE OR REPLACE PROCEDURE mysql.rds_kill(IN thread BIGINT) SQL SECURITY DEFINER KILL thread;
                 CREATE OR REPLACE PROCEDURE mysql.rds_kill_query(IN thread BIGINT) SQL SECURITY DEFINER KILL QUERY thread;
                 GRANT EXECUTE ON PROCEDURE mysql.rds_kill TO 'qurry_dba_kill'@'%'; GRANT EXECUTE ON PROCEDURE mysql.rds_kill_query TO 'qurry_dba_kill'@'%'",
            )
            .execute(&mut root)
            .await
            .unwrap();
            assert!(st.kill_session("d".into(), vid as i64, false).await.unwrap());
            assert!(st.kill_session("d".into(), vid as i64, true).await.unwrap());
            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
            let left: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM information_schema.PROCESSLIST WHERE ID = ?").bind(vid).fetch_one(&mut root).await.unwrap();
            assert_eq!(left, 0, "victim connection was killed");
            sqlx::raw_sql("DROP PROCEDURE mysql.rds_kill; DROP PROCEDURE mysql.rds_kill_query").execute(&mut root).await.ok();
        }
        sqlx::raw_sql("DROP USER IF EXISTS 'qurry_dba_kill'@'%'").execute(&mut root).await.ok();
    }

    /// PROD safeguard: count the rows to change in advance (without changing them)
    #[tokio::test]
    #[ignore]
    async fn pg_preview_impact_counts_rows() {
        let Some(st) = state(DbKind::Postgres).await else { return };
        let q = |s: &str| st.run_query("d".into(), s.into(), None, None);
        q("drop table if exists qurry_impact").await.ok();
        q("create table qurry_impact as select g as id from generate_series(1, 100) g").await.unwrap();
        assert_eq!(st.preview_impact("d".into(), "delete from qurry_impact where id <= 30".into()).await.unwrap(), Some(30));
        assert_eq!(st.preview_impact("d".into(), "update qurry_impact set id = id".into()).await.unwrap(), Some(100));
        assert_eq!(st.preview_impact("d".into(), "select 1".into()).await.unwrap(), None);
        assert_eq!(q("select count(*) from qurry_impact").await.unwrap().rows[0][0], serde_json::json!(100));
        q("drop table qurry_impact").await.ok();
    }

    /// Deleted rows taking up space until VACUUM show up as table/index bloat
    #[tokio::test]
    #[ignore]
    async fn pg_bloat_shows_deleted_space() {
        let Some(st) = state(DbKind::Postgres).await else { return };
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

    /// VACUUM FULL (pg_stat_progress_cluster) shows in Vacuum Progress too
    #[tokio::test]
    #[ignore]
    async fn pg_vacuum_full_shows_in_progress() {
        let Some(st) = state(DbKind::Postgres).await else { return };
        let (c, _) = cfg(DbKind::Postgres).unwrap();
        let other = sqlx::postgres::PgPoolOptions::new().max_connections(1).connect(&format!("postgres://{}@{}:{}/{}", c.user, c.host, c.port, c.database)).await.unwrap();
        sqlx::raw_sql("drop table if exists qurry_vfull; create table qurry_vfull as select g as id, repeat('x', 100) as pad from generate_series(1, 3000000) g").execute(&other).await.unwrap();
        let full = tokio::spawn(async move { sqlx::raw_sql("vacuum full qurry_vfull").execute(&other).await.map(|_| other) });
        let mut seen = None;
        for _ in 0..200 {
            let r = st.dba_report("d".into(), "vacuum_progress".into()).await.unwrap();
            let col = |n: &str| r.columns.iter().position(|c| c.name == n).unwrap();
            if let Some(row) = r.rows.iter().find(|x| crate::db::cell_text(&x[col("table")]) == "public.qurry_vfull") {
                seen = Some(crate::db::cell_text(&row[col("phase")]));
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
        let other = full.await.unwrap().unwrap();
        sqlx::raw_sql("drop table qurry_vfull").execute(&other).await.ok();
        assert!(seen.as_deref().is_some_and(|p| p.starts_with("VACUUM FULL: ")), "{seen:?}");
    }

    /// The lock-holding session and the waiting session both show, and killing the holder lets the waiter finish
    #[tokio::test]
    #[ignore]
    async fn pg_blocking_session_is_shown_and_killed() {
        let Some(st) = state(DbKind::Postgres).await else { return };
        let (c, _) = cfg(DbKind::Postgres).unwrap();
        let url = format!("postgres://{}@{}:{}/{}", c.user, c.host, c.port, c.database);
        let a = sqlx::postgres::PgPoolOptions::new().max_connections(1).connect(&url).await.unwrap();
        let b = sqlx::postgres::PgPoolOptions::new().max_connections(1).connect(&url).await.unwrap();
        let waiting: i32 = sqlx::query_scalar("select pg_backend_pid()").fetch_one(&b).await.unwrap();
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
        // The report is server-wide: tests running at the same time (audit, session) hold and wait for locks of their own
        let l = st.dba_report("d".into(), "locks".into()).await.unwrap();
        let lc = |n: &str| l.columns.iter().position(|c| c.name == n).unwrap();
        let ours: Vec<(String, String)> = l
            .rows
            .iter()
            .map(|r| (crate::db::cell_text(&r[lc("pid")]), crate::db::cell_text(&r[lc("waiting_pid")])))
            .filter(|(p, w)| *p == holder.to_string() || *w == waiting.to_string())
            .collect();
        assert_eq!(ours, [(holder.to_string(), waiting.to_string())], "{:?}", l.rows);

        assert!(st.kill_session("d".into(), holder as i64, true).await.unwrap());
        let r = tokio::time::timeout(std::time::Duration::from_secs(5), waiter).await.expect("waiter finished").unwrap();
        assert!(r.is_ok());
        drop(ta);
        sqlx::raw_sql("drop table if exists qurry_dba_lock").execute(&a).await.ok();
    }
}

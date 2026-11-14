# Roadmap

Eight phases. Each ends with something runnable and has a brief in `docs/phases/` with tasks, tests and acceptance criteria.

## Status

- [ ] [Phase 1: Skeleton](phases/phase-1-skeleton.md) — workspace, core traits, SQLite driver, app shell that runs a query
- [ ] [Phase 2: PostgreSQL and MySQL](phases/phase-2-postgres-mysql.md) — both drivers, TLS, keychain, cancellation
- [ ] [Phase 3: SSH tunnels](phases/phase-3-ssh-tunnels.md) — port forwarding, host key verification, jump host
- [ ] [Phase 4: Schema browser](phases/phase-4-schema-browser.md) — tree, table details, DDL
- [ ] [Phase 5: Editor](phases/phase-5-editor.md) — highlighting, autocomplete, run under cursor, history
- [ ] [Phase 6: Data viewing](phases/phase-6-data-viewing.md) — table grid, sort, filter, JSON view, cell viewer
- [ ] [Phase 7: Data editing](phases/phase-7-data-editing.md) — staged edits, SQL preview, safeguards
- [ ] [Phase 8: Polish](phases/phase-8-polish.md) — export, saved queries, shortcuts, packaging

## Order and dependencies

```
1 ──► 2 ──► 3
      │
      └──► 4 ──► 5
           │
           └──► 6 ──► 7 ──► 8
```

Phases 3, 5 and 6 can be reordered among themselves once their predecessors are done. Phase 1 and 2 come first because they prove the driver abstraction, which is the most expensive thing to get wrong.

## Requirement coverage

| Requirement (PRD) | Phase |
|---|---|
| FR-1 Connections | 1 (SQLite), 2 (PostgreSQL, MySQL, TLS, keychain), 3 (SSH) |
| FR-2 Editor | 1 (basic run), 2 (cancel), 5 (everything else) |
| FR-3 Schema browser | 4 |
| FR-4 Data viewing | 6 |
| FR-5 Data editing | 7 |
| FR-6 Safeguards | 2 (read-only session), 5 (confirmation for dangerous statements), 7 (staged edits) |
| FR-7 Export | 8 |

## Risks to retire early

| Risk | Where it is retired |
|---|---|
| PostgreSQL decoding of arbitrary types | Phase 2 spike, first task |
| Abandoning a row stream at the row limit | Phase 2 |
| `ts-rs` and the tagged `Value` type | Phase 1 |
| `russh` API for forwarding and known hosts | Phase 3 spike, first task |
| Grid performance and editing ergonomics | Phase 6 spike, first task |

## Deferred (after v1)

Roughly in order of likely value:

1. Editing results of simple single-table ad-hoc queries
2. Manual transaction mode with a visible transaction indicator and commit/rollback buttons
3. Visual `EXPLAIN`
4. Reading `~/.ssh/config`
5. Keyset pagination for large tables
6. Import from CSV
7. Profile import/export
8. MySQL `DELIMITER` support in the splitter
9. SQLite `rowid` addressing for tables without a primary key
10. More drivers (MariaDB-specific features, SQL Server, DuckDB, ClickHouse)
11. ER diagrams, schema diff
12. Linux and Windows packaging

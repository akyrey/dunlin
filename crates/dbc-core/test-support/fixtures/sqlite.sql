-- Shared logical fixture (SQLite flavour). postgres.sql and mysql.sql mirror this content.
-- Covers every type category, awkward identifiers, a table without a primary key,
-- a composite key, a foreign key, unique and check constraints, an index and a view.

CREATE TABLE customers (
    id         INTEGER PRIMARY KEY,
    name       TEXT NOT NULL,
    email      TEXT UNIQUE,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE orders (
    id          INTEGER PRIMARY KEY,
    customer_id INTEGER NOT NULL REFERENCES customers (id) ON DELETE CASCADE,
    total       NUMERIC(12, 2) CHECK (total >= 0),
    placed_on   DATE,
    paid        BOOLEAN NOT NULL DEFAULT 0
);

CREATE INDEX orders_customer_idx ON orders (customer_id);

CREATE TABLE order_items (
    order_id INTEGER NOT NULL REFERENCES orders (id),
    line     INTEGER NOT NULL,
    sku      TEXT NOT NULL,
    quantity INTEGER NOT NULL DEFAULT 1,
    PRIMARY KEY (order_id, line)
);

CREATE TABLE all_types (
    id        INTEGER PRIMARY KEY,
    c_bool    BOOLEAN,
    c_int     INTEGER,
    c_bigint  BIGINT,
    c_real    REAL,
    c_decimal NUMERIC,
    c_text    TEXT,
    c_blob    BLOB,
    c_json    JSON,
    c_xml     XML,
    c_uuid    UUID,
    c_date    DATE,
    c_ts      TIMESTAMP
);

-- No primary key and no unique not-null key: read-only in the grid.
CREATE TABLE event_log (
    happened_at TIMESTAMP,
    message     TEXT
);

CREATE TABLE "Mixed Case ""Quoted""" (
    "select"      INTEGER PRIMARY KEY,
    "spaced name" TEXT,
    "naïve"       TEXT
);

CREATE VIEW paid_orders AS SELECT id, customer_id, total FROM orders WHERE paid = 1;

-- Introspection (Phase 4): self-referencing and composite foreign keys with actions, a named
-- check, a multi-column unique constraint, a generated column, multi-column, partial and
-- expression indexes, comments, a quoted index name, and a table keyed by a unique not-null
-- column.
CREATE TABLE employees (
    id         INTEGER PRIMARY KEY,
    manager_id INTEGER REFERENCES employees (id) ON DELETE SET NULL,
    first_name TEXT NOT NULL,
    last_name  TEXT NOT NULL,
    full_name  TEXT GENERATED ALWAYS AS (first_name || ' ' || last_name) STORED,
    salary     NUMERIC(10, 2) CONSTRAINT salary_positive CHECK (salary > 0),
    CONSTRAINT employees_name_key UNIQUE (last_name, first_name)
);
CREATE INDEX employees_manager_name_idx ON employees (manager_id, last_name);
CREATE INDEX employees_high_paid_idx ON employees (salary) WHERE salary > 100000;
CREATE INDEX employees_lower_last_idx ON employees (lower(last_name));

CREATE TABLE shipments (
    id       INTEGER PRIMARY KEY,
    order_id INTEGER NOT NULL,
    line     INTEGER NOT NULL,
    CONSTRAINT shipments_item_fk FOREIGN KEY (order_id, line)
        REFERENCES order_items (order_id, line) ON DELETE CASCADE ON UPDATE CASCADE
);
CREATE INDEX shipments_item_idx ON shipments (order_id, line);

-- No primary key; the unique not-null column addresses rows.
CREATE TABLE tags (
    code  TEXT NOT NULL UNIQUE,
    label TEXT
);

CREATE INDEX "Mixed ""idx""" ON "Mixed Case ""Quoted""" ("spaced name");

-- SQLite only: a WITHOUT ROWID table.
CREATE TABLE settings (
    key   TEXT PRIMARY KEY,
    value TEXT
) WITHOUT ROWID;

INSERT INTO customers (id, name, email) VALUES
    (1, 'Ada', 'ada@example.com'),
    (2, 'Grace', NULL),
    (3, '', 'empty-name@example.com');

INSERT INTO orders (id, customer_id, total, placed_on, paid) VALUES
    (10, 1, 12.50, '2026-01-02', 1),
    (11, 1, 0, '2026-01-03', 0),
    (12, 2, 99999999.99, NULL, 1);

INSERT INTO order_items (order_id, line, sku, quantity) VALUES
    (10, 1, 'A-1', 2),
    (10, 2, 'B-7', 1),
    (12, 1, 'A-1', 5);

INSERT INTO all_types VALUES (
    1, 1, 42, 9223372036854775807, 1.5, '12345678901234567890.123456789', 'héllo',
    X'DEADBEEF', '{"a": [1, 2]}', '<a>b</a>', '6f1c2d3e-0000-4000-8000-000000000001',
    '2026-11-14', '2026-11-14 10:00:00'
);
INSERT INTO all_types (id) VALUES (2);

INSERT INTO event_log VALUES ('2026-11-14 09:00:00', 'started'), (NULL, NULL);

INSERT INTO "Mixed Case ""Quoted""" VALUES (1, 'a b', 'ï');

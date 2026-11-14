-- Shared logical fixture (PostgreSQL flavour). Mirrors sqlite.sql.

CREATE TABLE customers (
    id         integer PRIMARY KEY,
    name       text NOT NULL,
    email      text UNIQUE,
    created_at timestamptz DEFAULT now()
);

CREATE TABLE orders (
    id          integer PRIMARY KEY,
    customer_id integer NOT NULL REFERENCES customers (id) ON DELETE CASCADE,
    total       numeric(12, 2) CHECK (total >= 0),
    placed_on   date,
    paid        boolean NOT NULL DEFAULT false
);

CREATE INDEX orders_customer_idx ON orders (customer_id);

CREATE TABLE order_items (
    order_id integer NOT NULL REFERENCES orders (id),
    line     integer NOT NULL,
    sku      text NOT NULL,
    quantity integer NOT NULL DEFAULT 1,
    PRIMARY KEY (order_id, line)
);

CREATE TABLE all_types (
    id        integer PRIMARY KEY,
    c_bool    boolean,
    c_int     integer,
    c_bigint  bigint,
    c_real    double precision,
    c_decimal numeric,
    c_text    text,
    c_blob    bytea,
    c_json    jsonb,
    c_xml     xml,
    c_uuid    uuid,
    c_date    date,
    c_ts      timestamp
);

-- No primary key and no unique not-null key: read-only in the grid.
CREATE TABLE event_log (
    happened_at timestamp,
    message     text
);

CREATE TABLE "Mixed Case ""Quoted""" (
    "select"      integer PRIMARY KEY,
    "spaced name" text,
    "naïve"       text
);

CREATE VIEW paid_orders AS SELECT id, customer_id, total FROM orders WHERE paid;

INSERT INTO customers (id, name, email) VALUES
    (1, 'Ada', 'ada@example.com'),
    (2, 'Grace', NULL),
    (3, '', 'empty-name@example.com');

INSERT INTO orders (id, customer_id, total, placed_on, paid) VALUES
    (10, 1, 12.50, '2026-01-02', true),
    (11, 1, 0, '2026-01-03', false),
    (12, 2, 99999999.99, NULL, true);

INSERT INTO order_items (order_id, line, sku, quantity) VALUES
    (10, 1, 'A-1', 2),
    (10, 2, 'B-7', 1),
    (12, 1, 'A-1', 5);

INSERT INTO all_types VALUES (
    1, true, 42, 9223372036854775807, 1.5, 12345678901234567890.123456789, 'héllo',
    '\xDEADBEEF', '{"a": [1, 2]}', '<a>b</a>', '6f1c2d3e-0000-4000-8000-000000000001',
    '2026-11-14', '2026-11-14 10:00:00'
);
INSERT INTO all_types (id) VALUES (2);

INSERT INTO event_log VALUES ('2026-11-14 09:00:00', 'started'), (NULL, NULL);

INSERT INTO "Mixed Case ""Quoted""" VALUES (1, 'a b', 'ï');

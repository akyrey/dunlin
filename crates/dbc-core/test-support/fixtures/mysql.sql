-- Shared logical fixture (MySQL/MariaDB flavour). Mirrors sqlite.sql.

CREATE TABLE customers (
    id         INT PRIMARY KEY,
    name       VARCHAR(200) NOT NULL,
    email      VARCHAR(200) UNIQUE,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE orders (
    id          INT PRIMARY KEY,
    customer_id INT NOT NULL,
    total       DECIMAL(12, 2) CHECK (total >= 0),
    placed_on   DATE,
    paid        BOOLEAN NOT NULL DEFAULT FALSE,
    INDEX orders_customer_idx (customer_id),
    FOREIGN KEY (customer_id) REFERENCES customers (id) ON DELETE CASCADE
);

CREATE TABLE order_items (
    order_id INT NOT NULL,
    line     INT NOT NULL,
    sku      VARCHAR(50) NOT NULL,
    quantity INT NOT NULL DEFAULT 1,
    PRIMARY KEY (order_id, line),
    FOREIGN KEY (order_id) REFERENCES orders (id)
);

CREATE TABLE all_types (
    id        INT PRIMARY KEY,
    c_bool    BOOLEAN,
    c_int     INT,
    c_bigint  BIGINT,
    c_real    DOUBLE,
    c_decimal DECIMAL(38, 9),
    c_text    TEXT,
    c_blob    BLOB,
    c_json    JSON,
    c_xml     TEXT,
    c_uuid    CHAR(36),
    c_date    DATE,
    c_ts      DATETIME
);

-- No primary key and no unique not-null key: read-only in the grid.
CREATE TABLE event_log (
    happened_at DATETIME,
    message     TEXT
);

CREATE TABLE `Mixed Case "Quoted"` (
    `select`      INT PRIMARY KEY,
    `spaced name` TEXT,
    `naïve`       TEXT
);

CREATE VIEW paid_orders AS SELECT id, customer_id, total FROM orders WHERE paid;

INSERT INTO customers (id, name, email) VALUES
    (1, 'Ada', 'ada@example.com'),
    (2, 'Grace', NULL),
    (3, '', 'empty-name@example.com');

INSERT INTO orders (id, customer_id, total, placed_on, paid) VALUES
    (10, 1, 12.50, '2026-01-02', TRUE),
    (11, 1, 0, '2026-01-03', FALSE),
    (12, 2, 99999999.99, NULL, TRUE);

INSERT INTO order_items (order_id, line, sku, quantity) VALUES
    (10, 1, 'A-1', 2),
    (10, 2, 'B-7', 1),
    (12, 1, 'A-1', 5);

INSERT INTO all_types VALUES (
    1, TRUE, 42, 9223372036854775807, 1.5, 12345678901234567890.123456789, 'héllo',
    X'DEADBEEF', '{"a": [1, 2]}', '<a>b</a>', '6f1c2d3e-0000-4000-8000-000000000001',
    '2026-11-14', '2026-11-14 10:00:00'
);
INSERT INTO all_types (id) VALUES (2);

INSERT INTO event_log VALUES ('2026-11-14 09:00:00', 'started'), (NULL, NULL);

INSERT INTO `Mixed Case "Quoted"` VALUES (1, 'a b', 'ï');

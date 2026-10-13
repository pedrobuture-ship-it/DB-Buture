DROP SCHEMA IF EXISTS shop CASCADE;
CREATE SCHEMA shop;
SET search_path = shop;

CREATE TABLE users (
    id         bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    email      varchar(255) NOT NULL UNIQUE,
    name       text,
    tags       text[],
    meta       jsonb,
    created_at timestamptz NOT NULL DEFAULT now()
);
COMMENT ON TABLE users IS 'Registered customers';
COMMENT ON COLUMN users.email IS 'Login e-mail';

CREATE TABLE orders (
    id      bigserial PRIMARY KEY,
    user_id bigint REFERENCES users(id) ON DELETE CASCADE ON UPDATE RESTRICT,
    total   numeric(12,2) NOT NULL CHECK (total >= 0),
    net     numeric(12,2) GENERATED ALWAYS AS (total * 0.9) STORED
);

CREATE TABLE order_items (
    order_id bigint NOT NULL REFERENCES orders(id),
    line     int NOT NULL,
    sku      text NOT NULL,
    qty      int NOT NULL DEFAULT 1,
    PRIMARY KEY (order_id, line),
    UNIQUE (order_id, sku)
);

CREATE TABLE item_notes (
    order_id bigint NOT NULL,
    line     int NOT NULL,
    note     text,
    FOREIGN KEY (order_id, line) REFERENCES order_items(order_id, line)
);

CREATE TABLE events (
    id bigint NOT NULL,
    at timestamptz NOT NULL,
    payload jsonb
) PARTITION BY RANGE (at);

CREATE INDEX idx_orders_user ON orders (user_id);
CREATE INDEX idx_users_lower_email ON users (lower(email)) WHERE name IS NOT NULL;
CREATE INDEX idx_users_meta ON users USING gin (meta);
CREATE INDEX idx_users_created ON users USING brin (created_at);
CREATE INDEX idx_users_name_hash ON users USING hash (name);
CREATE INDEX idx_items_cover ON order_items (order_id) INCLUDE (qty);

CREATE VIEW order_totals AS
    SELECT u.id AS user_id, sum(o.total) AS total FROM users u JOIN orders o ON o.user_id = u.id GROUP BY u.id;
CREATE VIEW big_spenders AS SELECT user_id FROM order_totals WHERE total > 1000;
CREATE MATERIALIZED VIEW daily_orders AS SELECT count(*) AS n FROM orders;

CREATE FUNCTION touch_user() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN NEW.name := coalesce(NEW.name, 'anon'); RETURN NEW; END $$;
CREATE TRIGGER trg_users_touch BEFORE INSERT OR UPDATE ON users FOR EACH ROW EXECUTE FUNCTION touch_user();
CREATE PROCEDURE noop_proc() LANGUAGE sql AS $$ SELECT 1 $$;
CREATE SEQUENCE standalone_seq;

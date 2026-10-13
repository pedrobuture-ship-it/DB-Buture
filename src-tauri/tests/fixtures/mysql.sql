DROP DATABASE IF EXISTS shop_fixture;
CREATE DATABASE shop_fixture;
USE shop_fixture;

CREATE TABLE users (
  id BIGINT UNSIGNED AUTO_INCREMENT PRIMARY KEY,
  email VARCHAR(255) NOT NULL,
  name VARCHAR(80) NULL COMMENT 'Display name',
  created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
  UNIQUE KEY uq_users_email (email)
) COMMENT='Registered customers';

CREATE TABLE orders (
  id BIGINT UNSIGNED AUTO_INCREMENT PRIMARY KEY,
  user_id BIGINT UNSIGNED NULL,
  total DECIMAL(12,2) NOT NULL,
  net DECIMAL(12,2) AS (total * 0.9) STORED,
  CONSTRAINT chk_total CHECK (total >= 0),
  CONSTRAINT fk_orders_user FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE ON UPDATE RESTRICT,
  KEY idx_orders_user (user_id)
);

CREATE TABLE order_items (
  order_id BIGINT UNSIGNED NOT NULL,
  line INT NOT NULL,
  sku VARCHAR(40) NOT NULL,
  PRIMARY KEY (order_id, line),
  UNIQUE KEY uq_items_sku (order_id, sku),
  CONSTRAINT fk_items_order FOREIGN KEY (order_id) REFERENCES orders(id)
);

CREATE TABLE item_notes (
  order_id BIGINT UNSIGNED NOT NULL,
  line INT NOT NULL,
  note TEXT,
  CONSTRAINT fk_notes_item FOREIGN KEY (order_id, line) REFERENCES order_items(order_id, line)
);

CREATE INDEX idx_users_lower ON users ((lower(email)));
CREATE FULLTEXT INDEX ft_users_name ON users (name);

CREATE VIEW order_totals AS SELECT u.id AS user_id, SUM(o.total) AS total FROM users u JOIN orders o ON o.user_id = u.id GROUP BY u.id;
CREATE VIEW big_spenders AS SELECT user_id FROM order_totals WHERE total > 1000;

CREATE TRIGGER trg_orders_bi BEFORE INSERT ON orders FOR EACH ROW SET NEW.total = IFNULL(NEW.total, 0);
CREATE FUNCTION tax(x DECIMAL(10,2)) RETURNS DECIMAL(10,2) DETERMINISTIC RETURN x * 0.2;

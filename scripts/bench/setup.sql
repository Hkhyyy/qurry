-- The benchmark table: 1,000,000 made-up shop orders (about 300 MB with its index), the same rows on every run (no random()).
CREATE SCHEMA shop;
CREATE TABLE shop.orders (
  id          bigint PRIMARY KEY,
  customer    text        NOT NULL,
  email       text        NOT NULL,
  status      text        NOT NULL,
  amount      numeric(12,2) NOT NULL,
  currency    char(3)     NOT NULL,
  created_at  timestamptz NOT NULL,
  shipped_at  timestamptz,
  note        text,
  items       jsonb       NOT NULL
);
INSERT INTO shop.orders
SELECT i,
       'Acme Customer ' || (i % 50000),
       'customer' || (i % 50000) || '@shop.example',
       (ARRAY['new','paid','shipped','delivered','returned'])[1 + i % 5],
       round(((i * 7919) % 100000) / 100.0, 2),
       (ARRAY['USD','EUR','KRW'])[1 + i % 3],
       timestamptz '2025-01-01 00:00:00+00' + (i * interval '31 seconds'),
       CASE WHEN i % 5 >= 2 THEN timestamptz '2025-01-02 00:00:00+00' + (i * interval '31 seconds') END,
       CASE WHEN i % 4 = 0 THEN 'Leave at the front desk of Acme building ' || (i % 977) END,
       jsonb_build_array(
         jsonb_build_object('sku', 'ACME-' || (i % 1000), 'qty', 1 + i % 3, 'price', (i % 500) + 0.99),
         jsonb_build_object('sku', 'ACME-' || ((i * 31) % 1000), 'qty', 1, 'price', (i % 120) + 0.49))
FROM generate_series(1::bigint, 1000000) AS i;
ANALYZE shop.orders;

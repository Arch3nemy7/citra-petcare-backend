-- Design revisions: owners are optional on patients (deleting an owner
-- detaches their pets), owner phones are optional, vaccinations may be
-- recorded as due-only (no administration date yet), and stock-in movements
-- carry per-batch expiry/lot data so the app can show a FEFO batch list.

ALTER TABLE owners ALTER COLUMN phone DROP NOT NULL;

ALTER TABLE patients ALTER COLUMN owner_id DROP NOT NULL;

ALTER TABLE vaccinations ALTER COLUMN date_given DROP NOT NULL;

-- A stock-in with an expiry date opens a batch; remaining quantities are
-- derived by allocating consumption to batches earliest-expiry-first (FEFO).
ALTER TABLE stock_movements
    ADD COLUMN expiry_date date,
    ADD COLUMN lot_no      text;

-- Package/label photos captured when registering a new item (storage keys).
ALTER TABLE inventory_items
    ADD COLUMN photo_keys text[] NOT NULL DEFAULT '{}';

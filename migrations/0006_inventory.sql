CREATE TYPE inventory_category AS ENUM ('DRUG', 'VACCINE', 'SUPPLY');
CREATE TYPE movement_type AS ENUM ('IN', 'OUT', 'ADJUSTMENT');

CREATE TABLE inventory_items (
    id          uuid PRIMARY KEY,
    name        text               NOT NULL,
    category    inventory_category NOT NULL,
    unit        text               NOT NULL, -- botol / vial / pcs / ml ...
    min_stock   double precision   NOT NULL DEFAULT 0,
    expiry_date date,
    created_at  timestamptz        NOT NULL DEFAULT now(),
    updated_at  timestamptz        NOT NULL DEFAULT now(),
    deleted_at  timestamptz
);

CREATE INDEX inventory_items_name_idx ON inventory_items (lower(name)) WHERE deleted_at IS NULL;
CREATE INDEX inventory_items_expiry_idx ON inventory_items (expiry_date)
    WHERE deleted_at IS NULL AND expiry_date IS NOT NULL;
CREATE INDEX inventory_items_updated_at_idx ON inventory_items (updated_at);

CREATE TRIGGER inventory_items_set_updated_at
    BEFORE UPDATE ON inventory_items
    FOR EACH ROW EXECUTE FUNCTION set_updated_at();

-- Current stock is never stored — it is derived as
--   SUM(CASE type WHEN 'IN' THEN qty WHEN 'OUT' THEN -qty ELSE qty END)
-- over this table. IN/OUT quantities must be positive; ADJUSTMENT may be
-- signed (e.g. -2 after a recount or breakage).
CREATE TABLE stock_movements (
    id         uuid PRIMARY KEY,
    item_id    uuid             NOT NULL REFERENCES inventory_items (id),
    type       movement_type    NOT NULL,
    qty        double precision NOT NULL,
    reason     text,
    visit_id   uuid             REFERENCES visits (id), -- set when stock was used during a visit
    created_at timestamptz      NOT NULL DEFAULT now(),
    updated_at timestamptz      NOT NULL DEFAULT now(),
    deleted_at timestamptz,
    CONSTRAINT stock_movements_qty_check
        CHECK (qty <> 0 AND (type = 'ADJUSTMENT' OR qty > 0))
);

-- Covering index: the stock aggregate reads (item_id, type, qty) only, so the
-- SUM is answered from this index without touching the heap.
CREATE INDEX stock_movements_item_id_idx ON stock_movements (item_id)
    INCLUDE (type, qty) WHERE deleted_at IS NULL;
CREATE INDEX stock_movements_updated_at_idx ON stock_movements (updated_at);

CREATE TRIGGER stock_movements_set_updated_at
    BEFORE UPDATE ON stock_movements
    FOR EACH ROW EXECUTE FUNCTION set_updated_at();

-- Shared trigger: bump updated_at on every UPDATE.
-- updated_at is server-authoritative because the offline-sync endpoint
-- (/api/v1/sync/changes?since=) uses it as the change cursor.
CREATE OR REPLACE FUNCTION set_updated_at() RETURNS trigger
LANGUAGE plpgsql AS
$$
BEGIN
    NEW.updated_at := now();
    RETURN NEW;
END;
$$;

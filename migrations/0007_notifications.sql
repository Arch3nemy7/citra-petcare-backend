-- Reminder/notification log. Rows are produced by the daily scheduler job and
-- dispatched through the Notifier trait (FCM in production, log in dev).
CREATE TABLE notifications (
    id         uuid PRIMARY KEY,
    type       text        NOT NULL, -- VACCINATION_DUE | LOW_STOCK | EXPIRY_WARNING | ...
    title      text        NOT NULL,
    body       text        NOT NULL,
    payload    jsonb       NOT NULL DEFAULT '{}'::jsonb,
    sent_at    timestamptz,          -- set when the Notifier dispatch succeeded
    read_at    timestamptz,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    deleted_at timestamptz
);

CREATE INDEX notifications_created_at_idx ON notifications (created_at DESC) WHERE deleted_at IS NULL;
-- the scheduler's once-per-day dedup check filters on (type, created_at)
CREATE INDEX notifications_type_created_at_idx ON notifications (type, created_at DESC);

CREATE TRIGGER notifications_set_updated_at
    BEFORE UPDATE ON notifications
    FOR EACH ROW EXECUTE FUNCTION set_updated_at();

CREATE TYPE appointment_status AS ENUM ('SCHEDULED', 'DONE', 'CANCELLED', 'NO_SHOW');

CREATE TABLE appointments (
    id           uuid PRIMARY KEY,
    patient_id   uuid               NOT NULL REFERENCES patients (id),
    scheduled_at timestamptz        NOT NULL,
    reason       text               NOT NULL,
    status       appointment_status NOT NULL DEFAULT 'SCHEDULED',
    notes        text,
    created_at   timestamptz        NOT NULL DEFAULT now(),
    updated_at   timestamptz        NOT NULL DEFAULT now(),
    deleted_at   timestamptz
);

CREATE INDEX appointments_scheduled_at_idx ON appointments (scheduled_at) WHERE deleted_at IS NULL;
CREATE INDEX appointments_patient_id_idx ON appointments (patient_id) WHERE deleted_at IS NULL;
CREATE INDEX appointments_updated_at_idx ON appointments (updated_at);

CREATE TRIGGER appointments_set_updated_at
    BEFORE UPDATE ON appointments
    FOR EACH ROW EXECUTE FUNCTION set_updated_at();

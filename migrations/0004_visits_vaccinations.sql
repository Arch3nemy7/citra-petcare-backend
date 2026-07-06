CREATE TYPE attachment_kind AS ENUM ('PHOTO', 'XRAY', 'LAB', 'OTHER');

CREATE TABLE visits (
    id             uuid PRIMARY KEY,
    patient_id     uuid        NOT NULL REFERENCES patients (id),
    vet_id         uuid        NOT NULL REFERENCES users (id),
    visit_date     timestamptz NOT NULL,
    complaint      text        NOT NULL, -- anamnesis
    temperature_c  double precision,
    weight_kg      double precision,     -- weight history is derived from this column
    exam_notes     text,
    diagnosis      text,
    treatment      text,
    prescription   text,
    follow_up_date date,
    created_at     timestamptz NOT NULL DEFAULT now(),
    updated_at     timestamptz NOT NULL DEFAULT now(),
    deleted_at     timestamptz
);

CREATE INDEX visits_patient_id_idx ON visits (patient_id, visit_date DESC) WHERE deleted_at IS NULL;
CREATE INDEX visits_updated_at_idx ON visits (updated_at);

CREATE TRIGGER visits_set_updated_at
    BEFORE UPDATE ON visits
    FOR EACH ROW EXECUTE FUNCTION set_updated_at();

-- Files live in object storage; rows only reference the storage key.
CREATE TABLE visit_attachments (
    id         uuid PRIMARY KEY,
    visit_id   uuid            NOT NULL REFERENCES visits (id),
    file_key   text            NOT NULL,
    kind       attachment_kind NOT NULL DEFAULT 'OTHER',
    created_at timestamptz     NOT NULL DEFAULT now(),
    updated_at timestamptz     NOT NULL DEFAULT now(),
    deleted_at timestamptz
);

CREATE INDEX visit_attachments_visit_id_idx ON visit_attachments (visit_id) WHERE deleted_at IS NULL;
CREATE INDEX visit_attachments_updated_at_idx ON visit_attachments (updated_at);

CREATE TRIGGER visit_attachments_set_updated_at
    BEFORE UPDATE ON visit_attachments
    FOR EACH ROW EXECUTE FUNCTION set_updated_at();

CREATE TABLE vaccinations (
    id            uuid PRIMARY KEY,
    patient_id    uuid        NOT NULL REFERENCES patients (id),
    visit_id      uuid        REFERENCES visits (id), -- optional link to the visit it was given at
    vaccine_name  text        NOT NULL,
    date_given    date        NOT NULL,
    batch_no      text,
    next_due_date date,
    created_at    timestamptz NOT NULL DEFAULT now(),
    updated_at    timestamptz NOT NULL DEFAULT now(),
    deleted_at    timestamptz
);

CREATE INDEX vaccinations_patient_id_idx ON vaccinations (patient_id) WHERE deleted_at IS NULL;
CREATE INDEX vaccinations_next_due_idx ON vaccinations (next_due_date)
    WHERE deleted_at IS NULL AND next_due_date IS NOT NULL;
CREATE INDEX vaccinations_updated_at_idx ON vaccinations (updated_at);

CREATE TRIGGER vaccinations_set_updated_at
    BEFORE UPDATE ON vaccinations
    FOR EACH ROW EXECUTE FUNCTION set_updated_at();

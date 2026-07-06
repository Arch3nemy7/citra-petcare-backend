CREATE TYPE species AS ENUM ('CAT', 'DOG', 'RABBIT', 'BIRD', 'HAMSTER', 'REPTILE', 'OTHER');
CREATE TYPE sex AS ENUM ('MALE', 'FEMALE', 'UNKNOWN');
CREATE TYPE patient_status AS ENUM ('ACTIVE', 'DECEASED', 'INACTIVE');

CREATE TABLE owners (
    id         uuid PRIMARY KEY,
    name       text        NOT NULL,
    phone      text        NOT NULL,
    alt_phone  text,
    address    text,
    notes      text,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    deleted_at timestamptz
);

CREATE INDEX owners_phone_idx ON owners (phone) WHERE deleted_at IS NULL;
CREATE INDEX owners_name_idx ON owners (lower(name)) WHERE deleted_at IS NULL;
-- sync cursor scans
CREATE INDEX owners_updated_at_idx ON owners (updated_at);

CREATE TRIGGER owners_set_updated_at
    BEFORE UPDATE ON owners
    FOR EACH ROW EXECUTE FUNCTION set_updated_at();

CREATE TABLE patients (
    id             uuid PRIMARY KEY,
    owner_id       uuid           NOT NULL REFERENCES owners (id),
    name           text           NOT NULL,
    species        species        NOT NULL,
    breed          text,
    sex            sex            NOT NULL DEFAULT 'UNKNOWN',
    sterilized     boolean        NOT NULL DEFAULT false,
    birth_date     date,
    color_markings text,
    microchip_no   text,
    photo_key      text,
    allergies      text,
    alert_notes    text, -- e.g. "aggressive, needs muzzle" — surfaced prominently in the app
    status         patient_status NOT NULL DEFAULT 'ACTIVE',
    created_at     timestamptz    NOT NULL DEFAULT now(),
    updated_at     timestamptz    NOT NULL DEFAULT now(),
    deleted_at     timestamptz
);

CREATE INDEX patients_owner_id_idx ON patients (owner_id) WHERE deleted_at IS NULL;
CREATE INDEX patients_name_idx ON patients (lower(name)) WHERE deleted_at IS NULL;
CREATE INDEX patients_updated_at_idx ON patients (updated_at);

CREATE TRIGGER patients_set_updated_at
    BEFORE UPDATE ON patients
    FOR EACH ROW EXECUTE FUNCTION set_updated_at();

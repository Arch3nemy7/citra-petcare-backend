-- The clinic records four kinds of visits. The type drives the app's form
-- layout (Grooming skips the medical steps) and is shown alongside the date
-- in the visit history ("6 Jul 2026 · Periksa"). Existing rows are medical
-- examinations, so they default to PERIKSA.
CREATE TYPE visit_type AS ENUM ('PERIKSA', 'GROOMING', 'VAKSINASI', 'STERILISASI');

ALTER TABLE visits
    ADD COLUMN visit_type visit_type NOT NULL DEFAULT 'PERIKSA';

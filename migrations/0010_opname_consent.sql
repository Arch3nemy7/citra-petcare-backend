-- Opname (inpatient care) joins the visit types; like Sterilisasi it is a
-- procedure the owner must consent to, so attachments gain a CONSENT kind
-- for the signed letter of approval (anesthesia etc.). Postgres 12+ allows
-- ADD VALUE inside a transaction as long as the value is not used in the
-- same migration.
ALTER TYPE visit_type ADD VALUE 'OPNAME';
ALTER TYPE attachment_kind ADD VALUE 'CONSENT';

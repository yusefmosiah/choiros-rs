-- ADR-0014 Phase 6: Machine classes
-- Track which machine class each VM was booted with.
ALTER TABLE user_vms ADD COLUMN machine_class TEXT;

-- Per-user machine class preference (used on next VM boot).
ALTER TABLE users ADD COLUMN machine_class TEXT;

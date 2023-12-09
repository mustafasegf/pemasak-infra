-- Modify "projects" table
ALTER TABLE "projects" ADD COLUMN "state" text NOT NULL DEFAULT 'stopped';

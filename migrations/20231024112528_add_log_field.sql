-- Modify "builds" table
ALTER TABLE "builds" ADD COLUMN "log" text NOT NULL DEFAULT '';

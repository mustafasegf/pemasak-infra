-- Modify "projects" table
ALTER TABLE "projects" ADD COLUMN "environs" jsonb NOT NULL DEFAULT '{}';

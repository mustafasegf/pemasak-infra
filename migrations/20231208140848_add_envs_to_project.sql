-- Modify "projects" table
ALTER TABLE "projects" ADD COLUMN "envs" jsonb NOT NULL DEFAULT '{}';

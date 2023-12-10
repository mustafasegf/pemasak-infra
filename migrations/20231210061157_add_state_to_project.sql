-- Create enum type "project_state"
CREATE TYPE "project_state" AS ENUM ('empty', 'running', 'stopped', 'idle');
-- Modify "projects" table
ALTER TABLE "projects" ADD COLUMN "state" "project_state" NOT NULL DEFAULT 'empty';

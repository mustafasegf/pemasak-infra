-- Create enum type "build_state"
CREATE TYPE "build_state" AS ENUM ('pending', 'building', 'successful', 'failed');
-- Create "builds" table
CREATE TABLE "builds" ("id" uuid NOT NULL, "project_id" uuid NOT NULL, "status" "build_state" NOT NULL DEFAULT 'pending', "created_at" timestamptz NOT NULL DEFAULT now(), "updated_at" timestamptz NOT NULL DEFAULT now(), "finished_at" timestamptz NULL, PRIMARY KEY ("id"), CONSTRAINT "builds_project_id_fkey" FOREIGN KEY ("project_id") REFERENCES "projects" ("id") ON UPDATE CASCADE ON DELETE CASCADE);

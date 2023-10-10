-- Modify "domains" table
ALTER TABLE "domains" DROP COLUMN "projects_id", ADD COLUMN "project_id" uuid NOT NULL, ADD CONSTRAINT "domains_project_id_fkey" FOREIGN KEY ("project_id") REFERENCES "projects" ("id") ON UPDATE CASCADE ON DELETE CASCADE;

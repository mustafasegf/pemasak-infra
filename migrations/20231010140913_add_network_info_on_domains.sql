-- Modify "domains" table
ALTER TABLE "domains" ADD COLUMN "port" integer NOT NULL, ADD COLUMN "docker_ip" text NOT NULL, ADD COLUMN "db_url" text NULL;

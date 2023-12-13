-- Modify "domains" table
ALTER TABLE "domains" ADD COLUMN "subnet" text NOT NULL DEFAULT '0.0.0.0/0';

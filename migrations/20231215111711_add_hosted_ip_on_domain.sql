-- Modify "domains" table
ALTER TABLE "domains" ADD COLUMN "host_ip" text NOT NULL DEFAULT '0.0.0.0';

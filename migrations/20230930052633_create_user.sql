-- Create "users" table
CREATE TABLE "users" ("id" uuid NOT NULL, "username" character varying(255) NOT NULL, "password" character varying(255) NOT NULL, "name" text NOT NULL, "created_at" timestamptz NOT NULL DEFAULT now(), "updated_at" timestamptz NOT NULL DEFAULT now(), "deleted_at" timestamptz NULL, PRIMARY KEY ("id"));
-- Create index "unique_username" to table: "users"
CREATE UNIQUE INDEX "unique_username" ON "users" ("username");

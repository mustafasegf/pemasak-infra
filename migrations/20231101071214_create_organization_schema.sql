-- Create "organization" table
CREATE TABLE "organization" ("id" uuid NOT NULL, "name" character varying(64) NOT NULL, "created_at" timestamptz NOT NULL DEFAULT now(), "updated_at" timestamptz NOT NULL DEFAULT now(), "deleted_at" timestamptz NULL, PRIMARY KEY ("id"));
-- Create index "unique_organization_name" to table: "organization"
CREATE UNIQUE INDEX "unique_organization_name" ON "organization" ("name");
-- Create "organization_owners" table
CREATE TABLE "organization_owners" ("organization_id" uuid NOT NULL, "owner_id" uuid NOT NULL, "created_at" timestamptz NOT NULL DEFAULT now(), "updated_at" timestamptz NOT NULL DEFAULT now(), "deleted_at" timestamptz NULL, PRIMARY KEY ("organization_id", "owner_id"), CONSTRAINT "organization_owners_organization_id_fkey" FOREIGN KEY ("organization_id") REFERENCES "organization" ("id") ON UPDATE CASCADE ON DELETE CASCADE, CONSTRAINT "organization_owners_owner_id_fkey" FOREIGN KEY ("owner_id") REFERENCES "project_owners" ("id") ON UPDATE CASCADE ON DELETE CASCADE);
-- Create "users_organization" table
CREATE TABLE "users_organization" ("user_id" uuid NOT NULL, "organization_id" uuid NOT NULL, "created_at" timestamptz NOT NULL DEFAULT now(), "updated_at" timestamptz NOT NULL DEFAULT now(), "deleted_at" timestamptz NULL, PRIMARY KEY ("user_id", "organization_id"), CONSTRAINT "users_organization_organization_id_fkey" FOREIGN KEY ("organization_id") REFERENCES "organization" ("id") ON UPDATE CASCADE ON DELETE CASCADE, CONSTRAINT "users_organization_user_id_fkey" FOREIGN KEY ("user_id") REFERENCES "users" ("id") ON UPDATE CASCADE ON DELETE CASCADE);

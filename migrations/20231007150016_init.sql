-- Create enum type "role"
CREATE TYPE "role" AS ENUM ('admin', 'asdos', 'user');
-- Create "sessions" table
CREATE TABLE "sessions" ("id" character varying(128) NOT NULL, "expires" integer NULL, "session" text NOT NULL, PRIMARY KEY ("id"));
-- Create "user_permissions" table
CREATE TABLE "user_permissions" ("user_id" uuid NOT NULL, "token" character varying(256) NOT NULL);
-- Create "project_owners" table
CREATE TABLE "project_owners" ("id" uuid NOT NULL, "name" text NOT NULL, "created_at" timestamptz NOT NULL DEFAULT now(), "updated_at" timestamptz NOT NULL DEFAULT now(), "deleted_at" timestamptz NULL, PRIMARY KEY ("id"));
-- Create "projects" table
CREATE TABLE "projects" ("id" uuid NOT NULL, "owner_id" uuid NOT NULL, "name" text NOT NULL, "created_at" timestamptz NOT NULL DEFAULT now(), "updated_at" timestamptz NOT NULL DEFAULT now(), "deleted_at" timestamptz NULL, PRIMARY KEY ("id"), CONSTRAINT "projects_owner_id_fkey" FOREIGN KEY ("owner_id") REFERENCES "project_owners" ("id") ON UPDATE CASCADE ON DELETE CASCADE);
-- Create "api_token" table
CREATE TABLE "api_token" ("id" uuid NOT NULL, "project_id" uuid NOT NULL, "token" text NOT NULL, "created_at" timestamptz NOT NULL DEFAULT now(), "updated_at" timestamptz NOT NULL DEFAULT now(), "deleted_at" timestamptz NULL, PRIMARY KEY ("id"), CONSTRAINT "api_token_project_id_fkey" FOREIGN KEY ("project_id") REFERENCES "projects" ("id") ON UPDATE CASCADE ON DELETE CASCADE);
-- Create "domains" table
CREATE TABLE "domains" ("id" uuid NOT NULL, "projects_id" uuid NOT NULL, "name" text NOT NULL, "created_at" timestamptz NOT NULL DEFAULT now(), "updated_at" timestamptz NOT NULL DEFAULT now(), "deleted_at" timestamptz NULL, PRIMARY KEY ("id"), CONSTRAINT "domains_projects_id_fkey" FOREIGN KEY ("projects_id") REFERENCES "projects" ("id") ON UPDATE CASCADE ON DELETE CASCADE);
-- Create "users" table
CREATE TABLE "users" ("id" uuid NOT NULL, "username" character varying(255) NOT NULL, "password" text NOT NULL, "name" text NOT NULL, "created_at" timestamptz NOT NULL DEFAULT now(), "updated_at" timestamptz NOT NULL DEFAULT now(), "deleted_at" timestamptz NULL, "role" "role" NOT NULL DEFAULT 'user', PRIMARY KEY ("id"));
-- Create index "unique_username" to table: "users"
CREATE UNIQUE INDEX "unique_username" ON "users" ("username");
-- Create "users_owners" table
CREATE TABLE "users_owners" ("user_id" uuid NOT NULL, "owner_id" uuid NOT NULL, "created_at" timestamptz NOT NULL DEFAULT now(), "updated_at" timestamptz NOT NULL DEFAULT now(), "deleted_at" timestamptz NULL, PRIMARY KEY ("user_id", "owner_id"), CONSTRAINT "users_owners_owner_id_fkey" FOREIGN KEY ("owner_id") REFERENCES "project_owners" ("id") ON UPDATE CASCADE ON DELETE CASCADE, CONSTRAINT "users_owners_user_id_fkey" FOREIGN KEY ("user_id") REFERENCES "users" ("id") ON UPDATE CASCADE ON DELETE CASCADE);

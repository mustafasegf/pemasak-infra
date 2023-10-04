-- Create "owners" table
CREATE TABLE "owners" ("id" uuid NOT NULL, "name" text NOT NULL, "created_at" timestamptz NOT NULL DEFAULT now(), "updated_at" timestamptz NOT NULL DEFAULT now(), "deleted_at" timestamptz NULL, PRIMARY KEY ("id"));
-- Create "sessions" table
CREATE TABLE "sessions" ("id" character varying(128) NOT NULL, "expires" integer NULL, "session" text NOT NULL, PRIMARY KEY ("id"));
-- Create "user_permissions" table
CREATE TABLE "user_permissions" ("user_id" uuid NOT NULL, "token" character varying(256) NOT NULL);
-- Create "users" table
CREATE TABLE "users" ("id" uuid NOT NULL, "username" character varying(255) NOT NULL, "password" text NOT NULL, "name" text NOT NULL, "created_at" timestamptz NOT NULL DEFAULT now(), "updated_at" timestamptz NOT NULL DEFAULT now(), "deleted_at" timestamptz NULL, PRIMARY KEY ("id"));
-- Create index "unique_username" to table: "users"
CREATE UNIQUE INDEX "unique_username" ON "users" ("username");
-- Create "users_owners" table
CREATE TABLE "users_owners" ("user_id" uuid NOT NULL, "owner_id" uuid NOT NULL, "created_at" timestamptz NOT NULL DEFAULT now(), "updated_at" timestamptz NOT NULL DEFAULT now(), "deleted_at" timestamptz NULL, PRIMARY KEY ("user_id", "owner_id"), CONSTRAINT "users_owners_owner_id_fkey" FOREIGN KEY ("owner_id") REFERENCES "owners" ("id") ON UPDATE CASCADE ON DELETE CASCADE, CONSTRAINT "users_owners_user_id_fkey" FOREIGN KEY ("user_id") REFERENCES "users" ("id") ON UPDATE CASCADE ON DELETE CASCADE);
-- Create "api_token" table
CREATE TABLE "api_token" ("id" uuid NOT NULL, "user_id" uuid NOT NULL, "name" character varying(255) NOT NULL, "token" text NOT NULL, "created_at" timestamptz NOT NULL DEFAULT now(), "updated_at" timestamptz NOT NULL DEFAULT now(), "deleted_at" timestamptz NULL, PRIMARY KEY ("id"), CONSTRAINT "api_token_user_id_fkey" FOREIGN KEY ("user_id") REFERENCES "users" ("id") ON UPDATE CASCADE ON DELETE CASCADE);
-- Create "repositories" table
CREATE TABLE "repositories" ("id" uuid NOT NULL, "owner_id" uuid NOT NULL, "name" text NOT NULL, "created_at" timestamptz NOT NULL DEFAULT now(), "updated_at" timestamptz NOT NULL DEFAULT now(), "deleted_at" timestamptz NULL, PRIMARY KEY ("id"), CONSTRAINT "repositories_owner_id_fkey" FOREIGN KEY ("owner_id") REFERENCES "owners" ("id") ON UPDATE CASCADE ON DELETE CASCADE);
-- Create "domains" table
CREATE TABLE "domains" ("id" uuid NOT NULL, "repo_id" uuid NOT NULL, "name" text NOT NULL, "created_at" timestamptz NOT NULL DEFAULT now(), "updated_at" timestamptz NOT NULL DEFAULT now(), "deleted_at" timestamptz NULL, PRIMARY KEY ("id"), CONSTRAINT "domains_repo_id_fkey" FOREIGN KEY ("repo_id") REFERENCES "repositories" ("id") ON UPDATE CASCADE ON DELETE CASCADE);

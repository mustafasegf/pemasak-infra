-- Create "owner" table
CREATE TABLE "owner" ("id" uuid NOT NULL, "name" text NOT NULL, "created_at" timestamptz NOT NULL DEFAULT now(), "updated_at" timestamptz NOT NULL DEFAULT now(), "deleted_at" timestamptz NULL, PRIMARY KEY ("id"));
-- Create "repositories" table
CREATE TABLE "repositories" ("id" uuid NOT NULL, "owner_id" uuid NOT NULL, "name" text NOT NULL, "created_at" timestamptz NOT NULL DEFAULT now(), "updated_at" timestamptz NOT NULL DEFAULT now(), "deleted_at" timestamptz NULL, PRIMARY KEY ("id"), CONSTRAINT "repositories_owner_id_fkey" FOREIGN KEY ("owner_id") REFERENCES "owner" ("id") ON UPDATE CASCADE ON DELETE CASCADE);
-- Create "domains" table
CREATE TABLE "domains" ("id" uuid NOT NULL, "repo_id" uuid NOT NULL, "name" text NOT NULL, "created_at" timestamptz NOT NULL DEFAULT now(), "updated_at" timestamptz NOT NULL DEFAULT now(), "deleted_at" timestamptz NULL, PRIMARY KEY ("id"), CONSTRAINT "domains_repo_id_fkey" FOREIGN KEY ("repo_id") REFERENCES "repositories" ("id") ON UPDATE CASCADE ON DELETE CASCADE);
-- Create "users_owner" table
CREATE TABLE "users_owner" ("user_id" uuid NOT NULL, "owner_id" uuid NOT NULL, "created_at" timestamptz NOT NULL DEFAULT now(), "updated_at" timestamptz NOT NULL DEFAULT now(), "deleted_at" timestamptz NULL, PRIMARY KEY ("user_id", "owner_id"), CONSTRAINT "users_owner_owner_id_fkey" FOREIGN KEY ("owner_id") REFERENCES "owner" ("id") ON UPDATE CASCADE ON DELETE CASCADE, CONSTRAINT "users_owner_user_id_fkey" FOREIGN KEY ("user_id") REFERENCES "users" ("id") ON UPDATE CASCADE ON DELETE CASCADE);

-- Create "api_token" table
CREATE TABLE "api_token" ("id" uuid NOT NULL, "user_id" uuid NOT NULL, "token" text NOT NULL, "created_at" timestamptz NOT NULL DEFAULT now(), "updated_at" timestamptz NOT NULL DEFAULT now(), "deleted_at" timestamptz NULL, PRIMARY KEY ("id"), CONSTRAINT "api_token_user_id_fkey" FOREIGN KEY ("user_id") REFERENCES "users" ("id") ON UPDATE CASCADE ON DELETE CASCADE);

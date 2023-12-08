#!/usr/bin/env bash
json=$(cat configuration.yml| yj)
# Extract values using jq
db_user=$(echo "$json" | jq -r '.database.user')
db_password=$(echo "$json" | jq -r '.database.password')
db_port=$(echo "$json" | jq -r '.database.port')
db_name=$(echo "$json" | jq -r '.database.name')
# TODO: add support for ssl

database_url="postgresql://$db_user:$db_password@localhost:$db_port/$db_name?search_path=public&sslmode=disable"

echo "applying migrations to $database_url"

atlas schema apply \
  --url "$database_url" \
  --to "file://migrations?version=$@" \
  --dev-url "docker://postgres/15/dev?search_path=public" \
  --exclude "atlas_schema_revisions"

if [ $? -ne 0 ]; then
  echo "failed to apply migrations"
  exit 1
fi

atlas migrate set $@ \
  --url "$database_url" \
  --dir "file://migrations"

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

atlas migrate apply \
  --dir "file://migrations" \
  --url "$database_url"

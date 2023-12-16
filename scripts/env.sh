#!/usr/bin/env bash
json=$(cat configuration.yml | yj)
# Extract values using jq
db_user=$(echo "$json" | jq -r '.database.user')
db_password=$(echo "$json" | jq -r '.database.password')
db_port=$(echo "$json" | jq -r '.database.port')
db_name=$(echo "$json" | jq -r '.database.name')
application_port=$(echo "$json" | jq -r '.application.port')

# Print Docker Compose and SQLx format
cat <<EOF
# for docker compose
DB_USER=$db_user
DB_PASSWORD=$db_password
DB_PORT=$db_port
DB_NAME=$db_name
APPLICATION_PORT=$application_port

# for sqlx
DATABASE_URL=postgresql://$db_user:$db_password@localhost:$db_port/$db_name
EOF

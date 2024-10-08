#!/usr/bin/env bash
json=$(cat configuration.yml| yj)
# Extract values using jq
db_user=$(echo "$json" | jq -r '.database.user')
db_password=$(echo "$json" | jq -r '.database.password')
db_port=$(echo "$json" | jq -r '.database.port')
db_name=$(echo "$json" | jq -r '.database.name')
application_port=$(echo "$json" | jq -r '.application.port')
grafana_user=$(echo "$json" | jq -r '.grafana.user')
grafana_password=$(echo "$json" | jq -r '.grafana.password')
domain=$(echo "$json" | jq -r '.application.domain')

# Print Docker Compose and SQLx format
echo "# for docker compose"
echo "DB_USER=$db_user"
echo "DB_PASSWORD=$db_password"
echo "DB_PORT=$db_port"
echo "DB_NAME=$db_name"
echo "APPLICATION_PORT=$application_port"
echo "DOMAIN=$domain"
echo "GF_SECURITY_ADMIN_USER=$grafana_user"
echo "GF_SECURITY_ADMIN_PASSWORD=$grafana_password"

echo ""

echo "# for sqlx"
echo "DATABASE_URL=postgresql://$db_user:$db_password@localhost:$db_port/$db_name"

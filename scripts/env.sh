#!//usr/bin/env bash
json=$(cat configuration.yml| yj)
# Extract values using jq
db_user=$(echo "$json" | jq -r '.database.user')
db_password=$(echo "$json" | jq -r '.database.password')
db_port=$(echo "$json" | jq -r '.database.port')
db_name=$(echo "$json" | jq -r '.database.name')

# Print Docker Compose and SQLx format
echo "# for docker compose"
echo "DB_USER=postgres"
echo "DB_PASSWORD=$db_password"
echo "DB_PORT=5432"
echo "DB_NAME=postgres"

echo ""

echo "# for sqlx"
echo "DATABASE_URL=postgresql://$db_user:$db_password@localhost:$db_port/$db_name"

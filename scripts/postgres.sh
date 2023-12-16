#!/usr/bin/env bash
json=$(cat configuration.yml | yj)
# Extract values using jq
db_name=$(echo "$json" | jq -r '.database.name')

cat <<EOF >>"$(git rev-parse --show-toplevel)/config/postgres/postgresql.conf"
listen_addresses = '*'
max_worker_processes = 32
track_activity_query_size = 2048
pg_stat_statements.track = all
shared_preload_libraries = 'pg_stat_statements,pg_partman_bgw'
pg_partman_bgw.dbname = '$db_name'
pg_partman_bgw.interval = 60
pg_partman_bgw.role = 'postgres'
EOF

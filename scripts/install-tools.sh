#!//usr/bin/env bash
curl -sSf https://atlasgo.sh | sh
cargo install sqlx-cli --no-default-features --features postgres
cargo install bunyan

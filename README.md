# pemasak-infra
PaaS (Platform as a Service) to help sustain application deployment in Fasilkom UI

## Dev setup guide
make sure your user has docker access by running `groups` and check if docker is in it. If not run `sudo usermod -aG docker $USER newgrp docker` or run the app with sudo

1. install tool by running `./scripts/install-tools.sh`
2. install `jq` and `yj`
3. copy `configuration.example.yml` to `configuration.yml` and change the config
4. run `./scripts/env.sh > .env`
5. run `docker compose up -d`
6. run `./scripts/apply.sh`
7. run `RUST_LOG=info cargo run 2>&1 | bunyan` this will talke a while


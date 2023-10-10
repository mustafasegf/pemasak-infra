# pemasak-infra
PaaS (Platform as a Service) to help sustain application deployment in Fasilkom UI

## Dev setup guide
### Using nix (recomended)
1. run `./script/install-nix.sh` make sure not using root but the user have root privileges
2. close terminal and open it again to get new session
3. run `direnv allow`
4. copy `configuration.example.yml` to `configuration.yml` and change the config
5. run `./scripts/env.sh > .env`
6. run `docker compose up -d`
7. run `./scripts/apply.sh`
8. run `nix run .#dev` this will talke a while

### Not Using nix
make sure your user has docker access by running `groups` and check if docker is in it. If not run `sudo usermod -aG docker $USER newgrp docker` or run the app with sudo

1. install rust via rustup `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh`
2. install tool by running `./scripts/install-tools.sh`
3. install `jq` and `yj`
4. copy `configuration.example.yml` to `configuration.yml` and change the config
5. run `./scripts/env.sh > .env`
6. run `docker compose up -d`
7. run `./scripts/apply.sh`
8. run `RUST_LOG=info cargo run 2>&1 | bunyan` this will talke a while


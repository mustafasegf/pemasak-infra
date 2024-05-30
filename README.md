# pemasak-infra
PaaS (Platform as a Service) to help sustain application deployment in Fasilkom UI

## Dev setup guide
make sure your user has docker access by running `groups` and check if docker is in it. If not run `sudo usermod -aG docker $USER newgrp docker` or run the app with sudo

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

1. install rust via rustup `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh`
2. install tool by running `./scripts/install-tools.sh`
3. install `jq` and `yj`
4. copy `configuration.example.yml` to `configuration.yml` and change the config
5. run `./scripts/env.sh > .env`
6. run `docker compose up -d`
7. run `./scripts/apply.sh`
8. run `RUST_LOG=info cargo run` this will talke a while

### sqlx
after writing code. Before commit, run `cargo sqlx prepare`. To do that automatically you can enable the git hook by running `ln -sf ../../scripts/pre-commit ./.git/hooks`

### Server Maintainer Guide
1. Make sure docker is installed
2. Change the docker daemon file in `/etc/docker/daemon.json` to
```json
{
  "metrics-addr": "127.0.0.1:9323",
  "features": {
    "buildkit": false
  },
  "bip": "172.32.0.1/12",
  "default-address-pools": [
    {
      "base" :"172.17.0.0/12",
      "size": 24
    },
    {
      "base" :"192.168.0.0/16",
      "size": 24
    }
  ]
}
```
to make sure the project won't ran out of ip.

3. Make sure the user have docker group access by running `groups` and check if docker is in it. If not run `sudo usermod -aG docker $USER && newgrp docker`
4. Copy `configuration.example.yml` to `configuration.yml` and change the `configuration.yml` `application.bodylimit` to large value like 500mb or 1gb to allow large file upload.
5. Copy `.env.example` in `ui` folder to `.env` and change the `VITE_API_URL` to the server ip
6. Run `./scripts/env.sh > .env` to generate the environment variable
7. Run `docker compose up -d` to start the server

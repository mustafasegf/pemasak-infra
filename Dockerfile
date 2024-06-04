# stage 0: chef
FROM rust:1.72.0 AS chef
WORKDIR /app
RUN cargo install cargo-chef --version 0.1.60 --locked
RUN apt update

#stage 1: planner
FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

# stage 2: caching
FROM chef AS cacher
COPY --from=planner /app/recipe.json recipe.json
RUN cargo chef cook --release --recipe-path recipe.json

# stage 3: build
FROM chef AS builder
COPY . .
COPY --from=cacher /app/target target
COPY --from=cacher $CARGO_HOME $CARGO_HOME
RUN curl -sL https://deb.nodesource.com/setup_20.x -o /tmp/nodesource_setup.sh && bash /tmp/nodesource_setup.sh && apt update && apt install -y nodejs
RUN npm install -g pnpm
RUN cd ui && pnpm install --frozen-lockfile
RUN cargo build --release

# stage 4: run
# FROM gcr.io/distroless/cc-debian11
FROM ubuntu:22.04
WORKDIR /app
COPY --from=builder /app/target/release/pemasak-infra /app
COPY --from=builder /app/ui/dist /app/ui/dist
RUN apt update && apt install -y libssl-dev ca-certificates git apt-transport-https curl software-properties-common gnupg
RUN curl -fsSL https://download.docker.com/linux/ubuntu/gpg | gpg --dearmor -o /usr/share/keyrings/docker-archive-keyring.gpg
RUN echo "deb [arch=$(dpkg --print-architecture) signed-by=/usr/share/keyrings/docker-archive-keyring.gpg] https://download.docker.com/linux/ubuntu $(lsb_release -cs) stable" | tee /etc/apt/sources.list.d/docker.list > /dev/null
RUN apt update
RUN apt-cache policy docker-ce
RUN apt install -y docker-ce docker-ce-cli containerd.io docker-buildx-plugin docker-compose-plugin
CMD ["./pemasak-infra"]

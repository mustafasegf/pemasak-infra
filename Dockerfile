# stage 0: chef
FROM rust:1.72.0 AS chef
WORKDIR /app
RUN cargo install cargo-chef
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
RUN cargo build --release

# stage 4: run
# FROM gcr.io/distroless/cc-debian11
FROM ubuntu:22.04
WORKDIR /app
COPY --from=builder /app/target/release/pemasak-infra /app
RUN apt update && apt install -y libssl-dev ca-certificates git
CMD ["./pemasak-infra"]

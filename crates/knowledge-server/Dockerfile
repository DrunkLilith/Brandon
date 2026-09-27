FROM rust:1.98.0-bookworm AS build

WORKDIR /workspace
COPY Cargo.toml Cargo.lock rust-toolchain.toml ./
COPY crates ./crates
RUN cargo build --locked --release -p knowledge-server \
    --bin knowledge-server --bin bootstrap --bin migrate --bin rewrap-keys

FROM debian:12.13-slim AS runtime
RUN apt-get update \
    && apt-get install --no-install-recommends -y ca-certificates curl \
    && rm -rf /var/lib/apt/lists/* \
    && groupadd --system --gid 10001 knowledge \
    && useradd --system --uid 10001 --gid knowledge --no-create-home knowledge
COPY --from=build /workspace/target/release/knowledge-server /usr/local/bin/knowledge-server
COPY --from=build /workspace/target/release/bootstrap /usr/local/bin/bootstrap
COPY --from=build /workspace/target/release/migrate /usr/local/bin/migrate
COPY --from=build /workspace/target/release/rewrap-keys /usr/local/bin/rewrap-keys
USER 10001:10001
EXPOSE 3000
ENTRYPOINT ["/usr/local/bin/knowledge-server"]

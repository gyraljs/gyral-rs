# syntax=docker/dockerfile:1
FROM node:24.22.0-trixie-slim@sha256:a6155384446670e9719bdc4fd0ebe4d4b29fb32a8d6aa523da3547a6cfbcf22c AS web
WORKDIR /build
RUN corepack enable
COPY package.json pnpm-lock.yaml pnpm-workspace.yaml ./
RUN --mount=type=cache,target=/root/.local/share/pnpm/store pnpm install --frozen-lockfile
COPY tools/build-example.mjs ./tools/build-example.mjs
COPY examples/axum-app/web ./examples/axum-app/web
RUN pnpm build:example

FROM rust:1.99.1-trixie@sha256:09027f873d45b8ecd1dc86f1fa13b7a62b875c961f017a0e331267b350430137 AS rust
WORKDIR /build
COPY Cargo.toml Cargo.lock rust-toolchain.toml ./
COPY .cargo ./.cargo
COPY tools/prepare-v8.py tools/v8-archive.json ./tools/
RUN --mount=type=cache,target=/build/.local/v8 python3 tools/prepare-v8.py
COPY .sqlx ./.sqlx
COPY crates ./crates
COPY examples/axum-app/Cargo.toml ./examples/axum-app/Cargo.toml
COPY examples/axum-app/src ./examples/axum-app/src
COPY examples/askama-client-app/Cargo.toml ./examples/askama-client-app/Cargo.toml
COPY examples/askama-client-app/src ./examples/askama-client-app/src
COPY examples/askama-client-app/templates ./examples/askama-client-app/templates
COPY examples/askama-client-app/migrations ./examples/askama-client-app/migrations
COPY examples/notes-domain ./examples/notes-domain
COPY examples/notes-app ./examples/notes-app
RUN --mount=type=cache,target=/usr/local/cargo/registry --mount=type=cache,target=/build/target --mount=type=cache,target=/build/.local/v8 python3 tools/prepare-v8.py && cargo build --release --locked -p gyral-axum-example && cp target/release/gyral-axum-example /usr/local/bin/gyral-axum-example

FROM debian:trixie-20261027-slim@sha256:f6b08b09a05725f9431502d3b5714f5c4576e82c1a84fa1debe0c24a7e0c3ec4 AS app
ARG GYRAL_INPUT_SHA256
LABEL org.gyral-rs.input-sha256=$GYRAL_INPUT_SHA256
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates curl && rm -rf /var/lib/apt/lists/* && groupadd --gid 10001 gyral && useradd --uid 10001 --gid gyral --no-create-home gyral
WORKDIR /app
COPY --from=rust /usr/local/bin/gyral-axum-example /usr/local/bin/gyral-axum-example
COPY --from=web /build/examples/axum-app/dist ./dist
ENV GYRAL_EXAMPLE_BIND=0.0.0.0:3037 GYRAL_EXAMPLE_DIST=/app/dist
USER 10001:10001
EXPOSE 3037
STOPSIGNAL SIGTERM
HEALTHCHECK --interval=30s --timeout=3s --start-period=10s CMD curl --fail --silent http://127.0.0.1:3037/readyz > /dev/null
CMD ["gyral-axum-example"]

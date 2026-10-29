# syntax=docker/dockerfile:1
FROM node:24.22.0-trixie-slim@sha256:7f486cabe892bff8fef04c0f7da8dba84a560be3e4e2740b7a86afc57ebec315 AS web
WORKDIR /build
RUN corepack enable
COPY package.json pnpm-lock.yaml pnpm-workspace.yaml ./
RUN --mount=type=cache,target=/root/.local/share/pnpm/store pnpm install --frozen-lockfile
COPY tools/build-example.mjs ./tools/build-example.mjs
COPY examples/axum-app/web ./examples/axum-app/web
RUN pnpm build:example

FROM rust:1.99.0-trixie@sha256:81015672d6939b0265f09adf96d5de125482bb1330106076956b0518e74edc90 AS rust
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

FROM debian:trixie-20261005-slim@sha256:a29215f6a35e51e22adffa17f89e9d2ef06214e64a2bad10d765c46aea49f11f AS app
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

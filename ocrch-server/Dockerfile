# syntax=docker/dockerfile:1
#
# Build context must be the workspace root:
#   docker build -f ocrch-server/Dockerfile -t ocrch-server .
#
# At runtime, mount your config file and set DATABASE_URL:
#   docker run \
#     -e DATABASE_URL=postgres://... \
#     -v /path/to/ocrch-config.toml:/app/ocrch-config.toml:ro \
#     -p 8080:8080 \
#     ocrch-server

# Prepare the builder image
FROM rust:1.94-bookworm AS chef
RUN cargo install cargo-chef --locked
WORKDIR /app

# Plan the dependencies
FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json --bin ocrch-server

# Build the application
FROM chef AS builder
COPY --from=planner /app/recipe.json recipe.json
RUN cargo chef cook --release --recipe-path recipe.json
ENV SQLX_OFFLINE=true
COPY . .
RUN cargo build --release --bin ocrch-server

# Runtime image
FROM gcr.io/distroless/cc
COPY --from=builder /app/target/release/ocrch-server /app/ocrch-server
WORKDIR /app
USER nonroot:nonroot
EXPOSE 8080
ENTRYPOINT ["/app/ocrch-server"]

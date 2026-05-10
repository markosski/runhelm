FROM rust:1-alpine AS builder

WORKDIR /app
RUN apk add --no-cache musl-dev
COPY Cargo.toml Cargo.lock ./
COPY src ./src
RUN cargo build

FROM alpine:3.22

RUN addgroup -g 10001 runhelm && adduser -D -H -u 10001 -G runhelm runhelm
COPY --from=builder /app/target/debug/orchestrator /usr/local/bin/orchestrator
USER runhelm

EXPOSE 3456
ENTRYPOINT ["/usr/local/bin/orchestrator"]

FROM rust:1.92-slim AS builder

WORKDIR /app
COPY . .
RUN cargo build --release

FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*

WORKDIR /app
RUN mkdir -p /app/data

COPY --from=builder /app/target/release/app /app/app
COPY config/ /app/config/
COPY templates/ /app/templates/
COPY static/ /app/static/
COPY emails/ /app/emails/
COPY migrations/ /app/migrations/

ENV APP_ENV=production
EXPOSE 8080

CMD ["/app/app"]

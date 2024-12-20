FROM rust:1 AS builder
WORKDIR /usr/src/app
COPY . .
RUN cargo install --path .

FROM debian:bookworm
RUN apt update && apt install dumb-init
WORKDIR /app
COPY --from=builder /usr/src/app/target/release/demostats /app/demostats
EXPOSE 8811

CMD ["./demostats"]
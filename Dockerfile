FROM rust:latest AS builder
WORKDIR /usr/src/app
COPY Cargo.toml .
COPY tf2_demostats_http tf2_demostats_http
COPY tf2_demostats_cli tf2_demostats_cli
COPY tf2_demostats tf2_demostats
RUN cargo build --release

FROM debian:bookworm
WORKDIR /app
COPY --from=builder /usr/src/app/target/release/tf2_demostats /app/tf2_demostats
COPY schema.json .
EXPOSE 8811

ENTRYPOINT [ "./tf2_demostats"]
CMD [ "serve" ]

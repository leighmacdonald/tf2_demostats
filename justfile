set dotenv-load := true

test_post:
    curl -v -i --form "file=@test.dem" http://localhost:8811/

test $RUST_BACKTRACE="1":
    cargo test

check: clippy audit machete

clippy:
    cargo clippy

audit:
    cargo audit

machete:
    cargo machete  --with-metadata

snapshot:
    goreleaser release --snapshot --clean

schema:
    cargo run --bin tf2_demostats_schema

run $RUST_BACKTRACE="1":
    cargo run

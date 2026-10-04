set dotenv-load := true

test_post:
    #!/usr/bin/env bash
    set -euo pipefail
    req=$(mktemp)
    trap 'rm -f "$req"' EXIT
    demo_b64=$(base64 -w0 test.dem)
    printf '{"demo":"%s","filename":"test.dem"}' "$demo_b64" > "$req"
    curl -v -i -X POST http://localhost:8811/demostats.v1.DemoService/ParseDemo \
        -H 'content-type: application/json' \
        --data @"$req"

test $RUST_BACKTRACE="1":
    cargo test

check: clippy machete test

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

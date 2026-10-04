fn main() {
    connectrpc_build::Config::new()
        .files(&["../proto/demostats/v1/demostats.proto"])
        .includes(&["../proto/"])
        .include_file("_connectrpc.rs")
        // Gate generated client stubs behind the `client` cargo feature so
        // the server build does not pull in client transports.
        .gate_client_feature(true)
        .compile()
        .unwrap();
}

fn main() {
    tonic_build::configure()
        .build_server(true)
        .build_client(false)
        .compile(&["../proto/joker_guide.proto"], &["../proto"])
        .expect("failed to compile proto");
}

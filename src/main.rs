#[tokio::main(flavor = "current_thread")]
async fn main() {
    std::process::exit(jevctl::run().await);
}

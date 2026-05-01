#[tokio::main(flavor = "multi_thread")]
async fn main() {
    if let Err(e) = skills::cli::run().await {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}

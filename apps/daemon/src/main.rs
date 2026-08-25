#![forbid(unsafe_code)]

#[tokio::main(flavor = "current_thread")]
async fn main() {
    if let Err(error) = halquen_daemon::run().await {
        eprintln!("halquen-daemon: {error}");
        std::process::exit(1);
    }
}

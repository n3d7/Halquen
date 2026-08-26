#![forbid(unsafe_code)]

fn main() {
    if let Err(error) = halquen_desktop::run() {
        eprintln!("halquen-desktop: {error}");
        std::process::exit(1);
    }
}

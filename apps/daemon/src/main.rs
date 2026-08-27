#![forbid(unsafe_code)]

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let options = match parse_options(std::env::args().skip(1)) {
        Ok(options) => options,
        Err(error) => {
            eprintln!("halquen-daemon: {error}");
            std::process::exit(2);
        }
    };
    if let Err(error) = halquen_daemon::run(options).await {
        eprintln!("halquen-daemon: {error}");
        std::process::exit(1);
    }
}

fn parse_options(
    arguments: impl Iterator<Item = String>,
) -> Result<halquen_daemon::DaemonOptions, String> {
    let mut options = halquen_daemon::DaemonOptions::default();
    let mut arguments = arguments.peekable();
    while let Some(argument) = arguments.next() {
        match argument.as_str() {
            "--execution-mode" => {
                options.execution_mode = match arguments.next().as_deref() {
                    Some("dry-run") => halquen_daemon::ExecutionMode::DryRun,
                    Some("real") => halquen_daemon::ExecutionMode::Real,
                    _ => return Err("--execution-mode requires dry-run or real".to_owned()),
                };
            }
            "--allow-unsafe-agents" => options.allow_unsafe_agents = true,
            "--help" | "-h" => {
                return Err(
                    "usage: halquen-daemon [--execution-mode dry-run|real] [--allow-unsafe-agents]"
                        .to_owned(),
                );
            }
            _ => return Err(format!("unsupported option: {argument}")),
        }
    }
    Ok(options)
}

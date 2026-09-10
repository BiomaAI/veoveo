#[cfg(not(target_os = "linux"))]
compile_error!("the Computer execution launcher requires the qualified Linux profile");

fn main() -> std::process::ExitCode {
    let arguments: Vec<_> = std::env::args_os().skip(1).collect();
    if arguments.as_slice() == ["--version"] {
        println!(
            "veoveo-computer-exec {} protocol {}",
            env!("CARGO_PKG_VERSION"),
            veoveo_computer_execution::PROTOCOL_VERSION
        );
        return std::process::ExitCode::SUCCESS;
    }
    if !arguments.is_empty() {
        eprintln!("Computer execution launcher accepts its private request only on stdin");
        return std::process::ExitCode::from(125);
    }
    let result = veoveo_computer_execution::read_frame(std::io::stdin().lock())
        .and_then(veoveo_computer_execution::launch);
    match result {
        Ok(never) => match never {},
        Err(_) => {
            eprintln!("Computer execution request could not be launched");
            std::process::ExitCode::from(125)
        }
    }
}

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
    if arguments.as_slice() == ["--files"] {
        let mut input = std::io::stdin().lock();
        let result = veoveo_computer_execution::read_file_request(&mut input).and_then(|request| {
            veoveo_computer_execution::transfer_file(
                request,
                &mut input,
                &mut std::io::stdout().lock(),
            )
        });
        let code = match result {
            Ok(_) => 0,
            Err(veoveo_computer_execution::FileFailure::CommitUnknown) => 124,
            Err(_) => 125,
        };
        if veoveo_computer_execution::write_file_result(&mut std::io::stderr().lock(), &result)
            .is_err()
        {
            return std::process::ExitCode::from(124);
        }
        return std::process::ExitCode::from(code);
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

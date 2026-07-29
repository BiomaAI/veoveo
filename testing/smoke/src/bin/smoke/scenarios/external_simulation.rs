use std::{
    ffi::OsString,
    io::{ErrorKind, Read, Write},
    net::{TcpListener, TcpStream},
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    thread,
    time::Duration,
};

use anyhow::ensure;
use base64::{Engine as _, engine::general_purpose::STANDARD};
use sha2::{Digest, Sha256};

use super::*;

const UV_VERSION: &str = "0.11.32";
const SDK_WHEEL: &str = "veoveo_mcp-0.1.0-py3-none-any.whl";
const SDK_SHA256: &str = "3a12b26f667ab480d08b21dc84f8c274d09fa92ec707d9630b8fc40df5da26e5";
const INDEX_USERNAME: &str = "token";
const INDEX_PASSWORD: &str = "fixture-index-secret";
const CANONICAL_INDEX_ROOT: &str = "https://packages.example.internal/veoveo";

pub(crate) fn external_simulation_fixture() -> Result<()> {
    let fixture = Path::new("testing/fixtures/external-simulation-extension");
    ensure!(fixture.is_dir(), "external simulation fixture is missing");

    run_checked(
        Path::new("uvx"),
        [
            format!("uv@{UV_VERSION}").into(),
            "--directory".into(),
            fixture.as_os_str().to_os_string(),
            "lock".into(),
            "--check".into(),
            "--offline".into(),
            "--python".into(),
            "3.13".into(),
        ],
        [],
    )
    .context("checking the committed private-index lock")?;

    let workspace = tempfile::Builder::new()
        .prefix("veoveo-external-simulation-")
        .tempdir()
        .context("creating isolated extension workspace")?;
    let publication_a = workspace.path().join("publication-a");
    let publication_b = workspace.path().join("publication-b");
    fs::create_dir_all(&publication_a)?;
    fs::create_dir_all(&publication_b)?;
    build_sdk_wheel(&publication_a)?;
    build_sdk_wheel(&publication_b)?;
    let wheel_a = publication_a.join(SDK_WHEEL);
    let wheel_b = publication_b.join(SDK_WHEEL);
    let bytes = fs::read(&wheel_a).context("reading published SDK wheel")?;
    ensure!(
        hex::encode(Sha256::digest(&bytes)) == SDK_SHA256,
        "published SDK wheel does not match the compatibility manifest"
    );
    ensure!(
        fs::read(&wheel_b)? == bytes,
        "SDK wheel publication is not reproducible under the selected source epoch"
    );

    let index = PrivatePackageIndex::start(bytes)?;
    ensure!(
        unauthenticated_status(index.address())? == 401,
        "private package index accepted an unauthenticated request"
    );

    let checkout = workspace.path().join("external-checkout");
    copy_external_checkout(fixture, &checkout)?;
    rewrite_index_coordinate(&checkout.join("pyproject.toml"), index.root())?;
    rewrite_index_coordinate(&checkout.join("uv.lock"), index.root())?;

    let uv_environment = [
        ("UV_INDEX_VEOVEO_USERNAME", INDEX_USERNAME.into()),
        ("UV_INDEX_VEOVEO_PASSWORD", INDEX_PASSWORD.into()),
        ("UV_NO_CACHE", "1".into()),
    ];
    for arguments in [
        vec![
            "sync".into(),
            "--locked".into(),
            "--all-extras".into(),
            "--python".into(),
            "3.13".into(),
            "--allow-insecure-host".into(),
            "127.0.0.1".into(),
        ],
        vec![
            "run".into(),
            "--locked".into(),
            "--all-extras".into(),
            "--allow-insecure-host".into(),
            "127.0.0.1".into(),
            "pytest".into(),
            "-q".into(),
        ],
        vec!["build".into()],
    ] {
        run_uv(&checkout, arguments, uv_environment.clone())?;
    }
    ensure!(
        index.authenticated_requests() >= 1,
        "locked sync did not resolve the SDK from the authenticated package index"
    );
    ensure!(
        checkout
            .join("dist/anonymous_simulation_mcp-0.1.0-py3-none-any.whl")
            .is_file(),
        "isolated extension wheel was not produced"
    );
    ensure!(
        checkout
            .join("dist/anonymous_simulation_mcp-0.1.0.tar.gz")
            .is_file(),
        "isolated extension source distribution was not produced"
    );

    run_in_directory(
        &checkout,
        Path::new("docker"),
        [
            "buildx".into(),
            "bake".into(),
            "anonymous-simulation-extension".into(),
            "--print".into(),
        ],
        [],
    )
    .context("validating the external repository's native Bake graph")?;
    run_checked(
        Path::new("helm"),
        ["lint".into(), checkout.join("deploy/helm").into_os_string()],
        [],
    )?;
    let rendered = run_checked(
        Path::new("helm"),
        [
            "template".into(),
            "anonymous-simulation".into(),
            checkout.join("deploy/helm").into_os_string(),
            "--values".into(),
            checkout
                .join("deploy/helm/values.test.yaml")
                .into_os_string(),
        ],
        [],
    )?;
    contains(
        &rendered,
        "registry.example.internal/extensions/anonymous-simulation-mcp@sha256:",
    )?;
    not_contains(&rendered, "nvidia.com/gpu")?;
    not_contains(&rendered, "simulation-view-isaac")?;

    println!(
        "external simulation fixture ok: reproducible SDK artifact, authenticated locked install, \
         tests, package, Bake graph, and CPU-only chart"
    );
    Ok(())
}

fn build_sdk_wheel(output: &Path) -> Result<()> {
    run_checked(
        Path::new("uvx"),
        [
            format!("uv@{UV_VERSION}").into(),
            "build".into(),
            "--wheel".into(),
            "--out-dir".into(),
            output.as_os_str().to_os_string(),
            "sdk/python".into(),
        ],
        [("SOURCE_DATE_EPOCH", "1785096000".into())],
    )?;
    ensure!(
        output.join(SDK_WHEEL).is_file(),
        "SDK build omitted {SDK_WHEEL}"
    );
    Ok(())
}

fn run_uv(
    checkout: &Path,
    arguments: Vec<OsString>,
    environment: [(&'static str, OsString); 3],
) -> Result<String> {
    let mut command = vec![
        format!("uv@{UV_VERSION}").into(),
        "--directory".into(),
        checkout.as_os_str().to_os_string(),
    ];
    command.extend(arguments);
    run_checked(Path::new("uvx"), command, environment)
}

fn run_in_directory(
    directory: &Path,
    program: &Path,
    arguments: impl IntoIterator<Item = OsString>,
    environment: impl IntoIterator<Item = (&'static str, OsString)>,
) -> Result<String> {
    let output = Command::new(program)
        .args(arguments)
        .envs(environment)
        .current_dir(directory)
        .output()
        .with_context(|| format!("running {} in {}", program.display(), directory.display()))?;
    ensure!(
        output.status.success(),
        "{} failed with status {}\nstdout:\n{}\nstderr:\n{}",
        program.display(),
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(String::from_utf8(output.stdout)?)
}

fn copy_external_checkout(source: &Path, destination: &Path) -> Result<()> {
    fs::create_dir_all(destination)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let name = entry.file_name();
        let name_text = name.to_string_lossy();
        if matches!(
            name_text.as_ref(),
            ".venv" | ".pytest_cache" | ".veoveo" | "dist" | "__pycache__"
        ) {
            continue;
        }
        let source_path = entry.path();
        let destination_path = destination.join(&name);
        let kind = entry.file_type()?;
        if kind.is_dir() {
            copy_external_checkout(&source_path, &destination_path)?;
        } else if kind.is_file() {
            fs::copy(&source_path, &destination_path)?;
        } else {
            bail!(
                "external fixture contains unsupported filesystem entry {}",
                source_path.display()
            );
        }
    }
    Ok(())
}

fn rewrite_index_coordinate(path: &Path, replacement: &str) -> Result<()> {
    let source = fs::read_to_string(path)?;
    ensure!(
        source.contains(CANONICAL_INDEX_ROOT),
        "{} does not contain the compatibility-selected index",
        path.display()
    );
    fs::write(path, source.replace(CANONICAL_INDEX_ROOT, replacement))?;
    Ok(())
}

fn unauthenticated_status(address: &str) -> Result<u16> {
    let mut stream = TcpStream::connect(address)?;
    stream.write_all(
        b"GET /veoveo/simple/veoveo-mcp/ HTTP/1.1\r\nHost: packages.example.internal\r\nConnection: close\r\n\r\n",
    )?;
    let mut response = String::new();
    stream.read_to_string(&mut response)?;
    response
        .split_whitespace()
        .nth(1)
        .context("private index returned no HTTP status")?
        .parse()
        .context("private index returned an invalid HTTP status")
}

struct PrivatePackageIndex {
    address: String,
    root: String,
    stop: Arc<AtomicBool>,
    authenticated: Arc<AtomicUsize>,
    thread: Option<thread::JoinHandle<()>>,
}

impl PrivatePackageIndex {
    fn start(wheel: Vec<u8>) -> Result<Self> {
        let listener = TcpListener::bind("127.0.0.1:0")?;
        listener.set_nonblocking(true)?;
        let socket = listener.local_addr()?;
        let address = socket.to_string();
        let root = format!("http://{address}/veoveo");
        let stop = Arc::new(AtomicBool::new(false));
        let authenticated = Arc::new(AtomicUsize::new(0));
        let thread_stop = stop.clone();
        let thread_authenticated = authenticated.clone();
        let expected_authorization = format!(
            "Basic {}",
            STANDARD.encode(format!("{INDEX_USERNAME}:{INDEX_PASSWORD}"))
        );
        let thread = thread::spawn(move || {
            while !thread_stop.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        let _ = serve_index_request(
                            &mut stream,
                            &wheel,
                            &expected_authorization,
                            &thread_authenticated,
                        );
                    }
                    Err(error) if error.kind() == ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(5));
                    }
                    Err(_) => break,
                }
            }
        });
        Ok(Self {
            address,
            root,
            stop,
            authenticated,
            thread: Some(thread),
        })
    }

    fn address(&self) -> &str {
        &self.address
    }

    fn root(&self) -> &str {
        &self.root
    }

    fn authenticated_requests(&self) -> usize {
        self.authenticated.load(Ordering::Relaxed)
    }
}

impl Drop for PrivatePackageIndex {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        let _ = TcpStream::connect(&self.address);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn serve_index_request(
    stream: &mut TcpStream,
    wheel: &[u8],
    expected_authorization: &str,
    authenticated: &AtomicUsize,
) -> Result<()> {
    stream.set_read_timeout(Some(Duration::from_secs(5)))?;
    let mut request = [0_u8; 16 * 1024];
    let length = stream.read(&mut request)?;
    let request = String::from_utf8_lossy(&request[..length]);
    let mut lines = request.lines();
    let request_line = lines.next().context("package index request line missing")?;
    let mut request_parts = request_line.split_whitespace();
    let method = request_parts.next().unwrap_or_default();
    let path = request_parts.next().unwrap_or_default();
    let authorized = lines.any(|line| {
        line.split_once(':').is_some_and(|(name, value)| {
            name.eq_ignore_ascii_case("authorization") && value.trim() == expected_authorization
        })
    });
    if !authorized {
        write_response(
            stream,
            method,
            401,
            "Unauthorized",
            "text/plain",
            b"authentication required",
            Some("Basic realm=\"veoveo\""),
        )?;
        return Ok(());
    }
    authenticated.fetch_add(1, Ordering::Relaxed);
    match path {
        "/veoveo/simple/veoveo-mcp/" => {
            let html = format!(
                "<!doctype html><a href=\"../../artifacts/{SDK_WHEEL}#sha256={SDK_SHA256}\">\
                 {SDK_WHEEL}</a>"
            );
            write_response(
                stream,
                method,
                200,
                "OK",
                "text/html",
                html.as_bytes(),
                None,
            )?;
        }
        path if path == format!("/veoveo/artifacts/{SDK_WHEEL}") => {
            write_response(stream, method, 200, "OK", "application/zip", wheel, None)?;
        }
        _ => {
            write_response(
                stream,
                method,
                404,
                "Not Found",
                "text/plain",
                b"not found",
                None,
            )?;
        }
    }
    Ok(())
}

fn write_response(
    stream: &mut TcpStream,
    method: &str,
    status: u16,
    reason: &str,
    content_type: &str,
    body: &[u8],
    authenticate: Option<&str>,
) -> Result<()> {
    let authenticate = authenticate
        .map(|value| format!("WWW-Authenticate: {value}\r\n"))
        .unwrap_or_default();
    write!(
        stream,
        "HTTP/1.1 {status} {reason}\r\nContent-Type: {content_type}\r\n\
         Content-Length: {}\r\n{authenticate}Connection: close\r\n\r\n",
        body.len()
    )?;
    if method != "HEAD" {
        stream.write_all(body)?;
    }
    Ok(())
}

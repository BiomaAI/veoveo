use super::*;

pub(crate) struct ChildGuard {
    child: Child,
}

impl ChildGuard {
    pub(crate) fn spawn(
        program: &Path,
        args: impl IntoIterator<Item = OsString>,
        envs: impl IntoIterator<Item = (&'static str, OsString)>,
        log: &Path,
    ) -> Result<Self> {
        let stdout = File::create(log)
            .with_context(|| format!("failed to create child log {}", log.display()))?;
        let stderr = stdout.try_clone()?;
        let mut command = Command::new(program);
        configure_binary_runtime(&mut command, program);
        let child = command
            .args(args)
            .envs(envs)
            .stdout(Stdio::from(stdout))
            .stderr(Stdio::from(stderr))
            .spawn()
            .with_context(|| format!("failed to spawn {}", program.display()))?;
        Ok(Self { child })
    }

    pub(crate) fn stop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Drop for ChildGuard {
    fn drop(&mut self) {
        self.stop();
    }
}

#[derive(Debug)]
pub(crate) struct ContainerGuard {
    name: String,
}

impl ContainerGuard {
    pub(crate) fn new(name: impl Into<String>) -> Self {
        Self { name: name.into() }
    }
}

impl Drop for ContainerGuard {
    fn drop(&mut self) {
        let _ = Command::new("docker")
            .args(["rm", "-f", self.name.as_str()])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
}

pub(crate) fn assert_executable(path: &Path) -> Result<()> {
    if !path.exists() {
        bail!("required binary does not exist: {}", path.display());
    }
    Ok(())
}

pub(crate) fn run_checked(
    program: &Path,
    args: impl IntoIterator<Item = OsString>,
    envs: impl IntoIterator<Item = (&'static str, OsString)>,
) -> Result<String> {
    let output = run_raw(program, args, envs)?;
    if !output.status.success() {
        bail!(
            "{} failed with status {}\nstdout:\n{}\nstderr:\n{}",
            program.display(),
            output.status,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    Ok(String::from_utf8(output.stdout)?)
}

pub(crate) fn run_raw(
    program: &Path,
    args: impl IntoIterator<Item = OsString>,
    envs: impl IntoIterator<Item = (&'static str, OsString)>,
) -> Result<Output> {
    let mut command = Command::new(program);
    configure_binary_runtime(&mut command, program);
    command
        .args(args)
        .env_remove("VEOVEO_INTERNAL_TOKEN_SECRET")
        .envs(envs)
        .output()
        .with_context(|| format!("failed to run {}", program.display()))
}

pub(crate) fn configure_binary_runtime(command: &mut Command, program: &Path) {
    let Some(bin_dir) = program.parent() else {
        return;
    };
    let deps_dir = bin_dir.join("deps");
    if !deps_dir.exists() {
        return;
    }
    prepend_path_env(command, "DYLD_LIBRARY_PATH", &deps_dir);
    prepend_path_env(command, "LD_LIBRARY_PATH", &deps_dir);
    prepend_path_env(command, "PATH", &deps_dir);
}

pub(crate) fn prepend_path_env(command: &mut Command, key: &str, path: &Path) {
    let mut paths = vec![path.to_path_buf()];
    if let Some(existing) = env::var_os(key) {
        paths.extend(env::split_paths(&existing));
    }
    if let Ok(joined) = env::join_paths(paths) {
        command.env(key, joined);
    }
}

pub(crate) fn smoke_tmpdir() -> Result<PathBuf> {
    let tmpdir = env::temp_dir().join(format!("veoveo-smoke-{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(&tmpdir)?;
    Ok(tmpdir)
}

pub(crate) struct TmpDirGuard {
    path: PathBuf,
    remove_on_drop: bool,
}

impl TmpDirGuard {
    pub(crate) fn new(path: PathBuf) -> Self {
        Self {
            path,
            remove_on_drop: false,
        }
    }

    pub(crate) fn remove_on_drop(&mut self) {
        self.remove_on_drop = true;
    }
}

impl Drop for TmpDirGuard {
    fn drop(&mut self) {
        if self.remove_on_drop {
            let _ = std::fs::remove_dir_all(&self.path);
        } else {
            eprintln!(
                "smoke failed; leaving workspace for logs: {}",
                self.path.display()
            );
        }
    }
}

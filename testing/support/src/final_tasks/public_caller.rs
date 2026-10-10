//! Private fixture admission and append-only journals for public caller qualification.
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::{
    fs::File,
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::Mutex,
};
use veoveo_deploy_contract::InstallationTarget;
use veoveo_types::GatewayProfileId;
use veoveo_types::HttpsUrl;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PublicCallerInput<S> {
    pub schema: S,
    pub installation_target: PathBuf,
    pub endpoint: HttpsUrl,
    pub profile: GatewayProfileId,
    pub caller_token_file: PathBuf,
    pub output: PathBuf,
}
impl<S: PartialEq> PublicCallerInput<S> {
    pub fn admit(
        &self,
        selected: &InstallationTarget,
        incompatible: bool,
        schema: &S,
    ) -> Result<()> {
        ensure!(&self.schema == schema, "unsupported public caller profile");
        ensure!(
            !incompatible,
            "public OAuth and candidate/direct-replica profiles cannot be combined"
        );
        ensure!(
            self.installation_target.is_absolute(),
            "public caller requires an absolute installation target"
        );
        let admitted = InstallationTarget::load(&self.installation_target)
            .map_err(|_| anyhow::anyhow!("public caller installation admission failed"))?;
        ensure!(
            &admitted == selected,
            "public caller installation differs from selected installation"
        );
        ensure!(
            self.profile.as_str() == selected.operator.profile,
            "public caller profile differs from selected operator profile"
        );
        let mut expected = selected.public_base_url.clone();
        expected
            .path_segments_mut()
            .map_err(|_| anyhow::anyhow!("invalid installation public origin"))?
            .clear()
            .extend(["mcp", self.profile.as_str()]);
        ensure!(
            self.endpoint.as_str() == expected.as_str(),
            "public endpoint must be the selected installation operator MCP endpoint"
        );
        ensure!(
            self.output.is_absolute() && !self.output.exists(),
            "public caller journal requires a new absolute file"
        );
        Ok(())
    }
}

pub fn read_private_input<T: DeserializeOwned>(path: &Path) -> Result<T> {
    use std::os::unix::fs::MetadataExt;
    ensure!(path.is_absolute(), "public fixture path must be absolute");
    ensure!(
        !std::fs::symlink_metadata(path)
            .context("admitting private fixture")?
            .file_type()
            .is_symlink(),
        "public fixture cannot be a symlink"
    );
    let file = File::open(path).context("opening private public caller fixture")?;
    let metadata = file.metadata()?;
    ensure!(
        metadata.is_file() && metadata.mode() & 0o077 == 0 && metadata.len() <= 65536,
        "public caller fixture must be private and at most 64 KiB"
    );
    let mut bytes = Vec::new();
    file.take(65537).read_to_end(&mut bytes)?;
    ensure!(bytes.len() <= 65536, "public caller fixture exceeds 64 KiB");
    serde_json::from_slice(&bytes)
        .map_err(|_| anyhow::anyhow!("public caller typed fixture admission failed"))
}

pub struct PrivateCallerJournal(Mutex<File>);
impl PrivateCallerJournal {
    pub fn create(path: &Path) -> Result<Self> {
        use std::os::unix::fs::OpenOptionsExt;
        ensure!(path.is_absolute(), "private journal path must be absolute");
        let file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(path)
            .context("creating private public caller journal")?;
        Ok(Self(Mutex::new(file)))
    }
    pub fn append<T: Serialize>(&self, observation: &T) -> Result<()> {
        let mut file = self
            .0
            .lock()
            .map_err(|_| anyhow::anyhow!("private journal lock failed"))?;
        serde_json::to_writer(&mut *file, observation)?;
        file.write_all(b"\n")?;
        file.sync_all()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::{PermissionsExt, symlink};

    #[derive(Deserialize, Serialize)]
    #[serde(deny_unknown_fields)]
    struct Fixture {
        selected: bool,
    }

    #[test]
    fn private_caller_files_refuse_public_symlink_oversized_and_reused_paths() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let input = directory.path().join("input.json");
        std::fs::write(&input, br#"{"selected":true}"#)?;
        std::fs::set_permissions(&input, std::fs::Permissions::from_mode(0o644))?;
        assert!(read_private_input::<Fixture>(&input).is_err());
        std::fs::set_permissions(&input, std::fs::Permissions::from_mode(0o600))?;
        assert!(read_private_input::<Fixture>(&input)?.selected);
        let alias = directory.path().join("alias.json");
        symlink(&input, &alias)?;
        assert!(read_private_input::<Fixture>(&alias).is_err());
        std::fs::write(&input, vec![b' '; 65537])?;
        assert!(read_private_input::<Fixture>(&input).is_err());
        std::fs::write(&input, br#"{"selected":true,"unknown":1}"#)?;
        assert!(read_private_input::<Fixture>(&input).is_err());
        let output = directory.path().join("out.jsonl");
        let journal = PrivateCallerJournal::create(&output)?;
        journal.append(&Fixture { selected: true })?;
        assert_eq!(
            std::fs::metadata(&output)?.permissions().mode() & 0o777,
            0o600
        );
        assert!(PrivateCallerJournal::create(&output).is_err());
        let rows = std::fs::read_to_string(output)?;
        assert_eq!(rows.lines().count(), 1);
        assert!(serde_json::from_str::<Fixture>(rows.trim())?.selected);
        Ok(())
    }
}

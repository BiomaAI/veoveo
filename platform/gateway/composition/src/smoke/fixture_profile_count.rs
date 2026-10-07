use super::*;
pub fn fixture_profile_count(path: &Path) -> Result<u64> {
    let fixture: veoveo_mcp_contract::GatewayControlPlane =
        serde_json::from_slice(&fs::read(path)?)?;
    Ok(fixture.profiles.len().try_into()?)
}

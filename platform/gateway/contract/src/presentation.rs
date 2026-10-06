//! Gateway transport, owned HTTP route and health vocabularies.
#[derive(Clone, Copy, Debug, Eq, PartialEq, veoveo_types::Vocabulary)]
pub enum GatewayServerHealthState {
    #[vocabulary(rename = "healthy")]
    Healthy,
    #[vocabulary(rename = "degraded")]
    Degraded,
    #[vocabulary(rename = "offline")]
    Offline,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, veoveo_types::Vocabulary)]
pub enum UpstreamTransport {
    #[vocabulary(rename = "streamable_http")]
    StreamableHttp,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, veoveo_types::Vocabulary)]
pub enum OwnedRoutePurpose {
    #[vocabulary(rename = "webhook")]
    Webhook,
    #[vocabulary(rename = "artifact_bytes")]
    ArtifactBytes,
    #[vocabulary(rename = "provider_fetchable_files")]
    ProviderFetchableFiles,
    #[vocabulary(rename = "health")]
    Health,
}

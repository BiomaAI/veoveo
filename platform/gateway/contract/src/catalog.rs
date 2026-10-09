//! Transport-free declarations shared by catalog publishers, readers and schemas.
use crate::{AuthorizationServerId, ProtectedResourceId, ProtectedResourceName};
use schemars::{JsonSchema, Schema, SchemaGenerator};
use serde::{Serialize, de::DeserializeOwned};
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};
use veoveo_types::{
    ActionHandle, ActionKey, ActionName, ActionRegistry, ActionRegistryBuilder, AdmittedExtensions,
    DataLabelId, ExtensionError, ExtensionKey, ExtensionName, ExtensionRegistry,
    ExtensionRegistryBuilder, OAuthClientId, PolicyVersion, ScopeName, TenantId, Vocabulary,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum RuleSelector {
    Profiles,
    ProtectedResources,
    Servers,
    Tools,
    ResourceSchemes,
    Prompts,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectorRequirement {
    Optional,
    Required,
    Forbidden,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerRequirement {
    pub slug: Option<veoveo_types::ServerSlug>,
    pub resources: bool,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActionAccess {
    Read,
    Write,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActionDescriptor {
    pub access: ActionAccess,
    pub target_kinds: BTreeSet<ExtensionName>,
    pub selectors: BTreeMap<RuleSelector, SelectorRequirement>,
    pub server: Option<ServerRequirement>,
}
impl ActionDescriptor {
    pub fn validate(&self) -> Result<(), ExtensionError> {
        if self.target_kinds.is_empty() {
            return Err(ExtensionError::new(
                "action must declare a supported target",
            ));
        }
        if self.selectors.len() != 6 {
            return Err(ExtensionError::new(
                "action must declare every rule selector requirement",
            ));
        }
        Ok(())
    }
}
#[derive(Debug, Clone)]
pub struct OAuthClientFacts {
    pub id: OAuthClientId,
    pub authorization_server: AuthorizationServerId,
    pub tenant: Option<TenantId>,
    pub allowed_resources: BTreeSet<ProtectedResourceId>,
    pub allowed_scopes: BTreeSet<ScopeName>,
    pub grant_types: BTreeSet<crate::OAuthGrantType>,
    pub auth_methods: BTreeSet<crate::OAuthClientAuthMethod>,
    pub default_work_context: veoveo_types::WorkContextId,
    pub invocation_mode: veoveo_types::InvocationMode,
    pub has_jwks: bool,
    pub has_credential_secret: bool,
    pub has_redirect_uris: bool,
    pub has_compatibility_helpers: bool,
    pub direct_task_call_adapter: bool,
    pub has_knowledge_indexing: bool,
}
#[derive(Debug, Clone)]
pub struct WorkContextFacts {
    pub id: veoveo_types::WorkContextId,
    pub tenant: TenantId,
}
#[derive(Debug, Clone, Default)]
pub struct CatalogFacts {
    pub authorization_servers: BTreeSet<AuthorizationServerId>,
    pub policies: BTreeSet<PolicyVersion>,
    pub tenants: BTreeSet<TenantId>,
    pub data_labels: BTreeSet<DataLabelId>,
    pub oauth_clients: Vec<OAuthClientFacts>,
    pub work_contexts: Vec<WorkContextFacts>,
}
#[derive(Debug, Clone)]
pub struct ProtectedResourceDescriptor {
    pub name: ProtectedResourceName,
    pub resource: ProtectedResourceId,
    pub authorization_server: AuthorizationServerId,
    pub policy_version: PolicyVersion,
    pub required_scopes: BTreeSet<ScopeName>,
}
#[derive(Clone)]
pub struct CatalogObjectDescriptor {
    pub tenant: Option<TenantId>,
    pub kind: ExtensionName,
    pub id: String,
    pub value: Value,
}
impl std::fmt::Debug for CatalogObjectDescriptor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CatalogObjectDescriptor")
            .field("tenant", &self.tenant)
            .field("kind", &self.kind)
            .field("id", &self.id)
            .finish_non_exhaustive()
    }
}
/// Owner configuration checks use facts from the same catalog revision.
pub trait CatalogSection:
    Serialize + DeserializeOwned + JsonSchema + Send + Sync + 'static
{
    fn validate(&self, facts: &CatalogFacts) -> Result<(), ExtensionError>;
    fn protected_resources(&self) -> Vec<ProtectedResourceDescriptor>;
    fn objects(&self) -> Result<Vec<CatalogObjectDescriptor>, ExtensionError> {
        Ok(Vec::new())
    }
}
pub type SectionKey<T> = ExtensionKey<T>;
pub type TargetKey<T> = ExtensionKey<T>;
type SectionFacts = Arc<
    dyn Fn(
            &AdmittedExtensions,
            &CatalogFacts,
        ) -> Result<
            (
                Vec<ProtectedResourceDescriptor>,
                Vec<CatalogObjectDescriptor>,
            ),
            ExtensionError,
        > + Send
        + Sync,
>;
type SchemaFn = fn(&mut SchemaGenerator) -> Schema;
#[derive(Clone)]
struct SectionDeclaration {
    facts: SectionFacts,
    schema: SchemaFn,
}
#[derive(Clone)]
struct TargetDeclaration {
    section: ExtensionName,
    schema: SchemaFn,
    audit: TargetAudit,
}
type TargetAudit =
    Arc<dyn Fn(&AdmittedExtensions) -> Result<TargetAuditResource, ExtensionError> + Send + Sync>;
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TargetAuditResource {
    pub server: veoveo_types::ServerSlug,
    pub uri: veoveo_types::ResourceUri,
}

pub struct CatalogRegistryBuilder {
    actions: ActionRegistryBuilder,
    descriptors: BTreeMap<ActionName, ActionDescriptor>,
    sections: ExtensionRegistryBuilder,
    section_declarations: BTreeMap<ExtensionName, SectionDeclaration>,
    targets: ExtensionRegistryBuilder,
    target_declarations: BTreeMap<ExtensionName, TargetDeclaration>,
    core_target_kinds: BTreeSet<ExtensionName>,
}
impl CatalogRegistryBuilder {
    pub fn new(
        core_fields: impl IntoIterator<Item = String>,
        core_target_kinds: impl IntoIterator<Item = ExtensionName>,
    ) -> Self {
        Self {
            actions: ActionRegistryBuilder::new(),
            descriptors: BTreeMap::new(),
            sections: ExtensionRegistryBuilder::new(core_fields),
            section_declarations: BTreeMap::new(),
            targets: ExtensionRegistryBuilder::new(Vec::<String>::new()),
            target_declarations: BTreeMap::new(),
            core_target_kinds: core_target_kinds.into_iter().collect(),
        }
    }
    pub fn register_kernel<A: Vocabulary>(&mut self) -> Result<ActionKey<A>, ExtensionError> {
        self.actions.register::<A>()
    }
    pub fn register_actions<A: Vocabulary>(
        &mut self,
        descriptors: Vec<(A, ActionDescriptor)>,
    ) -> Result<ActionKey<A>, ExtensionError> {
        let mut checked = BTreeMap::new();
        for (action, descriptor) in descriptors {
            descriptor.validate()?;
            let name = ActionName::parse(action.as_str())?;
            if checked.insert(name, descriptor).is_some() {
                return Err(ExtensionError::new("duplicate action descriptor"));
            }
        }
        if checked.len() != A::ALL.len()
            || A::ALL
                .iter()
                .any(|action| !checked.keys().any(|name| name.as_str() == action.as_str()))
        {
            return Err(ExtensionError::new(
                "action descriptors must cover the complete vocabulary",
            ));
        }
        let key = self.actions.register::<A>()?;
        self.descriptors.extend(checked);
        Ok(key)
    }
    pub fn register_section<T: CatalogSection>(
        &mut self,
        name: ExtensionName,
    ) -> Result<SectionKey<T>, ExtensionError> {
        self.sections.reserve(name.clone())?;
        let key = self.sections.bind_serde::<T>(&name)?;
        let captured = key.clone();
        self.section_declarations.insert(
            name,
            SectionDeclaration {
                facts: Arc::new(move |sections, facts| {
                    let Some(section) = sections.get(&captured)? else {
                        return Ok((Vec::new(), Vec::new()));
                    };
                    section.validate(facts)?;
                    Ok((section.protected_resources(), section.objects()?))
                }),
                schema: T::json_schema,
            },
        );
        Ok(key)
    }
    pub fn register_target<T: Serialize + DeserializeOwned + JsonSchema + Send + Sync + 'static>(
        &mut self,
        name: ExtensionName,
        kinds: Vec<ExtensionName>,
        audit: fn(&T) -> Result<TargetAuditResource, ExtensionError>,
    ) -> Result<TargetKey<T>, ExtensionError> {
        if kinds.is_empty()
            || kinds.iter().any(|kind| {
                self.core_target_kinds.contains(kind) || self.target_declarations.contains_key(kind)
            })
            || kinds.iter().collect::<BTreeSet<_>>().len() != kinds.len()
        {
            return Err(ExtensionError::new(
                "target kind is empty, duplicate or collides with an existing target",
            ));
        }
        self.targets.reserve(name.clone())?;
        let key = self.targets.bind_serde::<T>(&name)?;
        let captured = key.clone();
        let audit: TargetAudit = Arc::new(move |extensions| {
            audit(
                &extensions
                    .get(&captured)?
                    .ok_or_else(|| ExtensionError::new("target codec mismatch"))?,
            )
        });
        for kind in kinds {
            self.target_declarations.insert(
                kind,
                TargetDeclaration {
                    section: name.clone(),
                    schema: T::json_schema,
                    audit: audit.clone(),
                },
            );
        }
        Ok(key)
    }
    pub fn build(self) -> Result<CatalogRegistry, ExtensionError> {
        for descriptor in self.descriptors.values() {
            if descriptor.target_kinds.iter().any(|kind| {
                !self.core_target_kinds.contains(kind)
                    && !self.target_declarations.contains_key(kind)
            }) {
                return Err(ExtensionError::new(
                    "action declares an unknown target kind",
                ));
            }
        }
        Ok(CatalogRegistry {
            actions: self.actions.build(),
            descriptors: self.descriptors,
            sections: self.sections.build(),
            section_declarations: self.section_declarations,
            targets: self.targets.build(),
            target_declarations: self.target_declarations,
        })
    }
}
#[derive(Clone)]
pub struct CatalogRegistry {
    actions: ActionRegistry,
    descriptors: BTreeMap<ActionName, ActionDescriptor>,
    sections: ExtensionRegistry,
    section_declarations: BTreeMap<ExtensionName, SectionDeclaration>,
    targets: ExtensionRegistry,
    target_declarations: BTreeMap<ExtensionName, TargetDeclaration>,
}
impl std::fmt::Debug for CatalogRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CatalogRegistry")
            .field("actions", &self.actions)
            .field("sections", &self.section_declarations.keys())
            .field("targets", &self.target_declarations.keys())
            .finish()
    }
}
impl CatalogRegistry {
    pub fn actions(&self) -> &ActionRegistry {
        &self.actions
    }
    pub fn action_key<A: Vocabulary>(&self) -> Result<ActionKey<A>, ExtensionError> {
        self.actions.key()
    }
    pub fn descriptor(&self, action: &ActionName) -> Option<&ActionDescriptor> {
        self.descriptors.get(action)
    }
    pub fn check_target(&self, target: &AdmittedPolicyTarget) -> Result<(), ExtensionError> {
        self.targets.check(&target.extensions)
    }
    pub fn check_reserved(
        &self,
        fields: &[&str],
        target_kinds: &[&str],
    ) -> Result<(), ExtensionError> {
        if self
            .section_declarations
            .keys()
            .any(|name| fields.contains(&name.as_str()))
            || self
                .target_declarations
                .keys()
                .any(|name| target_kinds.contains(&name.as_str()))
        {
            return Err(ExtensionError::new(
                "catalog contribution collides with the actual core contract",
            ));
        }
        Ok(())
    }
    pub fn check_action(&self, action: &ActionHandle) -> Result<(), ExtensionError> {
        self.actions.check(action)
    }
    pub fn section_key<T: Send + Sync + 'static>(
        &self,
        name: &ExtensionName,
    ) -> Result<SectionKey<T>, ExtensionError> {
        self.sections.key(name)
    }
    pub fn target_key<T: Send + Sync + 'static>(
        &self,
        name: &ExtensionName,
    ) -> Result<TargetKey<T>, ExtensionError> {
        self.targets.key(name)
    }
    pub fn same_binding(&self, other: &Self) -> bool {
        self.actions.same_binding(&other.actions)
    }
    pub fn admit_sections(
        &self,
        values: &BTreeMap<String, Value>,
        facts: &CatalogFacts,
    ) -> Result<AdmittedCatalogSections, ExtensionError> {
        if values.keys().any(|name| {
            !self
                .section_declarations
                .keys()
                .any(|declared| declared.as_str() == name)
        }) {
            return Err(ExtensionError::new("unknown or unbound catalog section"));
        }
        let sections = self.sections.admit(values.clone())?;
        let mut resources = Vec::new();
        let mut objects = Vec::new();
        let mut identities = BTreeSet::new();
        for section in self.section_declarations.values() {
            let (section_resources, section_objects) = (section.facts)(&sections, facts)?;
            resources.extend(section_resources);
            for object in section_objects {
                if CORE_CATALOG_OBJECT_KINDS.contains(&object.kind.as_str()) {
                    return Err(ExtensionError::new(
                        "contributed catalog object uses a core kind",
                    ));
                }
                if object.id.is_empty()
                    || !identities.insert((object.kind.clone(), object.id.clone()))
                {
                    return Err(ExtensionError::new(
                        "duplicate or empty contributed catalog object identity",
                    ));
                }
                objects.push(object);
            }
        }
        Ok(AdmittedCatalogSections {
            sections,
            resources,
            objects,
        })
    }
    pub fn contribute_target<T: Serialize + 'static>(
        &self,
        key: &TargetKey<T>,
        value: &T,
    ) -> Result<AdmittedPolicyTarget, ExtensionError> {
        self.finish_target(self.targets.contribute(key, value)?)
    }
    pub fn admit_target(&self, value: Value) -> Result<AdmittedPolicyTarget, ExtensionError> {
        let kind = value
            .get("kind")
            .and_then(Value::as_str)
            .ok_or_else(|| ExtensionError::new("target kind is required"))?;
        let declaration = self
            .target_declarations
            .iter()
            .find(|(name, _)| name.as_str() == kind)
            .map(|(_, declaration)| declaration)
            .ok_or_else(|| ExtensionError::new("unknown target kind"))?;
        let admitted = self
            .targets
            .admit(BTreeMap::from([(declaration.section.to_string(), value)]))?;
        self.finish_target(admitted)
    }
    fn finish_target(
        &self,
        extensions: AdmittedExtensions,
    ) -> Result<AdmittedPolicyTarget, ExtensionError> {
        let wire = extensions.wire();
        if wire.len() != 1 {
            return Err(ExtensionError::new("one target contribution is required"));
        }
        let (section, value) = wire.into_iter().next().expect("one target checked");
        let kind = value
            .get("kind")
            .and_then(Value::as_str)
            .ok_or_else(|| ExtensionError::new("target kind is required"))?;
        let kind = ExtensionName::parse(kind)?;
        let declaration = self
            .target_declarations
            .get(&kind)
            .ok_or_else(|| ExtensionError::new("unknown target kind"))?;
        if declaration.section.as_str() != section {
            return Err(ExtensionError::new(
                "target kind does not belong to its codec",
            ));
        }
        let audit = (declaration.audit)(&extensions)?;
        Ok(AdmittedPolicyTarget {
            extensions,
            section: declaration.section.clone(),
            kind,
            wire: value,
            audit,
        })
    }
    pub fn action_schema(&self) -> Schema {
        schemars::json_schema!({"type":"string", "enum": self.actions.names().map(ActionName::as_str).collect::<Vec<_>>()})
    }
    pub fn section_schemas(&self, generator: &mut SchemaGenerator) -> BTreeMap<String, Schema> {
        self.section_declarations
            .iter()
            .map(|(name, declaration)| (name.to_string(), (declaration.schema)(generator)))
            .collect()
    }
    pub fn target_schemas(&self, generator: &mut SchemaGenerator) -> Vec<Schema> {
        let mut groups: BTreeMap<ExtensionName, (SchemaFn, Vec<String>)> = BTreeMap::new();
        for (kind, declaration) in &self.target_declarations {
            groups
                .entry(declaration.section.clone())
                .or_insert((declaration.schema, Vec::new()))
                .1
                .push(kind.to_string());
        }
        groups.into_values().map(|(schema, kinds)| {
            let schema = schema(generator);
            schemars::json_schema!({"allOf": [schema, {"type":"object", "required":["kind"], "properties":{"kind":{"enum":kinds}}}]})
        }).collect()
    }
}
#[derive(Debug, Clone)]
pub struct AdmittedCatalogSections {
    sections: AdmittedExtensions,
    resources: Vec<ProtectedResourceDescriptor>,
    objects: Vec<CatalogObjectDescriptor>,
}
impl AdmittedCatalogSections {
    pub fn get<T: Send + Sync + 'static>(
        &self,
        key: &SectionKey<T>,
    ) -> Result<Option<T>, ExtensionError> {
        self.sections.get(key)
    }
    pub fn objects(&self) -> &[CatalogObjectDescriptor] {
        &self.objects
    }
    pub fn protected_resources(&self) -> &[ProtectedResourceDescriptor] {
        &self.resources
    }
}
#[derive(Clone, PartialEq, Eq)]
pub struct AdmittedPolicyTarget {
    extensions: AdmittedExtensions,
    section: ExtensionName,
    kind: ExtensionName,
    wire: Value,
    audit: TargetAuditResource,
}
impl std::fmt::Debug for AdmittedPolicyTarget {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AdmittedPolicyTarget")
            .field("kind", &self.kind)
            .finish()
    }
}
impl AdmittedPolicyTarget {
    pub fn get<T: Send + Sync + 'static>(&self, key: &TargetKey<T>) -> Result<T, ExtensionError> {
        self.extensions
            .get(key)?
            .ok_or_else(|| ExtensionError::new("target codec mismatch"))
    }
    pub fn audit(&self) -> &TargetAuditResource {
        &self.audit
    }
    pub fn kind(&self) -> &ExtensionName {
        &self.kind
    }
    pub fn wire(&self) -> &Value {
        &self.wire
    }
    pub fn section(&self) -> &ExtensionName {
        &self.section
    }
}

impl serde::Serialize for AdmittedPolicyTarget {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.wire.serialize(serializer)
    }
}

/// Object kinds written by the core catalog publisher. Owner sections cannot replace them.
pub const CORE_CATALOG_OBJECT_KINDS: &[&str] = &[
    "identity_provider",
    "authorization_server",
    "server",
    "profile",
    "tenant",
    "work_context",
    "policy",
    "policy_rule",
    "data_label",
    "oauth_client",
    "oidc_client",
    "secret",
];

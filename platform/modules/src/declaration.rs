//! Module-owned claims, append-only lanes and composition-supplied execution.
use crate::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModuleLayer {
    Kernel,
    Optional,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ObjectKind {
    Table,
    Function,
    Analyzer,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OwnershipClaim {
    Table(TableName),
    TablePrefix(TablePrefix),
    Function(FunctionName),
    FunctionPrefix(FunctionPrefix),
    Analyzer(AnalyzerName),
}
impl OwnershipClaim {
    pub(crate) fn kind(&self) -> ObjectKind {
        match self {
            Self::Table(_) | Self::TablePrefix(_) => ObjectKind::Table,
            Self::Function(_) | Self::FunctionPrefix(_) => ObjectKind::Function,
            Self::Analyzer(_) => ObjectKind::Analyzer,
        }
    }
    pub(crate) fn text(&self) -> &str {
        match self {
            Self::Table(n) => n.as_str(),
            Self::TablePrefix(n) => n.as_str(),
            Self::Function(n) => n.as_str(),
            Self::FunctionPrefix(n) => n.as_str(),
            Self::Analyzer(n) => n.as_str(),
        }
    }
    pub(crate) fn prefix(&self) -> bool {
        matches!(self, Self::TablePrefix(_) | Self::FunctionPrefix(_))
    }
    pub(crate) fn matches(&self, kind: ObjectKind, name: &str) -> bool {
        self.kind() == kind
            && if self.prefix() {
                name.starts_with(self.text())
            } else {
                name == self.text()
            }
    }
    pub(crate) fn overlaps(&self, other: &Self) -> bool {
        self.kind() == other.kind()
            && (self.text() == other.text()
                || self.prefix() && other.text().starts_with(self.text())
                || other.prefix() && self.text().starts_with(other.text()))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LaneRequirement {
    Satisfied(ModuleName),
    AtLeast {
        module: ModuleName,
        version: MigrationVersion,
    },
}
impl LaneRequirement {
    pub fn module(&self) -> &ModuleName {
        match self {
            Self::Satisfied(n) => n,
            Self::AtLeast { module, .. } => module,
        }
    }
    pub fn minimum(&self) -> Option<MigrationVersion> {
        match self {
            Self::Satisfied(_) => None,
            Self::AtLeast { version, .. } => Some(*version),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LaneExecution {
    image: ExecutionImage,
    command: ExecutionCommand,
}
impl LaneExecution {
    pub fn new(image: ExecutionImage, command: ExecutionCommand) -> Result<Self, DeclarationError> {
        Ok(Self { image, command })
    }
    pub fn image(&self) -> &ExecutionImage {
        &self.image
    }
    pub fn command(&self) -> &ExecutionCommand {
        &self.command
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExtensionBinding {
    point: ExtensionPointName,
    implementation: ExtensionImplementationName,
}
impl ExtensionBinding {
    pub fn new(point: ExtensionPointName, implementation: ExtensionImplementationName) -> Self {
        Self {
            point,
            implementation,
        }
    }
    pub fn point(&self) -> &ExtensionPointName {
        &self.point
    }
    pub fn implementation(&self) -> &ExtensionImplementationName {
        &self.implementation
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Migration {
    version: MigrationVersion,
    name: MigrationName,
    filename: String,
    sql: &'static str,
    requires: Vec<LaneRequirement>,
}
impl Migration {
    pub fn new(
        version: MigrationVersion,
        name: MigrationName,
        sql: &'static str,
    ) -> Result<Self, DeclarationError> {
        if sql.trim().is_empty() || sql.len() > 1_048_576 {
            return Err(DeclarationError::new(
                "migration SQL must contain 1..=1048576 bytes",
            ));
        }
        let filename = format!("{:04}_{}.surql", version.get(), name.as_str());
        Ok(Self {
            version,
            name,
            filename,
            sql,
            requires: Vec::new(),
        })
    }
    /// Pin requirements per migration, so later migrations cannot rewrite admitted history.
    pub fn with_requirements(
        mut self,
        requires: Vec<LaneRequirement>,
    ) -> Result<Self, DeclarationError> {
        check_requirements(&requires)?;
        self.requires = requires;
        Ok(self)
    }
    pub fn version(&self) -> MigrationVersion {
        self.version
    }
    pub fn name(&self) -> &MigrationName {
        &self.name
    }
    pub fn filename(&self) -> &str {
        &self.filename
    }
    pub fn sql(&self) -> &'static str {
        self.sql
    }
    pub fn requires(&self) -> &[LaneRequirement] {
        &self.requires
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MigrationLane(Vec<Migration>);
impl MigrationLane {
    pub fn empty() -> Self {
        Self(Vec::new())
    }
    pub fn new(migrations: Vec<Migration>) -> Result<Self, DeclarationError> {
        if migrations.len() > 4096 {
            return Err(DeclarationError::new("migration lane exceeds 4096 entries"));
        }
        let mut minima = std::collections::BTreeMap::new();
        let mut names = std::collections::BTreeSet::new();
        for (index, migration) in migrations.iter().enumerate() {
            if migration.version.get() as usize != index {
                return Err(DeclarationError::new(
                    "migration versions must be contiguous from zero",
                ));
            }
            if !names.insert(migration.name.clone()) {
                return Err(DeclarationError::new("duplicate migration name"));
            }
            for (module, old) in &minima {
                if !migration
                    .requires
                    .iter()
                    .any(|r| r.module() == module && r.minimum() >= *old)
                {
                    return Err(DeclarationError::new(
                        "migration prerequisites cannot be removed or lowered",
                    ));
                }
            }
            for required in &migration.requires {
                if let Some(old) = minima.get(required.module())
                    && *old > required.minimum()
                {
                    return Err(DeclarationError::new(
                        "migration prerequisite minimum cannot decrease",
                    ));
                }
                minima.insert(required.module().clone(), required.minimum());
            }
        }
        Ok(Self(migrations))
    }
    pub fn migrations(&self) -> &[Migration] {
        &self.0
    }
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
    pub fn latest(&self) -> Option<MigrationVersion> {
        self.0.last().map(Migration::version)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModuleSetup {
    name: ModuleName,
    layer: ModuleLayer,
    ownership: Vec<OwnershipClaim>,
    lane: MigrationLane,
    execution: LaneExecution,
    requires: Vec<LaneRequirement>,
    extensions: Vec<ExtensionBinding>,
}
impl ModuleSetup {
    pub fn builder(name: ModuleName, layer: ModuleLayer) -> ModuleSetupBuilder {
        ModuleSetupBuilder {
            name,
            layer,
            ownership: Vec::new(),
            lane: MigrationLane::empty(),
            execution: None,
            requires: Vec::new(),
            extensions: Vec::new(),
        }
    }
    pub fn name(&self) -> &ModuleName {
        &self.name
    }
    pub fn layer(&self) -> ModuleLayer {
        self.layer
    }
    pub fn ownership(&self) -> &[OwnershipClaim] {
        &self.ownership
    }
    pub fn lane(&self) -> &MigrationLane {
        &self.lane
    }
    pub fn execution(&self) -> &LaneExecution {
        &self.execution
    }
    pub fn requires(&self) -> &[LaneRequirement] {
        &self.requires
    }
    pub fn extensions(&self) -> &[ExtensionBinding] {
        &self.extensions
    }
}

pub struct ModuleSetupBuilder {
    name: ModuleName,
    layer: ModuleLayer,
    ownership: Vec<OwnershipClaim>,
    lane: MigrationLane,
    execution: Option<LaneExecution>,
    requires: Vec<LaneRequirement>,
    extensions: Vec<ExtensionBinding>,
}
impl ModuleSetupBuilder {
    pub fn ownership(mut self, value: Vec<OwnershipClaim>) -> Self {
        self.ownership = value;
        self
    }
    pub fn lane(mut self, value: MigrationLane) -> Self {
        self.lane = value;
        self
    }
    pub fn execution(mut self, value: LaneExecution) -> Self {
        self.execution = Some(value);
        self
    }
    pub fn requires(mut self, value: Vec<LaneRequirement>) -> Self {
        self.requires = value;
        self
    }
    pub fn extensions(mut self, value: Vec<ExtensionBinding>) -> Self {
        self.extensions = value;
        self
    }
    pub fn build(self) -> Result<ModuleSetup, DeclarationError> {
        check_requirements(&self.requires)?;
        if self.requires.iter().any(|r| r.module() == &self.name) {
            return Err(DeclarationError::new("module cannot require itself"));
        }
        for (i, claim) in self.ownership.iter().enumerate() {
            if self.ownership[..i]
                .iter()
                .any(|other| other.overlaps(claim))
            {
                return Err(DeclarationError::new("overlapping ownership claims"));
            }
        }
        let mut extensions = std::collections::BTreeSet::new();
        for binding in &self.extensions {
            if !extensions.insert((binding.point.clone(), binding.implementation.clone())) {
                return Err(DeclarationError::new("duplicate extension binding"));
            }
        }
        Ok(ModuleSetup {
            name: self.name,
            layer: self.layer,
            ownership: self.ownership,
            lane: self.lane,
            execution: self
                .execution
                .ok_or_else(|| DeclarationError::new("module lane execution must be declared"))?,
            requires: self.requires,
            extensions: self.extensions,
        })
    }
}
fn check_requirements(requires: &[LaneRequirement]) -> Result<(), DeclarationError> {
    let mut names = std::collections::BTreeSet::new();
    for required in requires {
        if !names.insert(required.module()) {
            return Err(DeclarationError::new("duplicate module requirement"));
        }
    }
    Ok(())
}

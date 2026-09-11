use std::collections::BTreeMap;
use veoveo_computers::api::TemplateView;
use veoveo_computers_runtime::DevelopmentTemplate;

#[derive(Clone)]
pub struct NamedTemplate {
    pub(crate) id: String,
    pub(crate) runtime: DevelopmentTemplate,
}
impl NamedTemplate {
    pub fn runtime(&self) -> &DevelopmentTemplate {
        &self.runtime
    }
    pub fn new(id: String, runtime: DevelopmentTemplate) -> Result<Self, crate::ApplicationError> {
        if id.is_empty()
            || id.len() > 64
            || !id.as_bytes()[0].is_ascii_alphanumeric()
            || !id
                .bytes()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-')
            || runtime.persistent_home().is_none()
        {
            return Err(crate::ApplicationError::Configuration);
        }
        Ok(Self { id, runtime })
    }
    pub fn view(&self) -> TemplateView {
        TemplateView {
            template_id: self.id.clone(),
            cpus: self.runtime.cpus(),
            memory_mib: self.runtime.memory_mib(),
            home_capacity_mib: self
                .runtime
                .persistent_home()
                .map(|h| u64::from(h.capacity_mib())),
        }
    }
}
pub struct Templates {
    admitted: BTreeMap<String, NamedTemplate>,
    default: Option<String>,
}
impl Templates {
    /// Historical templates remain admitted by fingerprint. Only the selected
    /// default applies to a new public Create request.
    pub fn new(
        templates: Vec<NamedTemplate>,
        default: Option<String>,
    ) -> Result<Self, crate::ApplicationError> {
        if templates.len() > 64 {
            return Err(crate::ApplicationError::Configuration);
        }
        let mut admitted = BTreeMap::new();
        let mut names = std::collections::BTreeSet::new();
        for template in templates {
            if !names.insert(template.id.clone())
                || admitted
                    .insert(template.runtime.fingerprint(), template)
                    .is_some()
            {
                return Err(crate::ApplicationError::Configuration);
            }
        }
        if default.as_ref().is_some_and(|d| !admitted.contains_key(d)) {
            return Err(crate::ApplicationError::Configuration);
        }
        Ok(Self { admitted, default })
    }
    pub(crate) fn default(&self) -> Option<&NamedTemplate> {
        self.default.as_ref().and_then(|d| self.admitted.get(d))
    }
    pub fn select(&self, id: Option<&str>) -> Option<&NamedTemplate> {
        match id {
            Some(id) => self.admitted.values().find(|template| template.id == id),
            None => self.default(),
        }
    }
    pub(crate) fn iter(&self) -> impl Iterator<Item = &NamedTemplate> {
        self.admitted.values()
    }
    pub(crate) fn contains(&self, id: &str, fingerprint: &str) -> bool {
        self.admitted.get(fingerprint).is_some_and(|t| t.id == id)
    }
    pub fn runtimes(&self) -> Vec<DevelopmentTemplate> {
        self.admitted.values().map(|t| t.runtime.clone()).collect()
    }
}

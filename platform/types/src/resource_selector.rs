//! Lexical resource selection shared by policy and persistence adapters.
//! Templates here are selectors, distinct from RFC 6570 expansion declarations.
use crate::{
    IdentifierError, ResourceScheme, ResourceTemplateUri, ResourceUri,
    identifier_syntax::validate_path_id,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum ResourceSelector {
    Scheme { scheme: ResourceScheme },
    UriPrefix { prefix: ResourceUriPrefix },
    Template { uri_template: ResourceUriTemplate },
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(try_from = "String", into = "String")]
pub struct ResourceUriPrefix(String);

impl ResourceUriPrefix {
    pub fn new(value: impl Into<String>) -> Result<Self, IdentifierError> {
        let value = value.into();
        validate_resource_pattern(&value)?;
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl AsRef<str> for ResourceUriPrefix {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl fmt::Display for ResourceUriPrefix {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl TryFrom<String> for ResourceUriPrefix {
    type Error = IdentifierError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl From<ResourceUriPrefix> for String {
    fn from(value: ResourceUriPrefix) -> Self {
        value.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(try_from = "String", into = "String")]
pub struct ResourceUriTemplate(String);

impl ResourceUriTemplate {
    pub fn new(value: impl Into<String>) -> Result<Self, IdentifierError> {
        let value = value.into();
        validate_uri_template(&value)?;
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn matches_uri(&self, uri: &ResourceUri) -> bool {
        resource_uri_template_matches(self.as_str(), uri.as_str())
    }

    /// Apply the existing selector to the declaration's spelling. This does not
    /// establish containment of all RFC 6570 expansions; reads are checked again.
    pub fn matches_template(&self, template: &ResourceTemplateUri) -> bool {
        resource_uri_template_matches(self.as_str(), template.as_str())
    }
}

impl AsRef<str> for ResourceUriTemplate {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl fmt::Display for ResourceUriTemplate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl TryFrom<String> for ResourceUriTemplate {
    type Error = IdentifierError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl From<ResourceUriTemplate> for String {
    fn from(value: ResourceUriTemplate) -> Self {
        value.0
    }
}

fn validate_resource_pattern(value: &str) -> Result<(), IdentifierError> {
    let Some((scheme, rest)) = value.split_once("://") else {
        return Err(IdentifierError::new(
            value,
            "must be an absolute server-owned resource URI",
        ));
    };
    ResourceScheme::parse(scheme)?;
    if rest.is_empty() || rest.chars().any(|c| c.is_control() || c.is_whitespace()) {
        return Err(IdentifierError::new(
            value,
            "must include a non-empty path and no whitespace/control characters",
        ));
    }
    Ok(())
}

fn validate_uri_template(value: &str) -> Result<(), IdentifierError> {
    validate_resource_pattern(value)?;
    let parts = parse_simple_resource_uri_template(value)?;
    if !parts
        .iter()
        .any(|part| matches!(part, ResourceUriTemplatePart::Variable(_)))
    {
        return Err(IdentifierError::new(
            value,
            "must include at least one URI-template variable",
        ));
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ResourceUriTemplatePart<'a> {
    Literal(&'a str),
    Variable(&'a str),
}

fn parse_simple_resource_uri_template(
    value: &str,
) -> Result<Vec<ResourceUriTemplatePart<'_>>, IdentifierError> {
    let mut parts = Vec::new();
    let mut remaining = value;
    let mut last_was_variable = false;
    while !remaining.is_empty() {
        let next_open = remaining.find('{');
        let next_close = remaining.find('}');
        match (next_open, next_close) {
            (None, None) => {
                parts.push(ResourceUriTemplatePart::Literal(remaining));
                break;
            }
            (None, Some(_)) => {
                return Err(IdentifierError::new(
                    value,
                    "must use balanced simple {variable} expressions",
                ));
            }
            (Some(open), Some(close)) if close < open => {
                return Err(IdentifierError::new(
                    value,
                    "must use balanced simple {variable} expressions",
                ));
            }
            (Some(open), _) if open > 0 => {
                let (literal, rest) = remaining.split_at(open);
                parts.push(ResourceUriTemplatePart::Literal(literal));
                remaining = rest;
                last_was_variable = false;
            }
            (Some(_), _) => {
                let close = remaining[1..]
                    .find('}')
                    .map(|index| index + 1)
                    .ok_or_else(|| {
                        IdentifierError::new(
                            value,
                            "must use balanced simple {variable} expressions",
                        )
                    })?;
                let variable = &remaining[1..close];
                if last_was_variable {
                    return Err(IdentifierError::new(
                        value,
                        "must separate URI-template variables with literal text",
                    ));
                }
                validate_path_id(variable).map_err(|_| {
                    IdentifierError::new(
                        value,
                        "template variables must be simple lowercase identifiers",
                    )
                })?;
                parts.push(ResourceUriTemplatePart::Variable(variable));
                remaining = &remaining[close + 1..];
                last_was_variable = true;
            }
        }
    }
    Ok(parts)
}

fn resource_uri_template_matches(template: &str, uri: &str) -> bool {
    let Ok(parts) = parse_simple_resource_uri_template(template) else {
        return false;
    };
    let mut remaining = uri;
    for (index, part) in parts.iter().enumerate() {
        match part {
            ResourceUriTemplatePart::Literal(literal) => {
                let Some(next) = remaining.strip_prefix(literal) else {
                    return false;
                };
                remaining = next;
            }
            ResourceUriTemplatePart::Variable(_) => {
                let next_literal = parts[index + 1..].iter().find_map(|part| match part {
                    ResourceUriTemplatePart::Literal(literal) => Some(*literal),
                    ResourceUriTemplatePart::Variable(_) => None,
                });
                let value = if let Some(next_literal) = next_literal {
                    let Some(end) = remaining.find(next_literal) else {
                        return false;
                    };
                    let value = &remaining[..end];
                    remaining = &remaining[end..];
                    value
                } else {
                    let value = remaining;
                    remaining = "";
                    value
                };
                if value.is_empty() || value.chars().any(|c| c.is_control() || c.is_whitespace()) {
                    return false;
                }
            }
        }
    }
    remaining.is_empty()
}

impl ResourceSelector {
    pub fn matches_uri(&self, uri: &ResourceUri) -> bool {
        self.matches_reference(uri.as_str())
    }

    /// Matches declaration spelling, without claiming containment of its expansions.
    pub fn matches_template(&self, uri: &ResourceTemplateUri) -> bool {
        self.matches_reference(uri.as_str())
    }

    fn matches_reference(&self, uri: &str) -> bool {
        match self {
            Self::Scheme { scheme } => uri
                .split_once("://")
                .is_some_and(|(s, _)| s == scheme.as_str()),
            Self::UriPrefix { prefix } => uri.starts_with(prefix.as_str()),
            Self::Template { uri_template } => {
                resource_uri_template_matches(uri_template.as_str(), uri)
            }
        }
    }
}

impl ResourceUriTemplate {
    /// Ordered literal segments separated by nonempty variables. The final empty
    /// segment means a trailing variable consumes the rest. Persistence adapters
    /// bind these literals as data and implement first-delimiter matching; they
    /// must not substitute a backtracking regular expression.
    pub fn literal_segments(&self) -> Vec<&str> {
        let parts = parse_simple_resource_uri_template(self.as_str())
            .expect("constructor validated the selector template");
        let mut literals: Vec<_> = parts
            .iter()
            .filter_map(|part| match part {
                ResourceUriTemplatePart::Literal(text) => Some(*text),
                ResourceUriTemplatePart::Variable(_) => None,
            })
            .collect();
        if matches!(parts.last(), Some(ResourceUriTemplatePart::Variable(_))) {
            literals.push("");
        }
        literals
    }
}

/// A registered owner's scheme intersected with a profile's lexical selectors.
/// Constructing this value establishes no authority; a policy evaluator supplies it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ResourceSelection {
    pub scheme: ResourceScheme,
    pub selectors: Vec<ResourceSelector>,
}
impl ResourceSelection {
    pub fn matches_uri(&self, uri: &ResourceUri) -> bool {
        uri.as_str()
            .split_once("://")
            .is_some_and(|(s, _)| s == self.scheme.as_str())
            && self
                .selectors
                .iter()
                .any(|selector| selector.matches_uri(uri))
    }
}

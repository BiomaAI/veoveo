use super::{TimeMcp, default_clock_policy, internal, invalid_params, not_found, require_scope};
use crate::contract::{
    TimeKnowledgeCollection as Collection, TimeResourceEntry as Entry, TimeResourcePage as Page,
};
use crate::{
    contract::{ConvertTimeRequest, ResolveTimeRequest, TimeResource, TimeScope},
    uris,
};
use rmcp::{ErrorData as McpError, RoleServer, model::ReadResourceResult, service::RequestContext};
use serde_json::json;
use veoveo_mcp_contract::hosting::{json_read, served_by_host};
use veoveo_types::ResourceAddress;

impl TimeMcp {
    /// Reads one admitted address. The host serves documents and the contract.
    pub(super) async fn read_time_resource(
        &self,
        resource: TimeResource,
        uri: &str,
        context: &RequestContext<RoleServer>,
    ) -> Result<ReadResourceResult, McpError> {
        let identity = require_scope(context, TimeScope::Read)?;
        if resource == TimeResource::TimelineApp {
            let html = veoveo_mcp_apps_extension::workbench_app_html(
                &veoveo_mcp_apps_extension::WorkbenchApp {
                    app_id: "time-timeline",
                    title: "Timeline",
                    subtitle: "Resolve times and manage scheduled events",
                    empty_message: "No temporal resources are visible to this identity.",
                    resources: &[
                        veoveo_mcp_apps_extension::WorkbenchResource {
                            label: "Current time",
                            uri: uris::CLOCK_CURRENT_URI,
                        },
                        veoveo_mcp_apps_extension::WorkbenchResource {
                            label: "Clock quality",
                            uri: uris::CLOCK_QUALITY_URI,
                        },
                        veoveo_mcp_apps_extension::WorkbenchResource {
                            label: "Calendars",
                            uri: uris::CALENDARS_URI,
                        },
                        veoveo_mcp_apps_extension::WorkbenchResource {
                            label: "Mission epochs",
                            uri: uris::EPOCHS_URI,
                        },
                        veoveo_mcp_apps_extension::WorkbenchResource {
                            label: "Events",
                            uri: uris::EVENTS_URI,
                        },
                    ],
                    tools: &[
                        veoveo_mcp_apps_extension::WorkbenchTool {
                            label: "Resolve time",
                            name: "resolve_time",
                            arguments_json: "{}",
                        },
                        veoveo_mcp_apps_extension::WorkbenchTool {
                            label: "Convert time",
                            name: "convert_time",
                            arguments_json: "{}",
                        },
                        veoveo_mcp_apps_extension::WorkbenchTool {
                            label: "Evaluate windows",
                            name: "evaluate_windows",
                            arguments_json: "{}",
                        },
                        veoveo_mcp_apps_extension::WorkbenchTool {
                            label: "Assess clock",
                            name: "assess_clock",
                            arguments_json: "{}",
                        },
                        veoveo_mcp_apps_extension::WorkbenchTool {
                            label: "Create event",
                            name: "create_temporal_event",
                            arguments_json: "{}",
                        },
                    ],
                    stream_result: None,
                },
            );
            return Ok(ReadResourceResult::new(vec![
                veoveo_mcp_apps_extension::app_html_contents(uri, &html),
            ]));
        }
        let scope = self.state.scope(&identity).await.map_err(internal)?;
        match &resource {
            TimeResource::Calendars { cursor: after } => {
                let page = self
                    .state
                    .catalog
                    .calendars_page(&scope, after.as_ref())
                    .await
                    .map_err(crate::index::query_error)?;
                json_read(
                    uri,
                    &Page::from_page(page, |calendar| {
                        let calendar = crate::contract::OperationalCalendarValue::from(calendar);
                        Entry::new(
                            TimeResource::Calendar {
                                id: calendar.calendar_id,
                                version: calendar.version,
                            },
                            calendar.name,
                        )
                    }),
                )
            }
            TimeResource::Epochs { cursor: after } => {
                let page = self
                    .state
                    .catalog
                    .epochs_page(&scope, after.as_ref())
                    .await
                    .map_err(crate::index::query_error)?;
                json_read(
                    uri,
                    &Page::from_page(page, |epoch| {
                        Entry::new(
                            TimeResource::EpochVersion {
                                id: epoch.epoch_id,
                                version: epoch.version,
                            },
                            epoch.name,
                        )
                    }),
                )
            }
            TimeResource::Events { cursor: after } => {
                let page = self
                    .state
                    .catalog
                    .events_page(&scope, after.as_ref(), None)
                    .await
                    .map_err(crate::index::query_error)?;
                self.state
                    .schedule_events(scope.clone(), page.items.clone())
                    .await
                    .map_err(internal)?;
                json_read(
                    uri,
                    &Page::from_page(page, |event| {
                        Entry::new(TimeResource::Event(event.event_id), event.name)
                    }),
                )
            }
            TimeResource::AuthorityReleases { cursor } => {
                let page = self
                    .state
                    .catalog
                    .releases_page(&scope, cursor.as_ref())
                    .await
                    .map_err(crate::index::query_error)?;
                json_read(
                    uri,
                    &Page::from_page(page, |release| {
                        let release = crate::contract::AuthorityReleaseValue::from(release);
                        Entry::new(
                            TimeResource::AuthorityRelease(release.release_id),
                            release.version_label,
                        )
                    }),
                )
            }
            TimeResource::BootstrapAuthorities { cursor } => {
                let references = self.state.authorities.bootstrap_references();
                if cursor.as_ref().is_some_and(|cursor| {
                    !references
                        .iter()
                        .any(|r| r.release_id() == cursor.release_id())
                }) {
                    return Err(invalid_params("unknown bootstrap authority cursor"));
                }
                let items = references
                    .into_iter()
                    .filter(|r| {
                        cursor
                            .as_ref()
                            .is_none_or(|c| r.release_id() > c.release_id())
                    })
                    .map(|r| {
                        Entry::new(
                            TimeResource::BootstrapAuthority(r.release_id().clone()),
                            r.version_label().to_owned(),
                        )
                    })
                    .collect();
                json_read(
                    uri,
                    &Page::<crate::BootstrapAuthorityCursor> {
                        items,
                        next_cursor: None,
                    },
                )
            }
            TimeResource::AuthorityRelease(id) => {
                let member = self
                    .state
                    .catalog
                    .observed_release(&scope, id)
                    .await
                    .map_err(internal)?
                    .ok_or_else(|| not_found("authority release"))?;
                observed_resource(&resource, member, context)
            }
            TimeResource::BootstrapAuthority(id) => {
                let reference = self
                    .state
                    .authorities
                    .bootstrap_reference(id)
                    .ok_or_else(|| not_found("bootstrap authority"))?;
                let descriptor = Collection::BootstrapAuthorities.descriptor();
                let text = serde_json::to_string(&reference).map_err(internal)?;
                let observation = veoveo_mcp_knowledge_extension::docs::observation(
                    &descriptor,
                    veoveo_mcp_knowledge_extension::content_digest(&text),
                    chrono::Utc::now(),
                );
                veoveo_mcp_knowledge_extension::server::member_result(
                    &resource.to_uri().map_err(invalid_params)?,
                    "application/json",
                    text,
                    observation,
                    &descriptor,
                    Some(&context.meta),
                )
                .map_err(internal)
            }
            TimeResource::Zone(zone_id) => {
                let engine = self.state.engine(&scope).await.map_err(internal)?;
                let now = engine
                    .resolve(&ResolveTimeRequest {
                        expression: crate::TimeExpressionValue::Rfc3339 {
                            value: chrono::Utc::now().to_rfc3339(),
                        }
                        .build()
                        .map_err(invalid_params)?,
                        additional_uncertainty_nanoseconds: 0,
                    })
                    .map_err(invalid_params)?;
                let projection = engine
                    .convert(&ConvertTimeRequest {
                        instant: now.into_instant(),
                        zone_ids: vec![zone_id.as_str().to_owned()],
                        scales: Vec::new(),
                    })
                    .map_err(invalid_params)?;
                json_read(
                    uri,
                    &json!({"zoneId": zone_id, "tzdbReleaseId": engine.authority().binding().tzdb_release_id(), "current": projection.zoned.into_iter().next()}),
                )
            }
            TimeResource::Calendar { id, version } => {
                let member = self
                    .state
                    .catalog
                    .observed_calendar(&scope, id, *version)
                    .await
                    .map_err(internal)?
                    .ok_or_else(|| not_found("calendar version"))?;
                observed_resource(&resource, member, context)
            }
            TimeResource::EpochVersion { id, version } => {
                let member = self
                    .state
                    .catalog
                    .observed_epoch(&scope, id, *version)
                    .await
                    .map_err(internal)?
                    .ok_or_else(|| not_found("mission epoch version"))?;
                observed_resource(&resource, member, context)
            }
            TimeResource::Epoch(id) => json_read(
                uri,
                &self
                    .state
                    .catalog
                    .epoch(&scope, id)
                    .await
                    .map_err(internal)?
                    .ok_or_else(|| not_found("mission epoch"))?,
            ),
            TimeResource::Event(id) => {
                let member = self
                    .state
                    .catalog
                    .observed_event(&scope, id)
                    .await
                    .map_err(internal)?
                    .ok_or_else(|| not_found("temporal event"))?;
                self.state
                    .schedule_event(scope, member.value().clone())
                    .await
                    .map_err(internal)?;
                observed_resource(&resource, member, context)
            }
            TimeResource::ClockQuality => {
                json_read(uri, &self.state.clock.quality().await.map_err(internal)?)
            }
            TimeResource::ClockCurrent => {
                let engine = self.state.engine(&scope).await.map_err(internal)?;
                let quality = self.state.clock.quality().await.map_err(internal)?;
                let policy = self
                    .state
                    .catalog
                    .clock_policy(&scope)
                    .await
                    .map_err(internal)?
                    .map(|value| value.0)
                    .unwrap_or_else(default_clock_policy);
                let time = engine
                    .resolve(&ResolveTimeRequest {
                        expression: crate::TimeExpressionValue::Rfc3339 {
                            value: chrono::Utc::now().to_rfc3339(),
                        }
                        .build()
                        .map_err(invalid_params)?,
                        additional_uncertainty_nanoseconds: quality.error_bound_nanoseconds,
                    })
                    .map_err(invalid_params)?;
                json_read(
                    uri,
                    &crate::contract::ClockCurrent {
                        time,
                        effective_policy: policy,
                        clock_quality: quality,
                    },
                )
            }
            TimeResource::AuthoritiesCurrent => {
                let engine = self.state.engine(&scope).await.map_err(internal)?;
                json_read(uri, engine.authority().effective())
            }
            TimeResource::Docs | TimeResource::Document(_) | TimeResource::Contract => {
                Err(served_by_host())
            }
            TimeResource::TimelineApp => Err(not_found("Time resource")),
        }
    }
}

fn observed_resource<T: serde::Serialize>(
    resource: &TimeResource,
    member: crate::catalog::knowledge::ObservedTime<T>,
    context: &RequestContext<RoleServer>,
) -> Result<ReadResourceResult, McpError> {
    let collection = member.collection();
    let (text, observation) = member.document().map_err(internal)?;
    veoveo_mcp_knowledge_extension::server::member_result(
        &resource.to_uri().map_err(invalid_params)?,
        "application/json",
        text,
        observation,
        &collection.descriptor(),
        Some(&context.meta),
    )
    .map_err(internal)
}

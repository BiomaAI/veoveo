//! Typed admission for resource subscriptions.
//!
//! A server that publishes resource changes implements [`ResourceSubscriptions`]
//! for its change source. The host parses every requested URI into the server's
//! address type before the server sees it, so authorization decides typed
//! addresses, never strings. An unparseable URI is Invalid Params (-32602).

use std::future::Future;

use rmcp::{ErrorData, RoleServer, service::RequestContext, service::SubscriptionContext};
use veoveo_types::{ResourceAddress, ResourceUri};

use crate::{ResourceListObservers, SubscriptionHub};

/// A server's resource-change source and its subscription authorization.
pub trait ResourceSubscriptions: Send + Sync + 'static {
    /// The server's typed resource address.
    type Address: ResourceAddress + Send;

    /// Authorizes every requested address for the subscribing caller, or fails
    /// the whole subscription. Called once per `subscriptions/listen`.
    fn authorize(
        &self,
        addresses: Vec<Self::Address>,
        context: &RequestContext<RoleServer>,
    ) -> impl Future<Output = Result<(), ErrorData>> + Send;

    /// Change notifications for individual resources.
    fn hub(&self) -> &SubscriptionHub;

    /// Additional resource-list change observers, when the server has them.
    fn resource_lists(&self) -> Option<&ResourceListObservers> {
        None
    }
}

/// Parses the accepted resource URIs into typed addresses.
pub fn requested_addresses<A: ResourceAddress>(
    uris: Option<&[String]>,
) -> Result<Vec<A>, ErrorData> {
    uris.unwrap_or_default()
        .iter()
        .map(|uri| {
            ResourceUri::new(uri.as_str())
                .ok()
                .and_then(|uri| A::parse(&uri).ok())
                .ok_or_else(|| ErrorData::invalid_params("unknown resource address", None))
        })
        .collect()
}

/// Parses and authorizes a listen request's resource subscriptions.
pub async fn admit_resource_subscriptions<R: ResourceSubscriptions>(
    source: &R,
    context: &SubscriptionContext,
) -> Result<(), ErrorData> {
    let addresses =
        requested_addresses::<R::Address>(context.accepted().resource_subscriptions.as_deref())?;
    if addresses.is_empty() {
        return Ok(());
    }
    source.authorize(addresses, context.request_context()).await
}

#[cfg(test)]
mod tests {
    use super::requested_addresses;
    use veoveo_types::{IdentifierError, ResourceAddress, ResourceUri};

    #[derive(Debug, PartialEq)]
    struct Item(String);

    impl ResourceAddress for Item {
        type Error = IdentifierError;
        fn parse(uri: &ResourceUri) -> Result<Self, Self::Error> {
            uri.as_str()
                .strip_prefix("fixture://items/")
                .map(|id| Self(id.to_owned()))
                .ok_or_else(|| IdentifierError::new(uri.as_str(), "not an item"))
        }
        fn to_uri(&self) -> Result<ResourceUri, Self::Error> {
            ResourceUri::new(format!("fixture://items/{}", self.0))
                .map_err(|_| IdentifierError::new(&self.0, "invalid item"))
        }
    }

    #[test]
    fn requested_uris_become_typed_addresses_or_invalid_params() {
        let uris = vec!["fixture://items/1".to_owned()];
        assert_eq!(
            requested_addresses::<Item>(Some(&uris)).unwrap(),
            [Item("1".into())]
        );
        assert!(requested_addresses::<Item>(None).unwrap().is_empty());
        let bad = vec!["fixture://other".to_owned()];
        assert_eq!(
            requested_addresses::<Item>(Some(&bad)).unwrap_err().code,
            rmcp::model::ErrorCode::INVALID_PARAMS
        );
    }
}

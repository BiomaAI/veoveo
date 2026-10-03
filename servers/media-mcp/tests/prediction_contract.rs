use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use serde_json::json;
use veoveo_media_mcp::contract::*;
use veoveo_types::{ResourceAddress, TaskId};

#[test]
fn prediction_addresses_encode_reserved_identity_and_reject_aliases() {
    for text in ["provider-job-1", "part/with ?#&=%+ 雨", &"\\".repeat(512)] {
        let id = MediaPredictionId::new(text).unwrap();
        let uri = MediaPredictionUri::new(id.clone());
        assert_eq!(MediaPredictionUri::parse(uri.as_str()).unwrap(), uri);
        assert_eq!(
            <MediaPredictionUri as ResourceAddress>::parse(&uri.to_uri().unwrap()).unwrap(),
            uri
        );
        let cursor = MediaPredictionCursor::new(id.clone()).unwrap();
        assert_eq!(
            MediaPredictionCursor::parse(cursor.as_str())
                .unwrap()
                .after(),
            id
        );
        let page_uri = MediaPredictionIndexUri::new(Some(&cursor));
        assert_eq!(
            MediaPredictionIndexUri::parse(page_uri.as_str()).unwrap(),
            page_uri
        );
        assert_eq!(
            serde_json::from_value::<MediaPredictionUri>(json!(uri)).unwrap(),
            uri
        );
    }
    for invalid in ["", ".", "..", "line\nfeed", &"x".repeat(513)] {
        assert!(MediaPredictionId::new(invalid).is_err());
        assert!(serde_json::from_value::<MediaPredictionId>(json!(invalid)).is_err());
    }
    for invalid in [
        "media://prediction/a/b",
        "media://prediction/a?x=1",
        "media://prediction/a#fragment",
        "media://prediction/%61",
        "other://prediction/a",
        "media://prediction/",
        "media://prediction/a/",
        "media://prediction/a+b",
        "media://prediction/a&b",
        "media://prediction/a=b",
    ] {
        assert!(MediaPredictionUri::parse(invalid).is_err(), "{invalid}");
    }
}

#[test]
fn prediction_pages_check_collection_positions_and_entry_relationships() {
    let ids = (0..100)
        .map(|n| MediaPredictionId::new(format!("job-{n:04}")).unwrap())
        .collect::<Vec<_>>();
    let cursor = MediaPredictionCursor::new(ids[99].clone()).unwrap();
    let page = MediaPredictionPage::from_ids(ids.clone(), Some(ids[99].clone())).unwrap();
    assert_eq!(page.items().len(), 100);
    assert_eq!(page.next_cursor(), Some(&cursor));
    let wire = serde_json::to_value(&page).unwrap();
    assert_eq!(
        serde_json::from_value::<MediaPredictionPage>(wire.clone()).unwrap(),
        page
    );
    assert!(MediaPredictionPage::from_ids(ids[..99].to_vec(), Some(ids[98].clone())).is_err());
    assert!(MediaPredictionPage::from_ids(vec![ids[1].clone(), ids[0].clone()], None).is_err());
    assert!(MediaPredictionPage::from_ids(vec![ids[0].clone(), ids[0].clone()], None).is_err());
    let mut bad = wire.clone();
    bad["items"][0]["id"] = json!("another-id");
    assert!(serde_json::from_value::<MediaPredictionPage>(bad).is_err());
    let mut bad = wire;
    bad["limit"] = json!(99);
    assert!(serde_json::from_value::<MediaPredictionPage>(bad).is_err());
    for envelope in [
        json!({"version":2,"collection":"media://predictions","after":"id"}),
        json!({"version":1,"collection":"media://usage","after":"id"}),
        json!({"version":1,"collection":"media://predictions","after":""}),
    ] {
        assert!(
            MediaPredictionCursor::parse(
                URL_SAFE_NO_PAD.encode(serde_json::to_vec(&envelope).unwrap())
            )
            .is_err()
        );
    }
    assert!(
        MediaPredictionCursor::parse(MediaUsageCursor::new(TaskId::new()).unwrap().as_str())
            .is_err()
    );
    for uri in [
        format!("media://predictions?cursor={0}&cursor={0}", cursor.as_str()),
        "media://predictions?limit=10".into(),
        "media://predictions#fragment".into(),
        "media://predictions/".into(),
    ] {
        assert!(MediaPredictionIndexUri::parse(uri).is_err());
    }
}

#[test]
fn subscription_targets_share_the_typed_resource_profile() {
    for uri in [
        MediaPredictionUri::new(MediaPredictionId::new("job/1").unwrap()).to_string(),
        MediaTaskUsageUri::new(TaskId::new()).unwrap().to_string(),
        MediaUsageIndexUri::new(None).to_string(),
        MediaPredictionIndexUri::new(None).to_string(),
    ] {
        assert!(subscribable(&uri).is_some());
    }
    for uri in [
        "media://models",
        "media://usage/task/bad",
        "media://predictions?extra=1",
        "other://prediction/1",
    ] {
        assert!(subscribable(uri).is_none());
    }
}

fn subscribable(uri: &str) -> Option<MediaSubscriptionResource> {
    MediaSubscriptionResource::from_resource(MediaResource::parse(uri).ok()?)
}

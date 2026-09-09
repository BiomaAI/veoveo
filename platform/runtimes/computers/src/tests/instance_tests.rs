use super::*;
use std::collections::BTreeSet;

fn replacement(home: bool, instance: u128) -> Binding {
    Binding::replacement(
        binding().computer_id(),
        Uuid::from_u128(instance),
        template(home).fingerprint(),
    )
    .unwrap()
}

#[test]
fn replacement_names_are_distinct_stable_and_fully_owner_bound() {
    let initial = binding();
    let first = replacement(false, 10);
    let second = replacement(false, 11);
    let other_owner = Binding::replacement(
        Uuid::from_u128(5),
        Uuid::from_u128(10),
        initial.template_fingerprint().into(),
    )
    .unwrap();
    assert_eq!(initial.name(), "cqnayesomb432fipnee");
    assert_eq!(initial.labels().len(), 2);
    assert_eq!(initial.replacement_instance_id(), None);
    assert_eq!(first, replacement(false, 10));
    assert_eq!(first.computer_id(), initial.computer_id());
    assert_eq!(first.template_fingerprint(), initial.template_fingerprint());
    let names: BTreeSet<_> = [&initial, &first, &second, &other_owner]
        .into_iter()
        .map(Binding::name)
        .collect();
    assert_eq!(names.len(), 4);
    for name in names {
        assert_eq!(name.len(), 19);
        assert!(
            name.bytes()
                .all(|b| b.is_ascii_lowercase() || (b'2'..=b'7').contains(&b))
        );
    }
    let labels = first.labels();
    assert_eq!(labels.len(), 3);
    assert_eq!(labels["veoveo-computer"], initial.computer_id().to_string());
    assert_eq!(labels["veoveo-instance"], Uuid::from_u128(10).to_string());
    assert_eq!(
        labels["veoveo-template"],
        initial.labels()["veoveo-template"]
    );
    assert!(labels.values().all(|value| value.len() <= 63));
}

#[test]
fn replacement_rejects_nil_or_reused_identity_and_invalid_fingerprint() {
    let computer = binding().computer_id();
    for (owner, instance, fingerprint) in [
        (Uuid::nil(), Uuid::from_u128(1), "a".repeat(64)),
        (computer, Uuid::nil(), "a".repeat(64)),
        (computer, computer, "a".repeat(64)),
        (computer, Uuid::from_u128(1), "not-a-fingerprint".into()),
    ] {
        assert_eq!(
            Binding::replacement(owner, instance, fingerprint).unwrap_err(),
            RuntimeFailure::BindingMismatch
        );
    }
}

fn observed(value: &Binding) -> api::Sandbox {
    let mut sandbox = sandbox(Phase::Ready);
    let metadata = sandbox.metadata.as_mut().unwrap();
    metadata.name = value.name();
    metadata.labels = value.labels();
    sandbox
}

#[test]
fn reserved_instance_label_is_checked_in_both_directions() {
    for value in [binding(), replacement(false, 10)] {
        assert!(Observation::checked(observed(&value), &value, "computers").is_ok());
        let mut candidate = observed(&value);
        candidate
            .metadata
            .as_mut()
            .unwrap()
            .labels
            .insert("provider-note".into(), "extra".into());
        assert!(Observation::checked(candidate.clone(), &value, "computers").is_ok());
        for changed in [
            None,
            Some(Uuid::from_u128(11).to_string()),
            Some(String::new()),
        ] {
            if value.replacement_instance_id().is_none() && changed.is_none() {
                continue;
            }
            let mut candidate = candidate.clone();
            let labels = &mut candidate.metadata.as_mut().unwrap().labels;
            labels.remove("veoveo-instance");
            if let Some(changed) = changed {
                labels.insert("veoveo-instance".into(), changed);
            }
            assert!(matches!(
                Observation::checked(candidate, &value, "computers"),
                Err(RuntimeFailure::BindingMismatch)
            ));
        }
    }
    // Even a forced short-name collision cannot cross the full Computer identity.
    let correct = replacement(false, 10);
    let wrong = Binding::replacement(
        Uuid::from_u128(5),
        Uuid::from_u128(10),
        correct.template_fingerprint().into(),
    )
    .unwrap();
    let mut forged = observed(&wrong);
    forged.metadata.as_mut().unwrap().name = correct.name();
    assert!(Observation::checked(forged, &correct, "computers").is_err());
}

#[tokio::test]
async fn replacement_without_a_separate_home_fails_before_any_provider_request() {
    let running = Running::start().await;
    let replacement = replacement(false, 10);
    assert!(matches!(
        running.runtime.create(&replacement, &template(false)).await,
        Err(RuntimeFailure::InvalidTemplate)
    ));
    let state = running.fake.0.lock().unwrap();
    assert_eq!(state.gets, 0);
    assert_eq!(state.creates, 0);
}

#[tokio::test]
async fn replacement_create_and_retry_keep_exact_original_home_and_template() {
    let running = Running::start().await;
    let replacement = replacement(true, 10);
    {
        let mut state = running.fake.0.lock().unwrap();
        state.expected_binding = Some(replacement.clone());
        state.sandbox = None;
    }
    let created = running
        .runtime
        .create(&replacement, &template(true))
        .await
        .unwrap();
    let again = running
        .runtime
        .create(&replacement, &template(true))
        .await
        .unwrap();
    assert_eq!(created.sandbox_id, again.sandbox_id);
    let state = running.fake.0.lock().unwrap();
    assert_eq!(state.creates, 1);
    let spec = state.created_spec.as_ref().unwrap();
    assert!(spec == &template(true).spec(binding().computer_id()).unwrap());
    assert!(spec != &template(true).spec(Uuid::from_u128(10)).unwrap());
    assert!(spec.environment.is_empty());
    assert_eq!(state.starts, 0);
    assert_eq!(state.stops, 0);
}

#[tokio::test]
async fn replacement_lifecycle_addresses_exact_instance_without_polling_or_recreation() {
    let running = Running::start().await;
    let runtime = &running.runtime;
    let replacement = replacement(true, 10);
    {
        let mut state = running.fake.0.lock().unwrap();
        state.expected_binding = Some(replacement.clone());
        state.sandbox = None;
    }
    let created = runtime.create(&replacement, &template(true)).await.unwrap();
    let before = running.fake.0.lock().unwrap().gets;
    runtime
        .wait_for(&replacement, &created, Phase::Ready)
        .await
        .unwrap();
    assert_eq!(running.fake.0.lock().unwrap().gets, before);
    let stopping = runtime.stop(&replacement).await.unwrap();
    let before = running.fake.0.lock().unwrap().gets;
    runtime
        .wait_for(&replacement, &stopping, Phase::Stopped)
        .await
        .unwrap();
    assert_eq!(running.fake.0.lock().unwrap().gets, before);
    let starting = runtime.start(&replacement).await.unwrap();
    let before = running.fake.0.lock().unwrap().gets;
    runtime
        .wait_for(&replacement, &starting, Phase::Ready)
        .await
        .unwrap();
    let state = running.fake.0.lock().unwrap();
    assert_eq!(state.gets, before);
    assert_eq!(
        (state.creates, state.starts, state.stops, state.watches),
        (1, 1, 1, 3)
    );
    let sandbox = state.sandbox.as_ref().unwrap();
    assert_eq!(
        sandbox.metadata.as_ref().unwrap().labels,
        replacement.labels()
    );
    assert!(
        sandbox.spec.as_ref().unwrap() == &template(true).spec(binding().computer_id()).unwrap()
    );
}

#[tokio::test]
async fn replacement_watch_rejects_another_full_instance_even_when_name_and_provider_id_match() {
    let running = Running::start().await;
    let replacement = replacement(true, 10);
    {
        let mut state = running.fake.0.lock().unwrap();
        state.expected_binding = Some(replacement.clone());
        state.sandbox = None;
        state.watch = 5;
    }
    let created = running
        .runtime
        .create(&replacement, &template(true))
        .await
        .unwrap();
    let before = running.fake.0.lock().unwrap().gets;
    assert!(matches!(
        running
            .runtime
            .wait_for(&replacement, &created, Phase::Ready)
            .await,
        Err(RuntimeFailure::BindingMismatch)
    ));
    let state = running.fake.0.lock().unwrap();
    assert_eq!(state.gets, before);
    assert_eq!(state.watches, 1);
    assert_eq!((state.starts, state.stops), (0, 0));
}

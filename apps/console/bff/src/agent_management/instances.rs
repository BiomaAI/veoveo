use super::*;

pub(super) fn router() -> Router<AppState> {
    Router::new()
        .route("/agent-instances", get(list).post(provision))
        .route("/agent-instances/{id}", get(read).patch(update))
        .route("/agent-operations/{id}", get(operation))
}

async fn list(
    State(state): State<AppState>,
    Query(page): Query<Page>,
    headers: HeaderMap,
) -> Response {
    forward::<(), wire::InstancePage>(&state, &headers, Method::GET, "agent-instances", page, None)
        .await
}

async fn read(
    State(state): State<AppState>,
    Path(id): Path<wire::AgentManagedInstanceId>,
    headers: HeaderMap,
) -> Response {
    forward::<(), wire::ManagedInstance>(
        &state,
        &headers,
        Method::GET,
        &format!("agent-instances/{id}"),
        Page::default(),
        None,
    )
    .await
}

async fn operation(
    State(state): State<AppState>,
    Path(id): Path<uuid::Uuid>,
    headers: HeaderMap,
) -> Response {
    forward::<(), wire::LifecycleOperation>(
        &state,
        &headers,
        Method::GET,
        &format!("agent-operations/{id}"),
        Page::default(),
        None,
    )
    .await
}

async fn provision(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<wire::ProvisionInstance>,
) -> Response {
    forward::<_, wire::LifecycleOperation>(
        &state,
        &headers,
        Method::POST,
        "agent-instances",
        Page::default(),
        Some(&request),
    )
    .await
}

async fn update(
    State(state): State<AppState>,
    Path(id): Path<wire::AgentManagedInstanceId>,
    headers: HeaderMap,
    Json(request): Json<wire::UpdateInstance>,
) -> Response {
    forward::<_, wire::LifecycleOperation>(
        &state,
        &headers,
        Method::PATCH,
        &format!("agent-instances/{id}"),
        Page::default(),
        Some(&request),
    )
    .await
}

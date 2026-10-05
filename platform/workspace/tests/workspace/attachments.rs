use super::*;
use chrono::{TimeDelta, Utc};
use veoveo_platform_store::ArtifactId;
use veoveo_workspace::persistence::WorkspaceRepository;
use veoveo_workspace::persistence::{WorkspaceAttachment, WorkspaceTurnRequest};

fn request(id: WorkspaceMessageId, attachments: Vec<WorkspaceAttachment>) -> WorkspaceTurnRequest {
    WorkspaceTurnRequest {
        id,
        text: String::new(),
        attachments,
        reply_to: None,
        addressed_agents: vec![],
        deadline: Utc::now() + TimeDelta::seconds(120),
    }
}

#[tokio::test]
async fn attachment_references_are_immutable_bounded_and_membership_scoped_without_grants() {
    tokio::time::timeout(std::time::Duration::from_secs(30), async {
        let db = TestDb::with_modules(vec![
            veoveo_agent_runtime::schema::module_setup(
                fixture::module_lanes::execution("agents").unwrap(),
            )
            .unwrap(),
            veoveo_workspace::schema::module_setup(
                fixture::module_lanes::execution("workspace").unwrap(),
            )
            .unwrap(),
        ])
        .await;
        let alice = identity(&db.a, "alice").await;
        let bob = identity(&db.a, "bob").await;
        context(&db.a, &alice, "shared").await;
        let a = authority(&db.a, &alice, "shared").await;
        let b = authority(&db.b, &bob, "shared").await;
        let chat = WorkspaceChatId::new();
        WorkspaceRepository::new(db.a.clone())
            .create_workspace_chat(&a, chat, "Files")
            .await
            .unwrap();
        join(&db.a, &a, &b, &bob, chat).await;
        // A reference is not proof that an object exists or that this reader can
        // access it. The message publishes only the sender's UUID and label.
        let file = WorkspaceAttachment {
            artifact: ArtifactId::new(),
            name: "Shared label <script>".into(),
        };
        let id = WorkspaceMessageId::new();
        let workspace_repository_a = WorkspaceRepository::new(db.a.clone());
        let workspace_repository_b = WorkspaceRepository::new(db.b.clone());
        let (first, retry) = tokio::join!(
            workspace_repository_a.send_workspace_turn(&a, chat, request(id, vec![file.clone()])),
            workspace_repository_b.send_workspace_turn(&a, chat, request(id, vec![file.clone()])),
        );
        let first = first.unwrap();
        assert_eq!(first, retry.unwrap());
        assert_eq!(first.message.attachments, Some(vec![file.clone()]));
        assert!(first.message.text.is_empty());
        assert!(first.runs.is_empty());
        let snapshot = WorkspaceRepository::new(db.b.clone())
            .workspace_recent_snapshot(&b, chat, None, 1)
            .await
            .unwrap();
        assert_eq!(snapshot.messages, vec![first.message.clone()]);
        let object: Option<surrealdb::types::Value> =
            db.a.client()
                .select(file.artifact.record_id())
                .await
                .unwrap();
        assert!(
            object.is_none(),
            "a chat attachment must never create an Artifact or grant"
        );
        for changed in [
            WorkspaceAttachment {
                name: "Changed label".into(),
                ..file.clone()
            },
            WorkspaceAttachment {
                artifact: ArtifactId::new(),
                ..file.clone()
            },
        ] {
            assert_eq!(
                WorkspaceRepository::new(db.a.clone())
                    .send_workspace_turn(&a, chat, request(id, vec![changed]))
                    .await,
                Err(WorkspaceError::Conflict)
            );
        }
        assert_eq!(
            WorkspaceRepository::new(db.b.clone())
                .send_workspace_turn(&b, chat, request(id, vec![file.clone()]))
                .await,
            Err(WorkspaceError::Conflict)
        );
        let head = WorkspaceRepository::new(db.a.clone())
            .workspace_head(&a, chat)
            .await
            .unwrap();
        for invalid in [
            vec![],
            vec![file.clone(); 2],
            vec![file.clone(); 9],
            vec![WorkspaceAttachment {
                name: "x".repeat(256),
                ..file.clone()
            }],
            vec![WorkspaceAttachment {
                name: "bad\nlabel".into(),
                ..file.clone()
            }],
            vec![WorkspaceAttachment {
                artifact: ArtifactId::from_uuid(uuid::Uuid::nil()),
                ..file.clone()
            }],
        ] {
            assert!(matches!(
                WorkspaceRepository::new(db.a.clone())
                    .send_workspace_turn(&a, chat, request(WorkspaceMessageId::new(), invalid))
                    .await,
                Err(WorkspaceError::Invalid(_))
            ));
        }
        assert_eq!(
            WorkspaceRepository::new(db.a.clone())
                .workspace_head(&a, chat)
                .await
                .unwrap(),
            head
        );
        WorkspaceRepository::new(db.a.clone())
            .remove_workspace_member(&a, chat, bob.principal_id)
            .await
            .unwrap();
        assert_eq!(
            WorkspaceRepository::new(db.b.clone())
                .workspace_recent_snapshot(&b, chat, None, 1)
                .await,
            Err(WorkspaceError::NotFound)
        );
        assert_eq!(
            WorkspaceRepository::new(db.b.clone())
                .send_workspace_turn(
                    &b,
                    chat,
                    request(WorkspaceMessageId::new(), vec![file.clone()])
                )
                .await,
            Err(WorkspaceError::NotFound)
        );
        assert_eq!(
            WorkspaceRepository::new(db.b.clone())
                .send_workspace_turn(&a, chat, request(id, vec![file]))
                .await
                .unwrap(),
            first
        );
    })
    .await
    .expect("attachment qualification is bounded");
}

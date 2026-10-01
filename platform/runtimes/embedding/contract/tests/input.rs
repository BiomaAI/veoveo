use veoveo_embedding_contract::*;

#[test]
fn typed_inputs_and_wire_admission_share_byte_and_count_bounds() {
    for text in [String::new(), " \n\t".into(), "é".repeat(8193)] {
        assert!(EmbeddingText::new(text.clone()).is_err());
        assert!(serde_json::from_value::<EmbeddingText>(serde_json::json!(text)).is_err());
    }
    let text = EmbeddingText::new("é".repeat(8192)).unwrap();
    assert!(EmbeddingBatch::new(vec![text.clone(); 8]).is_ok());
    assert!(EmbeddingBatch::new(vec![text; 9]).is_err());
    let tiny = EmbeddingText::new("x").unwrap();
    assert!(EmbeddingBatch::new(vec![tiny.clone(); 32]).is_ok());
    assert!(EmbeddingBatch::new(vec![tiny; 33]).is_err());
    assert!(serde_json::from_value::<EmbeddingBatch>(serde_json::json!([])).is_err());
    assert!(serde_json::from_value::<EmbeddingBatch>(serde_json::json!(vec!["x"; 33])).is_err());
    for task in [
        String::new(),
        " \t".into(),
        "task\nQuery:injection".into(),
        "a".repeat(1025),
    ] {
        assert!(EmbeddingTask::new(task.clone()).is_err());
        assert!(serde_json::from_value::<EmbeddingTask>(serde_json::json!(task)).is_err());
    }
    let batch = EmbeddingBatch::new(vec![EmbeddingText::new("a\n文").unwrap()]).unwrap();
    assert_eq!(
        serde_json::from_value::<EmbeddingBatch>(serde_json::to_value(&batch).unwrap()).unwrap(),
        batch
    );
    assert_eq!(
        serde_json::to_value(schemars::schema_for!(EmbeddingBatch)).unwrap()["maxItems"],
        32
    );
}

#[path = "../../../../../testing/fixtures/embedding.rs"]
mod embedding_fixture;
use veoveo_embedding_contract::*;

#[test]
fn vector_admission_binds_dimension_and_normalization() {
    let space = EmbeddingSpace {
        model: EmbeddingModelId::parse("synthetic").unwrap(),
        revision: EmbeddingModelRevision::parse("r1").unwrap(),
        dimension: EmbeddingDimension::new(3).unwrap(),
        pooling: EmbeddingPooling::LastToken,
        normalization: EmbeddingNormalization::L2,
        precision: EmbeddingPrecision::Float32,
        max_input_tokens: EmbeddingMaxInputTokens::new(8192).unwrap(),
    };
    assert!(
        EmbeddingVector::new(&embedding_fixture::runtime(space.clone()), vec![1., 0., 0.]).is_ok()
    );
    for values in [
        vec![1., 0.],
        vec![0., 0., 0.],
        vec![2., 0., 0.],
        vec![f32::NAN, 0., 0.],
    ] {
        assert!(EmbeddingVector::new(&embedding_fixture::runtime(space.clone()), values).is_err());
    }
    assert!(EmbeddingDimension::new(0).is_err());
    assert!(EmbeddingDimension::new(8193).is_err());
    assert!(EmbeddingModelRevision::parse("r1\nsecret").is_err());
}

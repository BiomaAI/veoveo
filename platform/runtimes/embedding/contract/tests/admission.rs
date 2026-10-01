use veoveo_embedding_contract::*;
use veoveo_types::Sha256Digest;

#[test]
fn vector_admission_binds_dimension_and_normalization() {
    let space = EmbeddingSpace {
        model: EmbeddingModelId::new("synthetic").unwrap(),
        revision: EmbeddingModelRevision::new("r1").unwrap(),
        dimension: EmbeddingDimension::new(3).unwrap(),
        runtime_image: Sha256Digest::from_bytes([1; 32]),
    };
    assert!(EmbeddingVector::new(space.clone(), vec![1., 0., 0.]).is_ok());
    for values in [
        vec![1., 0.],
        vec![0., 0., 0.],
        vec![2., 0., 0.],
        vec![f32::NAN, 0., 0.],
    ] {
        assert!(EmbeddingVector::new(space.clone(), values).is_err());
    }
    assert!(EmbeddingDimension::new(0).is_err());
    assert!(EmbeddingDimension::new(8193).is_err());
    assert!(EmbeddingModelRevision::new("r1\nsecret").is_err());
}

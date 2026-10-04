/// Reason declares no additional domain scopes. Gateway operation policy and
/// current owner and label checks still authorize every operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, veoveo_types::Vocabulary)]
#[vocabulary(scope)]
pub enum ReasonScope {}

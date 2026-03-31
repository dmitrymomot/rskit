pub fn require_feature(_name: &str) -> RequireFeatureLayer {
    todo!()
}

pub fn require_limit<F, Fut>(_name: &str, _usage: F) -> RequireLimitLayer
where
    F: Send,
    Fut: Send,
{
    todo!()
}

pub struct RequireFeatureLayer;
pub struct RequireLimitLayer;

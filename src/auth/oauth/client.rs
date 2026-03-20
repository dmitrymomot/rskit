use serde::de::DeserializeOwned;

#[allow(dead_code)]
pub(crate) async fn post_form<T: DeserializeOwned>(
    _url: &str,
    _params: &[(&str, &str)],
) -> crate::Result<T> {
    todo!()
}

#[allow(dead_code)]
pub(crate) async fn get_json<T: DeserializeOwned>(_url: &str, _token: &str) -> crate::Result<T> {
    todo!()
}

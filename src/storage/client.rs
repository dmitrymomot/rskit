use std::time::Duration;

use bytes::Bytes;

use super::options::PutOptions;
use super::presign::{PresignParams, presign_url};
use super::signing::{SigningParams, sign_request, uri_encode};
use crate::error::{Error, Result};

pub(crate) struct RemoteBackend {
    client: reqwest::Client,
    bucket: String,
    endpoint: String,
    endpoint_host: String,
    access_key: String,
    secret_key: String,
    region: String,
    path_style: bool,
}

/// SHA-256 hash of an empty body (used for DELETE, HEAD, GET).
const EMPTY_SHA256: &str = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";

impl RemoteBackend {
    pub fn new(
        client: reqwest::Client,
        bucket: String,
        endpoint: String,
        access_key: String,
        secret_key: String,
        region: String,
        path_style: bool,
    ) -> Result<Self> {
        let endpoint_host = strip_scheme(&endpoint).to_string();

        Ok(Self {
            client,
            bucket,
            endpoint,
            endpoint_host,
            access_key,
            secret_key,
            region,
            path_style,
        })
    }

    pub async fn put(
        &self,
        key: &str,
        data: Bytes,
        content_type: &str,
        opts: &PutOptions,
    ) -> Result<()> {
        let (url, host) = self.url_and_host(key);
        let canonical_uri = self.canonical_uri(key);

        let mut extra_headers = vec![("content-type".to_string(), content_type.to_string())];
        if let Some(ref cd) = opts.content_disposition {
            extra_headers.push(("content-disposition".to_string(), cd.clone()));
        }
        if let Some(ref cc) = opts.cache_control {
            extra_headers.push(("cache-control".to_string(), cc.clone()));
        }
        if let Some(acl) = &opts.acl {
            extra_headers.push(("x-amz-acl".to_string(), acl.as_header_value().to_string()));
        }

        let params = SigningParams {
            access_key: &self.access_key,
            secret_key: &self.secret_key,
            region: &self.region,
            method: "PUT",
            canonical_uri: &canonical_uri,
            host: &host,
            query_string: "",
            extra_headers: &extra_headers,
            payload_hash: "UNSIGNED-PAYLOAD",
            now: chrono::Utc::now(),
        };
        let (auth, signed_headers) = sign_request(&params);

        let content_length = data.len();
        let mut req = self.client.put(&url);
        for (k, v) in &signed_headers {
            req = req.header(k.as_str(), v.as_str());
        }
        req = req
            .header("authorization", &auth)
            .header("content-length", content_length)
            .body(data);

        let response = req
            .send()
            .await
            .map_err(|e| Error::internal(format!("PUT request failed: {e}")))?;

        let status = response.status();
        if !status.is_success() {
            let body_str = response.text().await.unwrap_or_default();
            return Err(Error::internal(format!(
                "PUT failed ({status}): {body_str}"
            )));
        }

        Ok(())
    }

    pub async fn delete(&self, key: &str) -> Result<()> {
        let (url, host) = self.url_and_host(key);
        let canonical_uri = self.canonical_uri(key);

        let params = SigningParams {
            access_key: &self.access_key,
            secret_key: &self.secret_key,
            region: &self.region,
            method: "DELETE",
            canonical_uri: &canonical_uri,
            host: &host,
            query_string: "",
            extra_headers: &[],
            payload_hash: EMPTY_SHA256,
            now: chrono::Utc::now(),
        };
        let (auth, signed_headers) = sign_request(&params);

        let mut req = self.client.delete(&url);
        for (k, v) in &signed_headers {
            req = req.header(k.as_str(), v.as_str());
        }
        req = req.header("authorization", &auth);

        let response = req
            .send()
            .await
            .map_err(|e| Error::internal(format!("DELETE request failed: {e}")))?;

        let status = response.status();
        if !status.is_success() {
            let body_str = response.text().await.unwrap_or_default();
            return Err(Error::internal(format!(
                "DELETE failed ({status}): {body_str}"
            )));
        }

        Ok(())
    }

    pub async fn exists(&self, key: &str) -> Result<bool> {
        let (url, host) = self.url_and_host(key);
        let canonical_uri = self.canonical_uri(key);

        let params = SigningParams {
            access_key: &self.access_key,
            secret_key: &self.secret_key,
            region: &self.region,
            method: "HEAD",
            canonical_uri: &canonical_uri,
            host: &host,
            query_string: "",
            extra_headers: &[],
            payload_hash: EMPTY_SHA256,
            now: chrono::Utc::now(),
        };
        let (auth, signed_headers) = sign_request(&params);

        let mut req = self.client.head(&url);
        for (k, v) in &signed_headers {
            req = req.header(k.as_str(), v.as_str());
        }
        req = req.header("authorization", &auth);

        let response = req
            .send()
            .await
            .map_err(|e| Error::internal(format!("HEAD request failed: {e}")))?;

        match response.status() {
            s if s.is_success() => Ok(true),
            s if s == reqwest::StatusCode::NOT_FOUND => Ok(false),
            status => Err(Error::internal(format!("HEAD failed ({status})"))),
        }
    }

    pub async fn list(&self, prefix: &str) -> Result<Vec<String>> {
        let mut all_keys = Vec::new();
        let mut continuation_token: Option<String> = None;

        loop {
            let mut query = format!("list-type=2&prefix={}", uri_encode(prefix, true));
            if let Some(ref token) = continuation_token {
                query.push_str(&format!("&continuation-token={}", uri_encode(token, true)));
            }

            // List is always at bucket root
            let (base_url, host) = if self.path_style {
                (
                    format!("{}/{}?{}", self.endpoint, self.bucket, query),
                    self.endpoint_host.clone(),
                )
            } else {
                (
                    format!("https://{}.{}/?{}", self.bucket, self.endpoint_host, query),
                    format!("{}.{}", self.bucket, self.endpoint_host),
                )
            };
            let canonical_uri = if self.path_style {
                format!("/{}", self.bucket)
            } else {
                "/".to_string()
            };

            let params = SigningParams {
                access_key: &self.access_key,
                secret_key: &self.secret_key,
                region: &self.region,
                method: "GET",
                canonical_uri: &canonical_uri,
                host: &host,
                query_string: &query,
                extra_headers: &[],
                payload_hash: EMPTY_SHA256,
                now: chrono::Utc::now(),
            };
            let (auth, signed_headers) = sign_request(&params);

            let mut req = self.client.get(&base_url);
            for (k, v) in &signed_headers {
                req = req.header(k.as_str(), v.as_str());
            }
            req = req.header("authorization", &auth);

            let response = req
                .send()
                .await
                .map_err(|e| Error::internal(format!("LIST request failed: {e}")))?;

            let status = response.status();
            let body = response
                .bytes()
                .await
                .map_err(|e| Error::internal(format!("failed to read response: {e}")))?;

            if !status.is_success() {
                let body_str = String::from_utf8_lossy(&body);
                return Err(Error::internal(format!(
                    "LIST failed ({status}): {body_str}"
                )));
            }

            let body_str = String::from_utf8_lossy(&body);

            // Hand-parse <Key>...</Key> values
            for key in extract_xml_values(&body_str, "Key") {
                all_keys.push(key);
            }

            // Check pagination
            let is_truncated = extract_xml_value(&body_str, "IsTruncated")
                .map(|v| v == "true")
                .unwrap_or(false);

            if is_truncated {
                continuation_token = extract_xml_value(&body_str, "NextContinuationToken");
            } else {
                break;
            }
        }

        Ok(all_keys)
    }

    pub async fn presigned_url(&self, key: &str, expires_in: Duration) -> Result<String> {
        let params = PresignParams {
            access_key: &self.access_key,
            secret_key: &self.secret_key,
            region: &self.region,
            bucket: &self.bucket,
            key,
            endpoint: &self.endpoint,
            endpoint_host: &self.endpoint_host,
            path_style: self.path_style,
            expires_in,
            now: chrono::Utc::now(),
        };
        Ok(presign_url(&params))
    }

    fn url_and_host(&self, key: &str) -> (String, String) {
        build_url_and_host(
            &self.endpoint,
            &self.endpoint_host,
            &self.bucket,
            key,
            self.path_style,
        )
    }

    fn canonical_uri(&self, key: &str) -> String {
        build_canonical_uri(&self.bucket, key, self.path_style)
    }
}

// Free functions exposed for unit tests

fn build_url_and_host(
    endpoint: &str,
    endpoint_host: &str,
    bucket: &str,
    key: &str,
    path_style: bool,
) -> (String, String) {
    let encoded_key = uri_encode(key, false);
    if path_style {
        (
            format!("{endpoint}/{bucket}/{encoded_key}"),
            endpoint_host.to_string(),
        )
    } else {
        (
            format!("https://{bucket}.{endpoint_host}/{encoded_key}"),
            format!("{bucket}.{endpoint_host}"),
        )
    }
}

fn build_canonical_uri(bucket: &str, key: &str, path_style: bool) -> String {
    let encoded_key = uri_encode(key, false);
    if path_style {
        format!("/{bucket}/{encoded_key}")
    } else {
        format!("/{encoded_key}")
    }
}

fn strip_scheme(endpoint: &str) -> &str {
    endpoint
        .strip_prefix("https://")
        .or_else(|| endpoint.strip_prefix("http://"))
        .unwrap_or(endpoint)
}

/// Extract all values between `<tag>` and `</tag>` from XML.
fn extract_xml_values(xml: &str, tag: &str) -> Vec<String> {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let mut values = Vec::new();
    let mut search_from = 0;
    while let Some(start) = xml[search_from..].find(&open) {
        let abs_start = search_from + start + open.len();
        if let Some(end) = xml[abs_start..].find(&close) {
            values.push(xml[abs_start..abs_start + end].to_string());
            search_from = abs_start + end + close.len();
        } else {
            break;
        }
    }
    values
}

/// Extract a single value between `<tag>` and `</tag>`.
fn extract_xml_value(xml: &str, tag: &str) -> Option<String> {
    extract_xml_values(xml, tag).into_iter().next()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn url_path_style() {
        let (url, _) = build_url_and_host(
            "https://s3.example.com",
            "s3.example.com",
            "mybucket",
            "photos/cat.jpg",
            true,
        );
        assert_eq!(url, "https://s3.example.com/mybucket/photos/cat.jpg");
    }

    #[test]
    fn host_path_style() {
        let (_, host) = build_url_and_host(
            "https://s3.example.com",
            "s3.example.com",
            "mybucket",
            "photos/cat.jpg",
            true,
        );
        assert_eq!(host, "s3.example.com");
    }

    #[test]
    fn url_virtual_hosted() {
        let (url, _) = build_url_and_host(
            "https://s3.example.com",
            "s3.example.com",
            "mybucket",
            "photos/cat.jpg",
            false,
        );
        assert_eq!(url, "https://mybucket.s3.example.com/photos/cat.jpg");
    }

    #[test]
    fn host_virtual_hosted() {
        let (_, host) = build_url_and_host(
            "https://s3.example.com",
            "s3.example.com",
            "mybucket",
            "photos/cat.jpg",
            false,
        );
        assert_eq!(host, "mybucket.s3.example.com");
    }

    #[test]
    fn canonical_uri_path_style() {
        let uri = build_canonical_uri("mybucket", "photos/cat.jpg", true);
        assert_eq!(uri, "/mybucket/photos/cat.jpg");
    }

    #[test]
    fn canonical_uri_virtual_hosted() {
        let uri = build_canonical_uri("mybucket", "photos/cat.jpg", false);
        assert_eq!(uri, "/photos/cat.jpg");
    }

    // -- XML parsing --

    #[test]
    fn extract_single_value() {
        let xml = "<Key>photos/cat.jpg</Key>";
        assert_eq!(extract_xml_values(xml, "Key"), vec!["photos/cat.jpg"]);
    }

    #[test]
    fn extract_multiple_values() {
        let xml = "<r><Key>a.txt</Key><Key>b.txt</Key></r>";
        assert_eq!(extract_xml_values(xml, "Key"), vec!["a.txt", "b.txt"]);
    }

    #[test]
    fn extract_missing_tag() {
        let xml = "<Bucket>test</Bucket>";
        assert!(extract_xml_values(xml, "Key").is_empty());
    }

    #[test]
    fn extract_empty_value() {
        let xml = "<Key></Key>";
        assert_eq!(extract_xml_values(xml, "Key"), vec![""]);
    }

    #[test]
    fn extract_ignores_unrelated_tags() {
        let xml = "<ListBucketResult><Bucket>test</Bucket><Contents><Key>file.txt</Key></Contents></ListBucketResult>";
        assert_eq!(extract_xml_values(xml, "Key"), vec!["file.txt"]);
        assert_eq!(extract_xml_values(xml, "Bucket"), vec!["test"]);
    }

    #[test]
    fn extract_no_close_tag() {
        let xml = "<Key>broken";
        assert!(extract_xml_values(xml, "Key").is_empty());
    }

    #[test]
    fn extract_single_value_helper() {
        let xml = "<IsTruncated>false</IsTruncated>";
        assert_eq!(
            extract_xml_value(xml, "IsTruncated"),
            Some("false".to_string())
        );
    }

    #[test]
    fn extract_single_value_helper_missing() {
        assert_eq!(extract_xml_value("<a>b</a>", "Key"), None);
    }

    #[test]
    fn endpoint_host_strips_https() {
        assert_eq!(strip_scheme("https://s3.example.com"), "s3.example.com");
    }

    #[test]
    fn endpoint_host_strips_http() {
        assert_eq!(strip_scheme("http://localhost:9000"), "localhost:9000");
    }

    #[test]
    fn endpoint_host_no_scheme() {
        assert_eq!(strip_scheme("s3.example.com"), "s3.example.com");
    }
}

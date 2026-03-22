use std::time::Duration;

use super::signing::{derive_signing_key, hex_encode, hmac_sha256, sha256_hex, uri_encode};

#[allow(dead_code)]
pub(crate) struct PresignParams<'a> {
    pub access_key: &'a str,
    pub secret_key: &'a str,
    pub region: &'a str,
    pub bucket: &'a str,
    pub key: &'a str,
    pub endpoint: &'a str,
    pub endpoint_host: &'a str,
    pub path_style: bool,
    pub expires_in: Duration,
    pub now: chrono::DateTime<chrono::Utc>,
}

#[allow(dead_code)]
pub(crate) fn presign_url(params: &PresignParams) -> String {
    let date_stamp = params.now.format("%Y%m%d").to_string();
    let amz_date = params.now.format("%Y%m%dT%H%M%SZ").to_string();
    let scope = format!("{}/{}/s3/aws4_request", date_stamp, params.region);
    let credential = format!("{}/{}", params.access_key, scope);
    let expires = params.expires_in.as_secs();

    // Build URL and host based on path_style
    let encoded_key = uri_encode(params.key, false);
    let (base_url, canonical_uri, host) = if params.path_style {
        (
            format!("{}/{}/{}", params.endpoint, params.bucket, encoded_key),
            format!("/{}/{}", params.bucket, encoded_key),
            params.endpoint_host.to_string(),
        )
    } else {
        (
            format!(
                "https://{}.{}/{}",
                params.bucket, params.endpoint_host, encoded_key
            ),
            format!("/{}", encoded_key),
            format!("{}.{}", params.bucket, params.endpoint_host),
        )
    };

    // Query parameters (alphabetically sorted, excluding X-Amz-Signature)
    let query_string = format!(
        "X-Amz-Algorithm=AWS4-HMAC-SHA256\
         &X-Amz-Credential={}\
         &X-Amz-Date={}\
         &X-Amz-Expires={}\
         &X-Amz-SignedHeaders=host",
        uri_encode(&credential, true),
        amz_date,
        expires,
    );

    // Canonical request (presigned uses UNSIGNED-PAYLOAD)
    let canonical_request = format!(
        "GET\n{}\n{}\nhost:{}\n\nhost\nUNSIGNED-PAYLOAD",
        canonical_uri, query_string, host,
    );

    // String to sign
    let canonical_request_hash = sha256_hex(canonical_request.as_bytes());
    let string_to_sign = format!(
        "AWS4-HMAC-SHA256\n{}\n{}\n{}",
        amz_date, scope, canonical_request_hash
    );

    // Derive signing key and compute signature
    let signing_key = derive_signing_key(params.secret_key, &date_stamp, params.region);
    let signature = hex_encode(&hmac_sha256(&signing_key, string_to_sign.as_bytes()));

    format!("{base_url}?{query_string}&X-Amz-Signature={signature}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};

    const ACCESS_KEY: &str = "AKIAIOSFODNN7EXAMPLE";
    const SECRET_KEY: &str = "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY";
    const REGION: &str = "us-east-1";

    fn test_time() -> chrono::DateTime<chrono::Utc> {
        Utc.with_ymd_and_hms(2013, 5, 24, 0, 0, 0).unwrap()
    }

    #[test]
    fn presign_path_style() {
        let params = PresignParams {
            access_key: ACCESS_KEY,
            secret_key: SECRET_KEY,
            region: REGION,
            bucket: "examplebucket",
            key: "test.txt",
            endpoint: "https://s3.amazonaws.com",
            endpoint_host: "s3.amazonaws.com",
            path_style: true,
            expires_in: Duration::from_secs(86400),
            now: test_time(),
        };
        let url = presign_url(&params);
        assert!(
            url.starts_with("https://s3.amazonaws.com/examplebucket/test.txt?"),
            "url: {url}"
        );
        assert!(url.contains("X-Amz-Algorithm=AWS4-HMAC-SHA256"));
        assert!(url.contains("X-Amz-Expires=86400"));
        assert!(url.contains("X-Amz-SignedHeaders=host"));
        assert!(url.contains("X-Amz-Credential=AKIAIOSFODNN7EXAMPLE"));
        assert!(url.contains("X-Amz-Signature="));
    }

    #[test]
    fn presign_virtual_hosted() {
        let params = PresignParams {
            access_key: ACCESS_KEY,
            secret_key: SECRET_KEY,
            region: REGION,
            bucket: "examplebucket",
            key: "test.txt",
            endpoint: "https://s3.amazonaws.com",
            endpoint_host: "s3.amazonaws.com",
            path_style: false,
            expires_in: Duration::from_secs(3600),
            now: test_time(),
        };
        let url = presign_url(&params);
        assert!(
            url.starts_with("https://examplebucket.s3.amazonaws.com/test.txt?"),
            "url: {url}"
        );
        assert!(url.contains("X-Amz-SignedHeaders=host"));
    }

    #[test]
    fn presign_encodes_special_chars_in_key() {
        let params = PresignParams {
            access_key: ACCESS_KEY,
            secret_key: SECRET_KEY,
            region: REGION,
            bucket: "bucket",
            key: "path/to/file with spaces.txt",
            endpoint: "https://s3.amazonaws.com",
            endpoint_host: "s3.amazonaws.com",
            path_style: true,
            expires_in: Duration::from_secs(3600),
            now: test_time(),
        };
        let url = presign_url(&params);
        assert!(url.contains("file%20with%20spaces.txt"), "url: {url}");
        assert!(url.contains("path/to/"), "url: {url}");
    }
}

use hmac::{Hmac, Mac};
use sha2::Sha256;

type HmacSha256 = Hmac<Sha256>;

/// Parameters needed to sign an S3 request.
pub(crate) struct SigningParams<'a> {
    pub access_key: &'a str,
    pub secret_key: &'a str,
    pub region: &'a str,
    pub method: &'a str,
    pub canonical_uri: &'a str,
    pub host: &'a str,
    pub query_string: &'a str,
    pub extra_headers: &'a [(String, String)],
    pub payload_hash: &'a str,
    pub now: chrono::DateTime<chrono::Utc>,
}

pub(crate) fn sha256_hex(data: &[u8]) -> String {
    crate::encoding::hex::sha256(data)
}

/// URI-encode per AWS spec. Encodes everything except A-Za-z0-9_.-~.
/// If `encode_slash` is true, '/' is also encoded.
pub(crate) fn uri_encode(input: &str, encode_slash: bool) -> String {
    let mut result = String::with_capacity(input.len() * 2);
    for byte in input.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'_' | b'-' | b'.' | b'~' => {
                result.push(byte as char);
            }
            b'/' if !encode_slash => {
                result.push('/');
            }
            _ => {
                result.push_str(&format!("%{byte:02X}"));
            }
        }
    }
    result
}

/// Sign an S3 request using AWS SigV4.
/// Returns (authorization_header_value, all_headers_to_add).
pub(crate) fn sign_request(params: &SigningParams) -> (String, Vec<(String, String)>) {
    let date_stamp = params.now.format("%Y%m%d").to_string();
    let amz_date = params.now.format("%Y%m%dT%H%M%SZ").to_string();
    let scope = format!("{}/{}/s3/aws4_request", date_stamp, params.region);

    // Build all headers that will be signed
    let mut headers: Vec<(String, String)> = Vec::new();
    headers.push(("host".to_string(), params.host.to_string()));
    headers.push((
        "x-amz-content-sha256".to_string(),
        params.payload_hash.to_string(),
    ));
    headers.push(("x-amz-date".to_string(), amz_date.clone()));
    for (k, v) in params.extra_headers {
        headers.push((k.to_lowercase(), v.to_string()));
    }
    headers.sort_by(|a, b| a.0.cmp(&b.0));

    // Canonical headers string
    let canonical_headers: String = headers.iter().map(|(k, v)| format!("{k}:{v}\n")).collect();

    // Signed headers string
    let signed_headers: String = headers
        .iter()
        .map(|(k, _)| k.as_str())
        .collect::<Vec<_>>()
        .join(";");

    // Sort query string parameters alphabetically (SigV4 requirement)
    let sorted_query_string = if params.query_string.is_empty() {
        String::new()
    } else {
        let mut pairs: Vec<&str> = params.query_string.split('&').collect();
        pairs.sort();
        pairs.join("&")
    };

    // Canonical request
    let canonical_request = format!(
        "{}\n{}\n{}\n{}\n{}\n{}",
        params.method,
        params.canonical_uri,
        sorted_query_string,
        canonical_headers,
        signed_headers,
        params.payload_hash,
    );

    // String to sign
    let canonical_request_hash = sha256_hex(canonical_request.as_bytes());
    let string_to_sign = format!(
        "AWS4-HMAC-SHA256\n{}\n{}\n{}",
        amz_date, scope, canonical_request_hash
    );

    // Signing key
    let signing_key = derive_signing_key(params.secret_key, &date_stamp, params.region);

    // Signature
    let signature =
        crate::encoding::hex::encode(&hmac_sha256(&signing_key, string_to_sign.as_bytes()));

    // Authorization header
    let authorization = format!(
        "AWS4-HMAC-SHA256 Credential={}/{},SignedHeaders={},Signature={}",
        params.access_key, scope, signed_headers, signature
    );

    (authorization, headers)
}

pub(crate) fn derive_signing_key(secret_key: &str, date_stamp: &str, region: &str) -> Vec<u8> {
    let k_date = hmac_sha256(
        format!("AWS4{secret_key}").as_bytes(),
        date_stamp.as_bytes(),
    );
    let k_region = hmac_sha256(&k_date, region.as_bytes());
    let k_service = hmac_sha256(&k_region, b"s3");
    hmac_sha256(&k_service, b"aws4_request")
}

pub(crate) fn hmac_sha256(key: &[u8], data: &[u8]) -> Vec<u8> {
    let mut mac = HmacSha256::new_from_slice(key).expect("HMAC accepts any key length");
    mac.update(data);
    mac.finalize().into_bytes().to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    const ACCESS_KEY: &str = "AKIAIOSFODNN7EXAMPLE";
    const SECRET_KEY: &str = "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY";
    const REGION: &str = "us-east-1";
    const EMPTY_HASH: &str = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";

    fn test_time() -> chrono::DateTime<chrono::Utc> {
        chrono::Utc.with_ymd_and_hms(2013, 5, 24, 0, 0, 0).unwrap()
    }

    #[test]
    fn sha256_hex_empty_body() {
        assert_eq!(sha256_hex(b""), EMPTY_HASH);
    }

    #[test]
    fn sha256_hex_payload() {
        assert_eq!(
            sha256_hex(b"Welcome to Amazon S3."),
            "44ce7dd67c959e0d3524ffac1771dfbba87d2b6b4b4e99e42034a8b803f8b072"
        );
    }

    #[test]
    fn uri_encode_preserves_unreserved() {
        assert_eq!(uri_encode("test-file_name.txt", true), "test-file_name.txt");
    }

    #[test]
    fn uri_encode_encodes_dollar() {
        assert_eq!(uri_encode("test$file.text", true), "test%24file.text");
    }

    #[test]
    fn uri_encode_encodes_slash_when_requested() {
        assert_eq!(uri_encode("a/b", true), "a%2Fb");
    }

    #[test]
    fn uri_encode_preserves_slash_when_not_requested() {
        assert_eq!(uri_encode("a/b", false), "a/b");
    }

    #[test]
    fn sign_get_object() {
        let params = SigningParams {
            access_key: ACCESS_KEY,
            secret_key: SECRET_KEY,
            region: REGION,
            method: "GET",
            canonical_uri: "/test.txt",
            host: "examplebucket.s3.amazonaws.com",
            query_string: "",
            extra_headers: &[("range".to_string(), "bytes=0-9".to_string())],
            payload_hash: EMPTY_HASH,
            now: test_time(),
        };
        let (auth, _headers) = sign_request(&params);
        assert!(
            auth.contains(
                "Signature=f0e8bdb87c964420e857bd35b5d6ed310bd44f0170aba48dd91039c6036bdb41"
            ),
            "auth header: {auth}"
        );
        assert!(auth.contains("SignedHeaders=host;range;x-amz-content-sha256;x-amz-date"));
    }

    #[test]
    fn sign_put_object() {
        let params = SigningParams {
            access_key: ACCESS_KEY,
            secret_key: SECRET_KEY,
            region: REGION,
            method: "PUT",
            canonical_uri: "/test%24file.text",
            host: "examplebucket.s3.amazonaws.com",
            query_string: "",
            extra_headers: &[
                (
                    "date".to_string(),
                    "Fri, 24 May 2013 00:00:00 GMT".to_string(),
                ),
                (
                    "x-amz-storage-class".to_string(),
                    "REDUCED_REDUNDANCY".to_string(),
                ),
            ],
            payload_hash: "44ce7dd67c959e0d3524ffac1771dfbba87d2b6b4b4e99e42034a8b803f8b072",
            now: test_time(),
        };
        let (auth, _headers) = sign_request(&params);
        assert!(
            auth.contains(
                "Signature=98ad721746da40c64f1a55b78f14c238d841ea1380cd77a1b5971af0ece108bd"
            ),
            "auth header: {auth}"
        );
    }

    #[test]
    fn sign_delete_request() {
        let params = SigningParams {
            access_key: ACCESS_KEY,
            secret_key: SECRET_KEY,
            region: REGION,
            method: "DELETE",
            canonical_uri: "/test.txt",
            host: "examplebucket.s3.amazonaws.com",
            query_string: "",
            extra_headers: &[],
            payload_hash: EMPTY_HASH,
            now: test_time(),
        };
        let (auth, headers) = sign_request(&params);
        assert!(auth.starts_with("AWS4-HMAC-SHA256 Credential="));
        assert!(auth.contains("SignedHeaders=host;x-amz-content-sha256;x-amz-date"));
        assert!(auth.contains("Signature="));
        // Verify headers contain required SigV4 headers
        let header_names: Vec<&str> = headers.iter().map(|(k, _)| k.as_str()).collect();
        assert!(header_names.contains(&"host"));
        assert!(header_names.contains(&"x-amz-date"));
        assert!(header_names.contains(&"x-amz-content-sha256"));
    }

    #[test]
    fn sign_get_with_query_params() {
        let params = SigningParams {
            access_key: ACCESS_KEY,
            secret_key: SECRET_KEY,
            region: REGION,
            method: "GET",
            canonical_uri: "/",
            host: "examplebucket.s3.amazonaws.com",
            query_string: "max-keys=2&prefix=J",
            extra_headers: &[],
            payload_hash: EMPTY_HASH,
            now: test_time(),
        };
        let (auth, _headers) = sign_request(&params);
        assert!(
            auth.contains(
                "Signature=34b48302e7b5fa45bde8084f4b7868a86f0a534bc59db6670ed5711ef69dc6f7"
            ),
            "auth header: {auth}"
        );
    }
}

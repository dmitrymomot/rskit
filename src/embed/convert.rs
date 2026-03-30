/// Encode an `f32` slice to a little-endian byte blob suitable for libsql
/// `F32_BLOB` columns.
pub fn to_f32_blob(v: &[f32]) -> Vec<u8> {
    let mut buf = Vec::with_capacity(v.len() * 4);
    for &f in v {
        buf.extend_from_slice(&f.to_le_bytes());
    }
    buf
}

/// Decode a little-endian byte blob back to `f32` values.
///
/// # Errors
///
/// Returns `Error::bad_request` if `blob.len()` is not a multiple of 4.
pub fn from_f32_blob(blob: &[u8]) -> crate::error::Result<Vec<f32>> {
    if !blob.len().is_multiple_of(4) {
        return Err(crate::error::Error::bad_request(
            "f32 blob length must be a multiple of 4",
        ));
    }
    Ok(blob
        .chunks_exact(4)
        .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_empty() {
        let blob = to_f32_blob(&[]);
        assert!(blob.is_empty());
        let back = from_f32_blob(&blob).unwrap();
        assert!(back.is_empty());
    }

    #[test]
    fn roundtrip_values() {
        let values = vec![1.0_f32, -0.5, 0.0, std::f32::consts::PI, f32::MAX, f32::MIN];
        let blob = to_f32_blob(&values);
        assert_eq!(blob.len(), values.len() * 4);
        let back = from_f32_blob(&blob).unwrap();
        assert_eq!(back, values);
    }

    #[test]
    fn reject_odd_length() {
        let err = from_f32_blob(&[0u8; 5]).unwrap_err();
        assert_eq!(err.status(), http::StatusCode::BAD_REQUEST);
    }

    #[test]
    fn little_endian_encoding() {
        let blob = to_f32_blob(&[1.0_f32]);
        assert_eq!(blob, 1.0_f32.to_le_bytes());
    }
}

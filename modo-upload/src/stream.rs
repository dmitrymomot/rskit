use crate::file::FieldMeta;
use bytes::Bytes;
use futures_util::stream;
use std::pin::Pin;
use tokio::io::AsyncRead;

/// An uploaded file fully buffered in memory as a sequence of chunks.
///
/// During multipart parsing all chunks are collected into memory before the
/// struct is returned.  The [`chunk()`](Self::chunk) and
/// [`into_reader()`](Self::into_reader) methods replay from this buffer.
pub struct BufferedUpload {
    name: String,
    file_name: String,
    content_type: String,
    chunks: Vec<Bytes>,
    total_size: usize,
    pos: usize,
}

impl BufferedUpload {
    /// Create from an axum multipart field by draining its chunks.
    #[doc(hidden)]
    pub async fn from_field(
        field: axum::extract::multipart::Field<'_>,
        max_size: Option<usize>,
    ) -> Result<Self, modo::Error> {
        let meta = FieldMeta::from_field(&field);

        // Collect chunks from the borrowed field into an owned Vec<Bytes>
        let mut chunks = Vec::new();
        let mut total_size: usize = 0;
        let mut field = field;
        while let Some(chunk) = field.chunk().await.map_err(|e| {
            modo::HttpError::BadRequest.with_message(format!("failed to read multipart chunk: {e}"))
        })? {
            total_size += chunk.len();
            if let Some(max) = max_size
                && total_size > max
            {
                return Err(modo::HttpError::PayloadTooLarge
                    .with_message("upload exceeds maximum allowed size"));
            }
            chunks.push(chunk);
        }

        Ok(Self {
            name: meta.name,
            file_name: meta.file_name,
            content_type: meta.content_type,
            chunks,
            total_size,
            pos: 0,
        })
    }

    /// The multipart field name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The original filename provided by the client.
    pub fn file_name(&self) -> &str {
        &self.file_name
    }

    /// The MIME content type.
    pub fn content_type(&self) -> &str {
        &self.content_type
    }

    /// Read the next chunk. Returns `None` when all chunks are consumed.
    pub async fn chunk(&mut self) -> Option<Result<Bytes, std::io::Error>> {
        if self.pos < self.chunks.len() {
            let chunk = self.chunks[self.pos].clone();
            self.pos += 1;
            Some(Ok(chunk))
        } else {
            None
        }
    }

    /// Convert into an `AsyncRead` for use with tokio I/O.
    pub fn into_reader(self) -> Pin<Box<dyn AsyncRead + Send>> {
        let chunks = self.chunks;
        let s = stream::iter(chunks.into_iter().map(Ok::<_, std::io::Error>));
        Box::pin(tokio_util::io::StreamReader::new(s))
    }

    /// Total size of all collected chunks in bytes.
    pub fn size(&self) -> usize {
        self.total_size
    }

    /// Collapse all chunks into a single contiguous `Bytes`.
    /// Single allocation sized to total content length.
    pub fn to_bytes(&self) -> bytes::Bytes {
        if self.chunks.len() == 1 {
            return self.chunks[0].clone(); // Bytes::clone is cheap (Arc)
        }
        let mut buf = bytes::BytesMut::with_capacity(self.total_size);
        for chunk in &self.chunks {
            buf.extend_from_slice(chunk);
        }
        buf.freeze()
    }

    /// Test helper — construct a `BufferedUpload` without multipart parsing.
    #[doc(hidden)]
    pub fn __test_new(name: &str, file_name: &str, content_type: &str, chunks: Vec<Bytes>) -> Self {
        let total_size = chunks.iter().map(|c| c.len()).sum();
        Self {
            name: name.to_owned(),
            file_name: file_name.to_owned(),
            content_type: content_type.to_owned(),
            chunks,
            total_size,
            pos: 0,
        }
    }
}

# Batch 2: Async Trait Migration — Implementation Plan

> **Status: COMPLETE** — All 3 issues (INC-01a, INC-01b, INC-01c) implemented and merged in PR `fix/review-issues`.

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [x]`) syntax for tracking.

**Goal:** Remove `async-trait` crate from `modo-email` and `modo-upload` by migrating to native async fn in traits (stabilized in Rust 1.75), using `trait-variant` for object-safe companion traits.
**Architecture:** Each async trait (`MailTransport`, `FileStorage`) gets `#[trait_variant::make(TraitDyn: Send)]` applied, which auto-generates an object-safe `TraitDyn` companion. All `Arc<dyn Trait>` usages switch to `Arc<dyn TraitDyn>`. The `FromMultipart` trait in `modo-upload` also uses `#[async_trait]` (including generated code from `modo-upload-macros`), so it must be migrated too before the dependency can be dropped.
**Tech Stack:** `trait-variant` crate (by the Rust lang team)

---

## Task 1: INC-01a — Migrate MailTransport to native async trait

**Files:**
- Modify: `modo-email/Cargo.toml`
- Modify: `modo-email/src/transport/trait_def.rs`
- Modify: `modo-email/src/transport/smtp.rs`
- Modify: `modo-email/src/transport/resend.rs`
- Modify: `modo-email/src/transport/factory.rs`
- Modify: `modo-email/src/mailer.rs`
- Modify: `modo-email/src/lib.rs`
- Modify: `modo-email/tests/integration.rs`

- [x] **Step 1: Add `trait-variant` dependency to `modo-email/Cargo.toml`**

  In `modo-email/Cargo.toml`, add `trait-variant` to `[dependencies]`:

  ```toml
  [dependencies]
  modo.workspace = true
  serde = { version = "1", features = ["derive"] }
  serde_json = "1"
  serde_yaml_ng = "0.10"
  async-trait = "0.1"
  trait-variant = "0.1"
  pulldown-cmark = "0.12"
  minijinja = { version = "2", features = ["loader"] }
  lettre = { version = "0.11", features = ["tokio1-native-tls", "builder", "smtp-transport"], optional = true }
  reqwest = { version = "0.12", features = ["json"], optional = true }
  ```

  Note: `async-trait` stays for now — it is removed in Task 3.

- [x] **Step 2: Migrate `MailTransport` trait definition**

  Replace the entire content of `modo-email/src/transport/trait_def.rs`:

  **Before:**
  ```rust
  use crate::message::MailMessage;

  /// Async trait that every delivery backend must implement.
  ///
  /// Implement this trait to add a custom transport (e.g. a test spy,
  /// an in-memory queue, or a third-party HTTP API).
  #[async_trait::async_trait]
  pub trait MailTransport: Send + Sync + 'static {
      /// Deliver `message` to its recipients.
      async fn send(&self, message: &MailMessage) -> Result<(), modo::Error>;
  }
  ```

  **After:**
  ```rust
  use crate::message::MailMessage;

  /// Async trait that every delivery backend must implement.
  ///
  /// Implement this trait to add a custom transport (e.g. a test spy,
  /// an in-memory queue, or a third-party HTTP API).
  ///
  /// Use [`MailTransportDyn`] (auto-generated object-safe companion) for
  /// trait objects: `Arc<dyn MailTransportDyn>`.
  #[trait_variant::make(MailTransportDyn: Send)]
  pub trait MailTransport: Sync + 'static {
      /// Deliver `message` to its recipients.
      async fn send(&self, message: &MailMessage) -> Result<(), modo::Error>;
  }
  ```

  Key changes:
  - Removed `#[async_trait::async_trait]`
  - Added `#[trait_variant::make(MailTransportDyn: Send)]`
  - Removed `Send` from the base trait bounds (the `MailTransportDyn` companion adds `Send` automatically; keeping `Send` on the base would be redundant since `trait_variant` adds it to the Dyn variant)

- [x] **Step 3: Update `mod.rs` to re-export `MailTransportDyn`**

  In `modo-email/src/transport/mod.rs`, add re-export of `MailTransportDyn`:

  **Before:**
  ```rust
  pub use trait_def::MailTransport;
  ```

  **After:**
  ```rust
  pub use trait_def::{MailTransport, MailTransportDyn};
  ```

- [x] **Step 4: Update `lib.rs` to re-export `MailTransportDyn`**

  In `modo-email/src/lib.rs`, update the transport re-export:

  **Before:**
  ```rust
  pub use transport::MailTransport;
  ```

  **After:**
  ```rust
  pub use transport::{MailTransport, MailTransportDyn};
  ```

- [x] **Step 5: Remove `#[async_trait]` from SMTP impl**

  In `modo-email/src/transport/smtp.rs`, remove the attribute:

  **Before:**
  ```rust
  #[async_trait::async_trait]
  impl MailTransport for SmtpTransport {
  ```

  **After:**
  ```rust
  impl MailTransport for SmtpTransport {
  ```

  The `async fn send(...)` signature remains unchanged.

- [x] **Step 6: Remove `#[async_trait]` from Resend impl**

  In `modo-email/src/transport/resend.rs`, remove the attribute:

  **Before:**
  ```rust
  #[async_trait::async_trait]
  impl MailTransport for ResendTransport {
  ```

  **After:**
  ```rust
  impl MailTransport for ResendTransport {
  ```

- [x] **Step 7: Update `factory.rs` — use `MailTransportDyn` in return type**

  In `modo-email/src/transport/factory.rs`:

  **Before:**
  ```rust
  use crate::config::EmailConfig;
  use crate::transport::MailTransport;
  use std::sync::Arc;

  /// Create the appropriate transport backend based on config.
  pub fn transport(config: &EmailConfig) -> Result<Arc<dyn MailTransport>, modo::Error> {
  ```

  **After:**
  ```rust
  use crate::config::EmailConfig;
  use crate::transport::MailTransportDyn;
  use std::sync::Arc;

  /// Create the appropriate transport backend based on config.
  pub fn transport(config: &EmailConfig) -> Result<Arc<dyn MailTransportDyn>, modo::Error> {
  ```

  The body is unchanged — concrete types that `impl MailTransport` automatically impl `MailTransportDyn` too.

- [x] **Step 8: Update `mailer.rs` — use `MailTransportDyn` in struct and constructor**

  In `modo-email/src/mailer.rs`, update the import, struct field, and constructor:

  **Before (import):**
  ```rust
  use crate::transport::MailTransport;
  ```

  **After (import):**
  ```rust
  use crate::transport::MailTransportDyn;
  ```

  **Before (struct):**
  ```rust
  pub struct Mailer {
      transport: Arc<dyn MailTransport>,
      templates: Arc<dyn TemplateProvider>,
      default_sender: SenderProfile,
      layout_engine: Arc<LayoutEngine>,
  }
  ```

  **After (struct):**
  ```rust
  pub struct Mailer {
      transport: Arc<dyn MailTransportDyn>,
      templates: Arc<dyn TemplateProvider>,
      default_sender: SenderProfile,
      layout_engine: Arc<LayoutEngine>,
  }
  ```

  **Before (constructor):**
  ```rust
      pub fn new(
          transport: Arc<dyn MailTransport>,
          templates: Arc<dyn TemplateProvider>,
          default_sender: SenderProfile,
          layout_engine: Arc<LayoutEngine>,
      ) -> Self {
  ```

  **After (constructor):**
  ```rust
      pub fn new(
          transport: Arc<dyn MailTransportDyn>,
          templates: Arc<dyn TemplateProvider>,
          default_sender: SenderProfile,
          layout_engine: Arc<LayoutEngine>,
      ) -> Self {
  ```

  **Before (test mock — MockTransport):**
  ```rust
      #[async_trait::async_trait]
      impl MailTransport for MockTransport {
  ```

  **After (test mock):**
  ```rust
      impl MailTransport for MockTransport {
  ```

  **Before (test helper):**
  ```rust
      fn test_mailer(transport: Arc<dyn MailTransport>) -> Mailer {
  ```

  **After (test helper):**
  ```rust
      fn test_mailer(transport: Arc<dyn MailTransportDyn>) -> Mailer {
  ```

  Also update the import at the top of the test module. The test module uses `use super::*;` which will pick up the new `MailTransportDyn` import.

- [x] **Step 9: Update `integration.rs` test file**

  In `modo-email/tests/integration.rs`:

  **Before (imports):**
  ```rust
  use modo_email::{MailMessage, MailTransport, Mailer, SendEmail, SenderProfile};
  ```

  **After (imports):**
  ```rust
  use modo_email::{MailMessage, MailTransport, MailTransportDyn, Mailer, SendEmail, SenderProfile};
  ```

  **Before (CapturingTransport impl):**
  ```rust
  #[async_trait::async_trait]
  impl MailTransport for CapturingTransport {
  ```

  **After:**
  ```rust
  impl MailTransport for CapturingTransport {
  ```

  **Before (FailingTransport impl):**
  ```rust
  #[async_trait::async_trait]
  impl MailTransport for FailingTransport {
  ```

  **After:**
  ```rust
  impl MailTransport for FailingTransport {
  ```

  **Before (transport_error_propagates test):**
  ```rust
      let transport: Arc<dyn MailTransport> = Arc::new(FailingTransport);
  ```

  **After:**
  ```rust
      let transport: Arc<dyn MailTransportDyn> = Arc::new(FailingTransport);
  ```

- [x] **Step 10: Verify compilation and tests**

  Run:
  ```bash
  cargo test -p modo-email
  ```
  Expected: All tests pass, no compilation errors.

  Also run with all features:
  ```bash
  cargo test -p modo-email --all-features
  ```
  Expected: All tests pass.

- [x] **Step 11: Commit**

  ```
  refactor(email): migrate MailTransport to native async trait

  Replace #[async_trait] with native async fn in traits (Rust 1.75+).
  Use trait_variant::make to generate MailTransportDyn as the
  object-safe companion trait for use with Arc<dyn MailTransportDyn>.
  ```

---

## Task 2: INC-01b — Migrate FileStorage to native async trait

**Files:**
- Modify: `modo-upload/Cargo.toml`
- Modify: `modo-upload/src/storage/types.rs`
- Modify: `modo-upload/src/storage/local.rs`
- Modify: `modo-upload/src/storage/opendal.rs`
- Modify: `modo-upload/src/storage/factory.rs`
- Modify: `modo-upload/src/storage/mod.rs`
- Modify: `modo-upload/src/lib.rs`
- Modify: `examples/upload/src/handlers.rs`

- [x] **Step 1: Add `trait-variant` dependency to `modo-upload/Cargo.toml`**

  In `modo-upload/Cargo.toml`, add `trait-variant` to `[dependencies]`:

  ```toml
  [dependencies]
  modo.workspace = true
  modo-upload-macros.workspace = true
  serde = { version = "1", features = ["derive"] }
  axum = { version = "0.8", features = ["multipart"] }
  async-trait = "0.1"
  trait-variant = "0.1"
  bytes = "1"
  futures-util = "0.3"
  tokio = { version = "1", features = ["fs", "io-util"] }
  tokio-util = { version = "0.7", features = ["io"] }
  ulid = "1"
  opendal = { version = "0.53", optional = true, features = ["services-s3"] }
  ```

  Note: `async-trait` stays for now — `FromMultipart` still needs it. Removed in Task 3.

- [x] **Step 2: Migrate `FileStorage` trait definition**

  Replace the trait definition in `modo-upload/src/storage/types.rs`:

  **Before:**
  ```rust
  use crate::file::UploadedFile;
  use crate::stream::BufferedUpload;

  /// Metadata returned after a file has been successfully stored.
  pub struct StoredFile {
      /// Relative path within the storage backend (e.g. `"avatars/01HXK3Q1A2B3.jpg"`).
      pub path: String,
      /// File size in bytes.
      pub size: u64,
  }

  /// Trait for persisting uploaded files to a storage backend.
  ///
  /// Both in-memory ([`UploadedFile`]) and chunked ([`BufferedUpload`]) uploads
  /// are supported.  Implementors must be `Send + Sync + 'static` so they can be
  /// shared across async tasks behind an `Arc<dyn FileStorage>`.
  ///
  /// Use the [`storage()`](crate::storage()) factory function to construct the
  /// backend configured by [`UploadConfig`](crate::UploadConfig), or instantiate
  /// a concrete backend directly (e.g. [`LocalStorage`](super::local::LocalStorage)).
  #[async_trait::async_trait]
  pub trait FileStorage: Send + Sync + 'static {
      /// Store a buffered in-memory file under `prefix/`.
      ///
      /// A ULID-based unique filename is generated automatically.
      /// Returns the stored path and size on success.
      async fn store(&self, prefix: &str, file: &UploadedFile) -> Result<StoredFile, modo::Error>;

      /// Store a chunked upload under `prefix/`.
      ///
      /// Chunks are consumed from `stream` sequentially.
      /// Returns the stored path and size on success.
      async fn store_stream(
          &self,
          prefix: &str,
          stream: &mut BufferedUpload,
      ) -> Result<StoredFile, modo::Error>;

      /// Delete a file by its storage path (as returned by [`store`](Self::store)).
      async fn delete(&self, path: &str) -> Result<(), modo::Error>;

      /// Return `true` if a file exists at the given storage path.
      async fn exists(&self, path: &str) -> Result<bool, modo::Error>;
  }
  ```

  **After:**
  ```rust
  use crate::file::UploadedFile;
  use crate::stream::BufferedUpload;

  /// Metadata returned after a file has been successfully stored.
  pub struct StoredFile {
      /// Relative path within the storage backend (e.g. `"avatars/01HXK3Q1A2B3.jpg"`).
      pub path: String,
      /// File size in bytes.
      pub size: u64,
  }

  /// Trait for persisting uploaded files to a storage backend.
  ///
  /// Both in-memory ([`UploadedFile`]) and chunked ([`BufferedUpload`]) uploads
  /// are supported.  Implementors must be `Sync + 'static` so they can be
  /// shared across async tasks behind an `Arc<dyn FileStorageDyn>`.
  ///
  /// Use the [`storage()`](crate::storage()) factory function to construct the
  /// backend configured by [`UploadConfig`](crate::UploadConfig), or instantiate
  /// a concrete backend directly (e.g. [`LocalStorage`](super::local::LocalStorage)).
  ///
  /// Use [`FileStorageDyn`] (auto-generated object-safe companion) for
  /// trait objects: `Arc<dyn FileStorageDyn>`.
  #[trait_variant::make(FileStorageDyn: Send)]
  pub trait FileStorage: Sync + 'static {
      /// Store a buffered in-memory file under `prefix/`.
      ///
      /// A ULID-based unique filename is generated automatically.
      /// Returns the stored path and size on success.
      async fn store(&self, prefix: &str, file: &UploadedFile) -> Result<StoredFile, modo::Error>;

      /// Store a chunked upload under `prefix/`.
      ///
      /// Chunks are consumed from `stream` sequentially.
      /// Returns the stored path and size on success.
      async fn store_stream(
          &self,
          prefix: &str,
          stream: &mut BufferedUpload,
      ) -> Result<StoredFile, modo::Error>;

      /// Delete a file by its storage path (as returned by [`store`](Self::store)).
      async fn delete(&self, path: &str) -> Result<(), modo::Error>;

      /// Return `true` if a file exists at the given storage path.
      async fn exists(&self, path: &str) -> Result<bool, modo::Error>;
  }
  ```

  Key changes:
  - Removed `#[async_trait::async_trait]`
  - Added `#[trait_variant::make(FileStorageDyn: Send)]`
  - Removed `Send` from the base trait bounds

- [x] **Step 3: Update `storage/mod.rs` to re-export `FileStorageDyn`**

  In `modo-upload/src/storage/mod.rs`:

  **Before:**
  ```rust
  pub use types::{FileStorage, StoredFile};
  ```

  **After:**
  ```rust
  pub use types::{FileStorage, FileStorageDyn, StoredFile};
  ```

- [x] **Step 4: Update `lib.rs` to re-export `FileStorageDyn`**

  In `modo-upload/src/lib.rs`:

  **Before:**
  ```rust
  pub use storage::{FileStorage, StoredFile, storage};
  ```

  **After:**
  ```rust
  pub use storage::{FileStorage, FileStorageDyn, StoredFile, storage};
  ```

  Also update the doc comment example at the top of `lib.rs` (line 41):

  **Before:**
  ```rust
  //!     file_storage: Service<Arc<dyn FileStorage>>,
  ```

  **After:**
  ```rust
  //!     file_storage: Service<Arc<dyn FileStorageDyn>>,
  ```

- [x] **Step 5: Remove `#[async_trait]` from LocalStorage impl**

  In `modo-upload/src/storage/local.rs`:

  **Before:**
  ```rust
  #[async_trait::async_trait]
  impl FileStorage for LocalStorage {
  ```

  **After:**
  ```rust
  impl FileStorage for LocalStorage {
  ```

- [x] **Step 6: Remove `#[async_trait]` from OpendalStorage impl**

  In `modo-upload/src/storage/opendal.rs`:

  **Before:**
  ```rust
  #[async_trait::async_trait]
  impl FileStorage for OpendalStorage {
  ```

  **After:**
  ```rust
  impl FileStorage for OpendalStorage {
  ```

- [x] **Step 7: Update `storage/factory.rs` — use `FileStorageDyn` in return type**

  In `modo-upload/src/storage/factory.rs`:

  **Before:**
  ```rust
  use crate::storage::FileStorage;
  use std::sync::Arc;

  /// Construct a [`FileStorage`] backend from [`UploadConfig`](crate::UploadConfig).
  /// ...
  pub fn storage(config: &crate::config::UploadConfig) -> Result<Arc<dyn FileStorage>, modo::Error> {
  ```

  **After:**
  ```rust
  use crate::storage::FileStorageDyn;
  use std::sync::Arc;

  /// Construct a [`FileStorage`] backend from [`UploadConfig`](crate::UploadConfig).
  /// ...
  pub fn storage(config: &crate::config::UploadConfig) -> Result<Arc<dyn FileStorageDyn>, modo::Error> {
  ```

- [x] **Step 8: Update `examples/upload/src/handlers.rs`**

  In `examples/upload/src/handlers.rs`:

  **Before:**
  ```rust
  use modo_upload::{FileStorage, MultipartForm};

  #[modo::handler(POST, "/profile")]
  async fn update_profile(
      storage: Service<Arc<dyn FileStorage>>,
      form: MultipartForm<ProfileForm>,
  ) -> JsonResult<serde_json::Value> {
  ```

  **After:**
  ```rust
  use modo_upload::{FileStorageDyn, MultipartForm};

  #[modo::handler(POST, "/profile")]
  async fn update_profile(
      storage: Service<Arc<dyn FileStorageDyn>>,
      form: MultipartForm<ProfileForm>,
  ) -> JsonResult<serde_json::Value> {
  ```

- [x] **Step 9: Verify compilation and tests**

  Run:
  ```bash
  cargo test -p modo-upload
  ```
  Expected: All tests pass.

  Also run with all features:
  ```bash
  cargo test -p modo-upload --all-features
  ```
  Expected: All tests pass.

  Check the upload example compiles:
  ```bash
  cargo check -p upload
  ```
  Expected: No errors.

- [x] **Step 10: Commit**

  ```
  refactor(upload): migrate FileStorage to native async trait

  Replace #[async_trait] with native async fn in traits (Rust 1.75+).
  Use trait_variant::make to generate FileStorageDyn as the
  object-safe companion trait for use with Arc<dyn FileStorageDyn>.
  ```

---

## Task 3: INC-01c — Drop async-trait dependency

**Important discovery:** `modo-upload` uses `async-trait` in TWO places:
1. `FileStorage` trait (migrated in Task 2)
2. `FromMultipart` trait in `modo-upload/src/from_multipart.rs` AND generated code from `modo-upload-macros/src/from_multipart.rs` (line 436: `#[modo_upload::__internal::async_trait]`)

The `FromMultipart` trait is NOT object-safe (returns `Self`), so it does not need `trait_variant`. It just needs `#[async_trait]` removed and native async fn used. However, the proc macro in `modo-upload-macros` generates `#[modo_upload::__internal::async_trait]` on the `impl` blocks. Both must be updated together.

**Files:**
- Modify: `modo-email/Cargo.toml` (remove `async-trait`)
- Modify: `modo-upload/Cargo.toml` (remove `async-trait`)
- Modify: `modo-upload/src/from_multipart.rs` (remove `#[async_trait]`)
- Modify: `modo-upload/src/lib.rs` (remove `async_trait` re-export from `__internal`)
- Modify: `modo-upload-macros/src/from_multipart.rs` (remove generated `#[async_trait]`)

- [x] **Step 1: Migrate `FromMultipart` trait definition**

  In `modo-upload/src/from_multipart.rs`:

  **Before:**
  ```rust
  /// Trait for parsing a struct from `multipart/form-data`.
  ///
  /// Implement this trait (or derive it with `#[derive(FromMultipart)]`) to
  /// describe how multipart fields map to struct fields.  The
  /// [`MultipartForm`](crate::MultipartForm) extractor calls this automatically during request
  /// extraction.
  #[async_trait::async_trait]
  pub trait FromMultipart: Sized {
      /// Parse `multipart` into `Self`, enforcing `max_file_size` on every file
      /// field when `Some`.
      async fn from_multipart(
          multipart: &mut axum::extract::Multipart,
          max_file_size: Option<usize>,
      ) -> Result<Self, modo::Error>;
  }
  ```

  **After:**
  ```rust
  /// Trait for parsing a struct from `multipart/form-data`.
  ///
  /// Implement this trait (or derive it with `#[derive(FromMultipart)]`) to
  /// describe how multipart fields map to struct fields.  The
  /// [`MultipartForm`](crate::MultipartForm) extractor calls this automatically during request
  /// extraction.
  pub trait FromMultipart: Sized {
      /// Parse `multipart` into `Self`, enforcing `max_file_size` on every file
      /// field when `Some`.
      fn from_multipart(
          multipart: &mut axum::extract::Multipart,
          max_file_size: Option<usize>,
      ) -> impl std::future::Future<Output = Result<Self, modo::Error>> + Send;
  }
  ```

  Note: We use `-> impl Future + Send` instead of `async fn` because `async fn` in traits without `#[async_trait]` returns an opaque `impl Future` that is NOT `Send` by default. The `FromMultipart` trait is used in an axum extractor which requires `Send` futures. Using `-> impl Future<...> + Send` explicitly guarantees `Send`-ness. This is the correct approach for traits where you need `Send` but do NOT need trait objects (no `dyn FromMultipart`).

- [x] **Step 2: Remove generated `#[async_trait]` from proc macro and wrap body in `async move`**

  In `modo-upload-macros/src/from_multipart.rs`, replace the entire `quote!` block (lines 435-460):

  **Before:**
  ```rust
      Ok(quote! {
          #[modo_upload::__internal::async_trait]
          impl #impl_generics modo_upload::FromMultipart for #struct_name #ty_generics #where_clause {
              async fn from_multipart(
                  multipart: &mut modo_upload::__internal::axum::extract::Multipart,
                  __max_file_size: Option<usize>,
              ) -> Result<Self, modo::Error> {
                  #(#var_decls)*

                  while let Some(__field) = multipart.next_field().await
                      .map_err(|e| modo::HttpError::BadRequest.with_message(format!("{e}")))?
                  {
                      match __field.name() {
                          #(#match_arms)*
                          _ => {}
                      }
                  }

                  #(#validation_stmts)*

                  Ok(Self {
                      #(#field_assignments),*
                  })
              }
          }
      })
  ```

  **After:**
  ```rust
      Ok(quote! {
          impl #impl_generics modo_upload::FromMultipart for #struct_name #ty_generics #where_clause {
              fn from_multipart(
                  multipart: &mut modo_upload::__internal::axum::extract::Multipart,
                  __max_file_size: Option<usize>,
              ) -> impl std::future::Future<Output = Result<Self, modo::Error>> + Send {
                  async move {
                      #(#var_decls)*

                      while let Some(__field) = multipart.next_field().await
                          .map_err(|e| modo::HttpError::BadRequest.with_message(format!("{e}")))?
                      {
                          match __field.name() {
                              #(#match_arms)*
                              _ => {}
                          }
                      }

                      #(#validation_stmts)*

                      Ok(Self {
                          #(#field_assignments),*
                      })
                  }
              }
          }
      })
  ```

  Key changes:
  - Removed `#[modo_upload::__internal::async_trait]` attribute
  - Changed `async fn from_multipart(...)  -> Result<Self, modo::Error>` to `fn from_multipart(...) -> impl std::future::Future<Output = Result<Self, modo::Error>> + Send`
  - Wrapped the entire function body in `async move { ... }`

- [x] **Step 4: Remove `async_trait` from `modo-upload/src/lib.rs` internal re-exports**

  In `modo-upload/src/lib.rs`:

  **Before:**
  ```rust
  /// Internal helpers exposed for use by generated code. Not public API.
  #[doc(hidden)]
  pub mod __internal {
      pub use crate::validate::mime_matches;
      pub use async_trait::async_trait;
      pub use axum;
  }
  ```

  **After:**
  ```rust
  /// Internal helpers exposed for use by generated code. Not public API.
  #[doc(hidden)]
  pub mod __internal {
      pub use crate::validate::mime_matches;
      pub use axum;
  }
  ```

- [x] **Step 5: Remove `async-trait` from `modo-email/Cargo.toml`**

  In `modo-email/Cargo.toml`, remove the line:
  ```toml
  async-trait = "0.1"
  ```

- [x] **Step 6: Remove `async-trait` from `modo-upload/Cargo.toml`**

  In `modo-upload/Cargo.toml`, remove the line:
  ```toml
  async-trait = "0.1"
  ```

- [x] **Step 7: Grep to verify complete removal**

  Run the following to confirm no `async-trait` / `async_trait` references remain (other than docs/plans):

  ```bash
  # Check Cargo.toml files
  grep -r "async.trait" --include="Cargo.toml" .

  # Check Rust source files
  grep -rn "async_trait" --include="*.rs" .
  ```

  Expected: No matches in any `Cargo.toml`. No matches in any `.rs` file (excluding `target/`, docs, and this plan file).

- [x] **Step 8: Run all tests**

  ```bash
  cargo test -p modo-email
  cargo test -p modo-email --all-features
  cargo test -p modo-upload
  cargo test -p modo-upload --all-features
  cargo check -p upload
  ```
  Expected: All pass.

- [x] **Step 9: Run full workspace check**

  ```bash
  just check
  ```
  Expected: fmt, lint, and all tests pass.

- [x] **Step 10: Commit**

  ```
  refactor: drop async-trait dependency from modo-email and modo-upload

  Migrate FromMultipart trait to native async fn in traits and remove
  the async-trait crate from both modo-email and modo-upload. All async
  traits now use native Rust syntax (stabilized in 1.75).
  ```

---

## Appendix: Files to update in documentation (non-blocking)

The following documentation files reference `Arc<dyn FileStorage>` and should be updated to `Arc<dyn FileStorageDyn>` in a follow-up. They are NOT blocking for compilation or tests:

- `README.md` (line 188)
- `modo-upload/README.md` (lines 70, 85, 167)
- `modo-upload-macros/README.md` (line 101)
- `claude-plugin/skills/modo/references/upload.md` (lines 85, 266, 279, 363, 386, 458, 511, 512)

These can be updated in a separate documentation commit after the refactoring is verified.

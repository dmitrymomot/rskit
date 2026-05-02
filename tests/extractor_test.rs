use axum::Router;
use axum::body::Body;
use axum::http::Request;
use axum::routing::get;
use http::StatusCode;
use modo::client::ClientInfo;
use modo::sanitize::Sanitize;
use modo::service::Registry;
use serde::Deserialize;
use tower::ServiceExt;

#[tokio::test]
async fn test_service_extractor_success() {
    #[derive(Debug)]
    struct Greeter(String);

    async fn handler(modo::service::Service(greeter): modo::service::Service<Greeter>) -> String {
        greeter.0.clone()
    }

    let mut registry = Registry::new();
    registry.add(Greeter("hello".to_string()));
    let app = Router::new()
        .route("/", get(handler))
        .with_state(registry.into_state());

    let response = app
        .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    assert_eq!(&body[..], b"hello");
}

#[tokio::test]
async fn test_service_extractor_missing_returns_500() {
    #[derive(Debug)]
    struct Missing;

    async fn handler(_: modo::service::Service<Missing>) -> String {
        "unreachable".to_string()
    }

    let registry = Registry::new();
    let app = Router::new()
        .route("/", get(handler))
        .with_state(registry.into_state());

    let response = app
        .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
}

#[derive(Deserialize)]
struct CreateItem {
    title: String,
}

impl Sanitize for CreateItem {
    fn sanitize(&mut self) {
        modo::sanitize::trim(&mut self.title);
    }
}

#[tokio::test]
async fn test_json_request_deserializes_and_sanitizes() {
    async fn handler(
        modo::extractor::JsonRequest(item): modo::extractor::JsonRequest<CreateItem>,
    ) -> String {
        item.title
    }

    let registry = Registry::new();
    let app = Router::new()
        .route("/", axum::routing::post(handler))
        .with_state(registry.into_state());

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"title":"  hello  "}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    assert_eq!(&body[..], b"hello");
}

#[tokio::test]
async fn test_form_request_deserializes_and_sanitizes() {
    async fn handler(
        modo::extractor::FormRequest(item): modo::extractor::FormRequest<CreateItem>,
    ) -> String {
        item.title
    }

    let registry = Registry::new();
    let app = Router::new()
        .route("/", axum::routing::post(handler))
        .with_state(registry.into_state());

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/")
                .header("content-type", "application/x-www-form-urlencoded")
                .body(Body::from("title=%20+hello+%20"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    assert_eq!(&body[..], b"hello");
}

#[derive(Deserialize)]
struct MultiSelectForm {
    name: String,
    work_days: Vec<u8>,
    policy_ids: Vec<String>,
}

impl Sanitize for MultiSelectForm {
    fn sanitize(&mut self) {
        modo::sanitize::trim(&mut self.name);
    }
}

#[tokio::test]
async fn test_form_request_repeated_keys_into_vec() {
    async fn handler(
        modo::extractor::FormRequest(form): modo::extractor::FormRequest<MultiSelectForm>,
    ) -> String {
        let days = form
            .work_days
            .iter()
            .map(u8::to_string)
            .collect::<Vec<_>>()
            .join(",");
        format!("{}|{}|{}", form.name, days, form.policy_ids.join(","))
    }

    let registry = Registry::new();
    let app = Router::new()
        .route("/", axum::routing::post(handler))
        .with_state(registry.into_state());

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/")
                .header("content-type", "application/x-www-form-urlencoded")
                .body(Body::from(
                    "name=Alice&work_days=1&work_days=2&work_days=3&work_days=4&work_days=5\
                     &policy_ids=pto&policy_ids=sick",
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    assert_eq!(&body[..], b"Alice|1,2,3,4,5|pto,sick");
}

#[derive(Deserialize)]
struct ParallelArrayForm {
    contact_type: Vec<String>,
    contact_value: Vec<String>,
}

impl Sanitize for ParallelArrayForm {
    fn sanitize(&mut self) {}
}

#[tokio::test]
async fn test_form_request_parallel_arrays() {
    async fn handler(
        modo::extractor::FormRequest(form): modo::extractor::FormRequest<ParallelArrayForm>,
    ) -> String {
        form.contact_type
            .into_iter()
            .zip(form.contact_value)
            .map(|(kind, value)| format!("{kind}={value}"))
            .collect::<Vec<_>>()
            .join(";")
    }

    let registry = Registry::new();
    let app = Router::new()
        .route("/", axum::routing::post(handler))
        .with_state(registry.into_state());

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/")
                .header("content-type", "application/x-www-form-urlencoded")
                .body(Body::from(
                    "contact_type=email&contact_value=a%40b.com\
                     &contact_type=phone&contact_value=555-0100",
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    assert_eq!(&body[..], b"email=a@b.com;phone=555-0100");
}

#[tokio::test]
async fn test_query_extractor_repeated_keys_into_vec() {
    #[derive(Deserialize)]
    struct TaggedSearch {
        q: String,
        tags: Vec<String>,
    }
    impl Sanitize for TaggedSearch {
        fn sanitize(&mut self) {
            modo::sanitize::trim(&mut self.q);
        }
    }

    async fn handler(modo::extractor::Query(p): modo::extractor::Query<TaggedSearch>) -> String {
        format!("{}|{}", p.q, p.tags.join(","))
    }

    let registry = Registry::new();
    let app = Router::new()
        .route("/", get(handler))
        .with_state(registry.into_state());

    let response = app
        .oneshot(
            Request::builder()
                .uri("/?q=rust&tags=web&tags=axum&tags=sqlite")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    assert_eq!(&body[..], b"rust|web,axum,sqlite");
}

#[tokio::test]
async fn test_multipart_request_repeated_text_fields() {
    #[derive(Deserialize)]
    struct ChecklistForm {
        title: String,
        items: Vec<String>,
    }
    impl Sanitize for ChecklistForm {
        fn sanitize(&mut self) {
            modo::sanitize::trim(&mut self.title);
        }
    }

    async fn handler(
        modo::extractor::MultipartRequest(form, _): modo::extractor::MultipartRequest<
            ChecklistForm,
        >,
    ) -> String {
        format!("{}|{}", form.title, form.items.join(","))
    }

    let registry = Registry::new();
    let app = Router::new()
        .route("/", axum::routing::post(handler))
        .with_state(registry.into_state());

    let boundary = "----TestRepeatBoundary";
    let body = format!(
        "--{boundary}\r\nContent-Disposition: form-data; name=\"title\"\r\n\r\nGroceries\r\n\
         --{boundary}\r\nContent-Disposition: form-data; name=\"items\"\r\n\r\nmilk\r\n\
         --{boundary}\r\nContent-Disposition: form-data; name=\"items\"\r\n\r\neggs\r\n\
         --{boundary}\r\nContent-Disposition: form-data; name=\"items\"\r\n\r\nbread\r\n\
         --{boundary}--\r\n"
    );

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/")
                .header(
                    "content-type",
                    format!("multipart/form-data; boundary={boundary}"),
                )
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    assert_eq!(&body[..], b"Groceries|milk,eggs,bread");
}

#[tokio::test]
async fn test_query_extractor_sanitizes() {
    async fn handler(modo::extractor::Query(item): modo::extractor::Query<CreateItem>) -> String {
        item.title
    }

    let registry = Registry::new();
    let app = Router::new()
        .route("/", get(handler))
        .with_state(registry.into_state());

    let response = app
        .oneshot(
            Request::builder()
                .uri("/?title=%20+hello+%20")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    assert_eq!(&body[..], b"hello");
}

#[tokio::test]
async fn test_multipart_request_text_fields() {
    #[derive(Deserialize)]
    struct ProfileData {
        name: String,
    }
    impl Sanitize for ProfileData {
        fn sanitize(&mut self) {
            modo::sanitize::trim(&mut self.name);
        }
    }

    async fn handler(
        modo::extractor::MultipartRequest(data, _files): modo::extractor::MultipartRequest<
            ProfileData,
        >,
    ) -> String {
        data.name
    }

    let registry = Registry::new();
    let app = Router::new()
        .route("/", axum::routing::post(handler))
        .with_state(registry.into_state());

    let boundary = "----TestBoundary";
    let body = format!(
        "--{boundary}\r\nContent-Disposition: form-data; name=\"name\"\r\n\r\n  Alice  \r\n--{boundary}--\r\n"
    );

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/")
                .header(
                    "content-type",
                    format!("multipart/form-data; boundary={boundary}"),
                )
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    assert_eq!(&body[..], b"Alice");
}

#[tokio::test]
async fn test_client_info_extraction() {
    async fn handler(info: ClientInfo) -> String {
        let ua = info.user_agent_value().unwrap_or("none");
        let device = info.device_name_value().unwrap_or("none");
        // Fingerprint is server-computed from UA + Accept-Language + Accept-Encoding.
        let fp_len = info.fingerprint_value().map(str::len).unwrap_or(0);
        format!("{ua}|{device}|{fp_len}")
    }

    let app = Router::new().route("/", get(handler));

    let response = app
        .oneshot(
            Request::builder()
                .uri("/")
                .header("user-agent", "TestBot/2.0")
                .header("accept-language", "en-US")
                .header("accept-encoding", "gzip")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let text = String::from_utf8(body.to_vec()).unwrap();
    assert_eq!(text, "TestBot/2.0|Unknown on Unknown|64");
}

#[test]
fn test_uploaded_file_struct() {
    let file = modo::extractor::UploadedFile {
        name: "photo.jpg".to_string(),
        content_type: "image/jpeg".to_string(),
        size: 1024,
        data: bytes::Bytes::from_static(b"fake image data"),
    };
    assert_eq!(file.name, "photo.jpg");
    assert_eq!(file.size, 1024);
}

#[test]
fn test_files_get_and_file() {
    use std::collections::HashMap;

    let file = modo::extractor::UploadedFile {
        name: "doc.pdf".to_string(),
        content_type: "application/pdf".to_string(),
        size: 512,
        data: bytes::Bytes::from_static(b"pdf data"),
    };

    let mut map = HashMap::new();
    map.insert("document".to_string(), vec![file]);
    let mut files = modo::extractor::Files::from_map(map);

    assert!(files.get("document").is_some());
    assert!(files.get("missing").is_none());

    let taken = files.file("document").unwrap();
    assert_eq!(taken.name, "doc.pdf");
    assert!(files.get("document").is_none()); // removed after file()
}

#[tokio::test]
async fn test_json_request_rejects_invalid_json() {
    async fn handler(
        modo::extractor::JsonRequest(_item): modo::extractor::JsonRequest<CreateItem>,
    ) -> String {
        "unreachable".to_string()
    }

    let registry = Registry::new();
    let app = Router::new()
        .route("/", axum::routing::post(handler))
        .with_state(registry.into_state());

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/")
                .header("content-type", "application/json")
                .body(Body::from("not json"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[derive(Deserialize)]
struct ClientForm {
    client: ClientInner,
}

#[derive(Deserialize)]
struct ClientInner {
    name: String,
    id: u32,
}

impl Sanitize for ClientForm {
    fn sanitize(&mut self) {
        modo::sanitize::trim(&mut self.client.name);
    }
}

#[tokio::test]
async fn test_form_request_nested_struct() {
    async fn handler(
        modo::extractor::FormRequest(form): modo::extractor::FormRequest<ClientForm>,
    ) -> String {
        format!("{}|{}", form.client.name, form.client.id)
    }

    let registry = Registry::new();
    let app = Router::new()
        .route("/", axum::routing::post(handler))
        .with_state(registry.into_state());

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/")
                .header("content-type", "application/x-www-form-urlencoded")
                .body(Body::from("client[name]=Acme&client[id]=42"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    assert_eq!(&body[..], b"Acme|42");
}

#[tokio::test]
async fn test_multipart_request_with_file_upload() {
    #[derive(Deserialize)]
    struct UploadForm {
        name: String,
    }
    impl Sanitize for UploadForm {
        fn sanitize(&mut self) {
            modo::sanitize::trim(&mut self.name);
        }
    }

    async fn handler(
        modo::extractor::MultipartRequest(data, mut files): modo::extractor::MultipartRequest<
            UploadForm,
        >,
    ) -> String {
        let file = files.file("avatar").unwrap();
        format!(
            "{}|{}|{}|{}",
            data.name, file.name, file.content_type, file.size
        )
    }

    let registry = Registry::new();
    let app = Router::new()
        .route("/", axum::routing::post(handler))
        .with_state(registry.into_state());

    let boundary = "----TestFileBoundary";
    let file_data = b"fake image bytes";
    let body = format!(
        "--{boundary}\r\nContent-Disposition: form-data; name=\"name\"\r\n\r\nAlice\r\n--{boundary}\r\nContent-Disposition: form-data; name=\"avatar\"; filename=\"photo.jpg\"\r\nContent-Type: image/jpeg\r\n\r\n{}\r\n--{boundary}--\r\n",
        String::from_utf8_lossy(file_data)
    );

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/")
                .header(
                    "content-type",
                    format!("multipart/form-data; boundary={boundary}"),
                )
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let text = String::from_utf8_lossy(&body);
    assert_eq!(
        text,
        format!("Alice|photo.jpg|image/jpeg|{}", file_data.len())
    );
}

#[tokio::test]
async fn test_query_extractor_nested_struct() {
    #[derive(Deserialize)]
    struct FilterParams {
        filter: FilterInner,
    }
    #[derive(Deserialize)]
    struct FilterInner {
        status: String,
        role: String,
    }
    impl Sanitize for FilterParams {
        fn sanitize(&mut self) {}
    }

    async fn handler(modo::extractor::Query(p): modo::extractor::Query<FilterParams>) -> String {
        format!("{}|{}", p.filter.status, p.filter.role)
    }

    let registry = Registry::new();
    let app = Router::new()
        .route("/", get(handler))
        .with_state(registry.into_state());

    let response = app
        .oneshot(
            Request::builder()
                .uri("/?filter[status]=active&filter[role]=admin")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    assert_eq!(&body[..], b"active|admin");
}

#[tokio::test]
async fn test_multipart_request_vec_of_structs() {
    #[derive(Deserialize)]
    struct ContactRow {
        kind: String,
        value: String,
    }
    #[derive(Deserialize)]
    struct NewClientForm {
        name: String,
        contacts: Vec<ContactRow>,
    }
    impl Sanitize for NewClientForm {
        fn sanitize(&mut self) {
            modo::sanitize::trim(&mut self.name);
        }
    }

    async fn handler(
        modo::extractor::MultipartRequest(form, _): modo::extractor::MultipartRequest<
            NewClientForm,
        >,
    ) -> String {
        let rows = form
            .contacts
            .into_iter()
            .map(|c| format!("{}={}", c.kind, c.value))
            .collect::<Vec<_>>()
            .join(";");
        format!("{}|{}", form.name, rows)
    }

    let registry = Registry::new();
    let app = Router::new()
        .route("/", axum::routing::post(handler))
        .with_state(registry.into_state());

    let boundary = "----TestNestedBoundary";
    let body = format!(
        "--{boundary}\r\nContent-Disposition: form-data; name=\"name\"\r\n\r\nAcme\r\n\
         --{boundary}\r\nContent-Disposition: form-data; name=\"contacts[0][kind]\"\r\n\r\nemail\r\n\
         --{boundary}\r\nContent-Disposition: form-data; name=\"contacts[0][value]\"\r\n\r\na@b.com\r\n\
         --{boundary}\r\nContent-Disposition: form-data; name=\"contacts[1][kind]\"\r\n\r\nphone\r\n\
         --{boundary}\r\nContent-Disposition: form-data; name=\"contacts[1][value]\"\r\n\r\n555-0100\r\n\
         --{boundary}--\r\n"
    );

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/")
                .header(
                    "content-type",
                    format!("multipart/form-data; boundary={boundary}"),
                )
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    assert_eq!(&body[..], b"Acme|email=a@b.com;phone=555-0100");
}

#[tokio::test]
async fn test_query_extractor_percent_encoded_brackets() {
    #[derive(Deserialize)]
    struct EncodedFilterParams {
        filter: EncodedFilterInner,
    }
    #[derive(Deserialize)]
    struct EncodedFilterInner {
        status: String,
        role: String,
    }
    impl Sanitize for EncodedFilterParams {
        fn sanitize(&mut self) {}
    }

    async fn handler(
        modo::extractor::Query(p): modo::extractor::Query<EncodedFilterParams>,
    ) -> String {
        format!("{}|{}", p.filter.status, p.filter.role)
    }

    let registry = Registry::new();
    let app = Router::new()
        .route("/", get(handler))
        .with_state(registry.into_state());

    // %5B = '[', %5D = ']' — what URLSearchParams / axios default emit
    let response = app
        .oneshot(
            Request::builder()
                .uri("/?filter%5Bstatus%5D=active&filter%5Brole%5D=admin")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    assert_eq!(&body[..], b"active|admin");
}

#[tokio::test]
async fn test_form_request_vec_of_structs_percent_encoded_brackets() {
    #[derive(Deserialize)]
    struct EncodedContact {
        kind: String,
        value: String,
    }
    #[derive(Deserialize)]
    struct EncodedClientForm {
        contacts: Vec<EncodedContact>,
    }
    impl Sanitize for EncodedClientForm {
        fn sanitize(&mut self) {}
    }

    async fn handler(
        modo::extractor::FormRequest(form): modo::extractor::FormRequest<EncodedClientForm>,
    ) -> String {
        form.contacts
            .into_iter()
            .map(|c| format!("{}={}", c.kind, c.value))
            .collect::<Vec<_>>()
            .join(";")
    }

    let registry = Registry::new();
    let app = Router::new()
        .route("/", axum::routing::post(handler))
        .with_state(registry.into_state());

    // %5B = '[', %5D = ']'  — what real browsers send
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/")
                .header("content-type", "application/x-www-form-urlencoded")
                .body(Body::from(
                    "contacts%5B0%5D%5Bkind%5D=email&contacts%5B0%5D%5Bvalue%5D=a%40b.com\
                     &contacts%5B1%5D%5Bkind%5D=phone&contacts%5B1%5D%5Bvalue%5D=555-0100",
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    assert_eq!(&body[..], b"email=a@b.com;phone=555-0100");
}

use modo_upload::{FromMultipart, UploadedFile};

#[derive(FromMultipart, modo::Sanitize, modo::Validate)]
pub(crate) struct ProfileForm {
    #[upload(max_size = "5mb", accept = "image/*")]
    pub(crate) avatar: UploadedFile,

    #[clean(trim)]
    #[validate(required, min_length = 2)]
    pub(crate) name: String,

    #[clean(trim, normalize_email)]
    #[validate(required, email)]
    pub(crate) email: String,
}

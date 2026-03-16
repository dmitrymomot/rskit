use modo_upload::FromMultipart;

#[derive(FromMultipart)]
enum BadForm {
    A,
    B,
}

fn main() {}

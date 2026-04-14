use modo::auth::session::device::{parse_device_name, parse_device_type};

#[test]
fn chrome_on_macos() {
    let ua = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36";
    assert_eq!(parse_device_name(ua), "Chrome on macOS");
    assert_eq!(parse_device_type(ua), "desktop");
}

#[test]
fn safari_on_iphone() {
    let ua = "Mozilla/5.0 (iPhone; CPU iPhone OS 17_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.0 Mobile/15E148 Safari/604.1";
    assert_eq!(parse_device_name(ua), "Safari on iPhone");
    assert_eq!(parse_device_type(ua), "mobile");
}

#[test]
fn firefox_on_windows() {
    let ua = "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:120.0) Gecko/20100101 Firefox/120.0";
    assert_eq!(parse_device_name(ua), "Firefox on Windows");
    assert_eq!(parse_device_type(ua), "desktop");
}

#[test]
fn edge_on_windows() {
    let ua = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36 Edg/120.0.0.0";
    assert_eq!(parse_device_name(ua), "Edge on Windows");
    assert_eq!(parse_device_type(ua), "desktop");
}

#[test]
fn chrome_on_android_mobile() {
    let ua = "Mozilla/5.0 (Linux; Android 14; Pixel 8) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Mobile Safari/537.36";
    assert_eq!(parse_device_name(ua), "Chrome on Android");
    assert_eq!(parse_device_type(ua), "mobile");
}

#[test]
fn safari_on_ipad() {
    let ua = "Mozilla/5.0 (iPad; CPU OS 17_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.0 Mobile/15E148 Safari/604.1";
    assert_eq!(parse_device_name(ua), "Safari on iPad");
    assert_eq!(parse_device_type(ua), "tablet");
}

#[test]
fn chrome_on_linux() {
    let ua = "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36";
    assert_eq!(parse_device_name(ua), "Chrome on Linux");
    assert_eq!(parse_device_type(ua), "desktop");
}

#[test]
fn opera_on_macos() {
    let ua = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36 OPR/106.0.0.0";
    assert_eq!(parse_device_name(ua), "Opera on macOS");
    assert_eq!(parse_device_type(ua), "desktop");
}

#[test]
fn unknown_ua() {
    assert_eq!(parse_device_name("curl/7.88.1"), "Unknown on Unknown");
    assert_eq!(parse_device_type("curl/7.88.1"), "desktop");
}

#[test]
fn empty_ua() {
    assert_eq!(parse_device_name(""), "Unknown on Unknown");
    assert_eq!(parse_device_type(""), "desktop");
}

pub fn parse_device_name(user_agent: &str) -> String {
    let browser = parse_browser(user_agent);
    let os = parse_os(user_agent);
    format!("{browser} on {os}")
}

pub fn parse_device_type(user_agent: &str) -> String {
    let ua = user_agent.to_lowercase();
    if ua.contains("tablet") || ua.contains("ipad") {
        "tablet".to_string()
    } else if ua.contains("mobile")
        || ua.contains("iphone")
        || (ua.contains("android") && !ua.contains("tablet"))
    {
        "mobile".to_string()
    } else {
        "desktop".to_string()
    }
}

fn parse_browser(ua: &str) -> &str {
    if ua.contains("OPR/") || ua.contains("Opera") {
        "Opera"
    } else if ua.contains("Edg/") {
        "Edge"
    } else if ua.contains("Firefox/") {
        "Firefox"
    } else if ua.contains("Chromium/") {
        "Chromium"
    } else if ua.contains("Chrome/") {
        "Chrome"
    } else if ua.contains("Safari/") {
        "Safari"
    } else {
        "Unknown"
    }
}

fn parse_os(ua: &str) -> &str {
    if ua.contains("iPhone") {
        "iPhone"
    } else if ua.contains("iPad") {
        "iPad"
    } else if ua.contains("HarmonyOS") {
        "HarmonyOS"
    } else if ua.contains("Android") {
        "Android"
    } else if ua.contains("CrOS") {
        "ChromeOS"
    } else if ua.contains("Mac OS X") || ua.contains("Macintosh") || ua.contains("OS X") {
        "macOS"
    } else if ua.contains("Windows") {
        "Windows"
    } else if ua.contains("FreeBSD") {
        "FreeBSD"
    } else if ua.contains("OpenBSD") {
        "OpenBSD"
    } else if ua.contains("Linux") {
        "Linux"
    } else {
        "Unknown"
    }
}

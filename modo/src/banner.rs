use crate::config::ServerConfig;

const BANNER: &str = r#"
                      _
  _ __ ___   ___   __| | ___
 | '_ ` _ \ / _ \ / _` |/ _ \
 | | | | | | (_) | (_| | (_) |
 |_| |_| |_|\___/ \__,_|\___/
"#;

pub(crate) fn print(server_config: &ServerConfig, route_count: usize, module_count: usize) {
    let version = env!("CARGO_PKG_VERSION");
    let environment = &server_config.environment;
    let port = server_config.port;

    print!("{BANNER}");

    let module_suffix = if module_count > 0 {
        format!(" ({module_count} modules)")
    } else {
        String::new()
    };

    println!("    >> version:      {version}");
    println!("    >> environment:  {environment}");
    println!("    >> listening on: http://localhost:{port}");
    println!("    >> routes:       {route_count}{module_suffix}");
    println!();
}

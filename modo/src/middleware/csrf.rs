impl modo_csrf::CsrfState for crate::app::AppState {
    fn csrf_config(&self) -> modo_csrf::CsrfConfig {
        self.services
            .get::<modo_csrf::CsrfConfig>()
            .map(|c| (*c).clone())
            .unwrap_or_default()
    }

    fn csrf_secret(&self) -> &[u8] {
        self.server_config.secret_key.as_bytes()
    }
}

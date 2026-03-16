use std::sync::Arc;

use crate::{
    CachedTemplateProvider, EmailConfig, FilesystemProvider, LayoutEngine, Mailer, SenderProfile,
    TemplateProvider,
};

/// Create a [`Mailer`] using [`FilesystemProvider`] and the transport configured in `config`.
///
/// This is the standard entry point. Templates are loaded from `config.templates_path`.
pub fn mailer(config: &EmailConfig) -> Result<Mailer, modo::Error> {
    let fs_provider = FilesystemProvider::new(&config.templates_path);
    let provider: Arc<dyn TemplateProvider> = if config.cache_templates {
        Arc::new(CachedTemplateProvider::new(
            fs_provider,
            config.template_cache_size,
        ))
    } else {
        Arc::new(fs_provider)
    };
    mailer_with(config, provider)
}

/// Create a [`Mailer`] with a custom [`TemplateProvider`].
///
/// Use this when you want to load templates from a database, cache, or any
/// source other than the filesystem.
pub fn mailer_with(
    config: &EmailConfig,
    provider: Arc<dyn TemplateProvider>,
) -> Result<Mailer, modo::Error> {
    let transport = crate::transport::transport(config)?;
    let layout = Arc::new(LayoutEngine::try_new(&config.templates_path)?);
    let sender = SenderProfile {
        from_name: config.default_from_name.clone(),
        from_email: config.default_from_email.clone(),
        reply_to: config.default_reply_to.clone(),
    };
    Ok(Mailer::new(transport, provider, sender, layout))
}

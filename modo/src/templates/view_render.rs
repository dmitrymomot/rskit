use crate::templates::{TemplateContext, TemplateEngine, TemplateError};

/// Trait for types that can be rendered by `ViewRenderer`.
///
/// Implemented by `#[modo::view]` structs (via macro) and tuples of views.
/// Tuples render each element and concatenate the HTML.
pub trait ViewRender {
    /// Whether this view has a dual template (htmx = "...").
    /// Used by ViewRenderer to add `Vary: HX-Request` header.
    fn has_dual_template(&self) -> bool {
        false
    }

    /// Render this view to an HTML string.
    fn render_with(
        &self,
        engine: &TemplateEngine,
        context: &TemplateContext,
        is_htmx: bool,
    ) -> Result<String, TemplateError>;
}

macro_rules! impl_view_render_tuple {
    ($($idx:tt : $T:ident),+) => {
        impl<$($T: ViewRender),+> ViewRender for ($($T,)+) {
            fn has_dual_template(&self) -> bool {
                $(self.$idx.has_dual_template() ||)+ false
            }

            fn render_with(
                &self,
                engine: &TemplateEngine,
                context: &TemplateContext,
                is_htmx: bool,
            ) -> Result<String, TemplateError> {
                let mut html = String::new();
                $(html.push_str(&self.$idx.render_with(engine, context, is_htmx)?);)+
                Ok(html)
            }
        }
    };
}

impl_view_render_tuple!(0: A, 1: B);
impl_view_render_tuple!(0: A, 1: B, 2: C);
impl_view_render_tuple!(0: A, 1: B, 2: C, 3: D);
impl_view_render_tuple!(0: A, 1: B, 2: C, 3: D, 4: E);

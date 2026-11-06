//! Headless probes; assertions never run inside a native event loop.
mod list_probe;
use ryne_extensions::*;
use ryne_session::{EditorSession, SessionServices};
use std::sync::Arc;

struct EmptyControls;
impl RenderActionProvider for EmptyControls {
    fn id(&self) -> &'static str {
        "empty"
    }
    fn commands(&self) -> &'static [&'static str] {
        &[]
    }
    fn controls(&self, _: &EditingState) -> Result<Vec<RenderControl>, String> {
        Ok(vec![])
    }
    fn plan(&self, _: &EditingState, _: &RenderActionRequest) -> Result<CommandPlan, String> {
        Ok(CommandPlan::Consume)
    }
}
fn main() {
    list_probe::audit();
    let features = ryne_features::MarkdownFeatures::new(
        Arc::new(ryne_services::math::FormularyRenderer::default()),
        vec![],
    )
    .unwrap()
    .with_render_action_provider(Arc::new(EmptyControls))
    .unwrap();
    let doc = EditorSession::new(
        "plain",
        SessionServices {
            features: Arc::new(features),
            files: Arc::new(ryne_services::files::AtomicFileStore),
        },
    );
    let first = doc.cached_render_control_batch(&doc).unwrap().controls;
    let second = doc.cached_render_control_batch(&doc).unwrap().controls;
    assert!(first.is_empty() && second.is_empty());
    assert!(!Arc::ptr_eq(&first, &second));
    assert!(doc.export_snapshot().render().is_err());
    println!("empty_extension unchanged_controls_reuse=false export_rejected=true");

    #[cfg(unix)]
    {
        let directory = std::path::PathBuf::from(std::env::args().nth(1).unwrap());
        let target = directory.join("target.md");
        let alias = directory.join("alias.md");
        std::fs::write(&target, b"original").unwrap();
        std::os::unix::fs::symlink(&target, &alias).unwrap();
        let store = ryne_services::files::AtomicFileStore;
        assert_eq!(store.read(&alias).unwrap(), "original");
        store
            .write(&alias, &ryne_parser::Source::new("edited"))
            .unwrap();
        let is_link = std::fs::symlink_metadata(&alias)
            .unwrap()
            .file_type()
            .is_symlink();
        assert!(is_link);
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "edited");
        assert_eq!(std::fs::read_to_string(&alias).unwrap(), "edited");
        println!("atomic_save symlink_preserved=true target_updated=true alias_contains_edit=true");
    }
}
